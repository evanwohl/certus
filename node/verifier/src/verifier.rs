use certus_common::{
    contracts::EscrowClient,
    crypto::sha256,
    types::{JobSpec, VerificationResult},
};
use ethers::{
    middleware::SignerMiddleware,
    providers::{Provider, Http},
    signers::{LocalWallet, Signer},
    types::{Address, H256, U256},
};
use wasmtime::*;
use anyhow::Result;
use tracing::{info, error, warn};
use std::str::FromStr;
use std::sync::Arc;
use hex;

/// Verifier node
pub struct VerifierNode {
    escrow: EscrowClient,
    engine: Engine,
    address: Address,
}

impl VerifierNode {
    /// Initialize verifier
    pub async fn new(
        rpc_url: &str,
        private_key: &str,
        contract_addr: &str,
    ) -> Result<Self> {
        let provider = Provider::<Http>::try_from(rpc_url)?;
        let wallet = private_key.parse::<LocalWallet>()?.with_chain_id(421614u64);
        let address = wallet.address();

        let client = Arc::new(SignerMiddleware::new(
            provider,
            wallet,
        ));

        let escrow = EscrowClient::new(
            Address::from_str(contract_addr)?,
            client,
        );

        // Deterministic Wasm engine
        let mut config = Config::new();
        config.wasm_threads(false);
        config.wasm_simd(false);
        config.cranelift_nan_canonicalization(true);
        config.consume_fuel(true);

        let engine = Engine::new(&config)?;

        Ok(Self {
            escrow,
            engine,
            address,
        })
    }

    /// Main verification loop
    pub async fn run(&self) -> Result<()> {
        info!("Verifier running: {}", self.address);

        // Spawn heartbeat task
        let escrow = self.escrow.clone();
        tokio::spawn(async move {
            loop {
                // Send heartbeat every 8 minutes
                tokio::time::sleep(tokio::time::Duration::from_secs(480)).await;

                if let Err(e) = escrow.heartbeat().await {
                    error!("Heartbeat failed: {}", e);
                }
            }
        });

        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

            let receipts = self.escrow.get_pending_receipts().await?;

            for (job, receipt_hash) in receipts {
                info!("Verifying job {:?}", job.job_id);

                let wasm = self.fetch_wasm(&job.wasm_hash).await?;
                let input = self.fetch_input(&job.input_hash).await?;

                match self.verify_receipt(&job, receipt_hash, &wasm, &input).await {
                    Ok(VerificationResult::Valid) => {
                        info!("Receipt valid");
                    }
                    Ok(VerificationResult::Fraud { claimed: _, computed: _ }) => {
                        warn!("Fraud detected, submitting proof");

                        // Get the actual output for fraud proof
                        let actual_output = self.execute_wasm(
                            &wasm,
                            &input,
                            job.fuel_limit,
                            job.mem_limit,
                        )?;

                        self.submit_fraud(
                            H256::from(job.job_id),
                            &wasm,
                            &input,
                            &actual_output,
                        ).await?;
                    }
                    Ok(VerificationResult::Error(msg)) => {
                        error!("Verification error: {}", msg);
                    }
                    Err(e) => {
                        error!("Verification failed: {}", e);
                    }
                }
            }
        }
    }

    /// Verify execution receipt
    pub async fn verify_receipt(
        &self,
        job: &JobSpec,
        claimed_output_hash: H256,
        wasm: &[u8],
        input: &[u8],
    ) -> Result<VerificationResult> {
        info!("Verifying job: {:?}", job.job_id);

        // Validate hashes
        if sha256(wasm) != H256::from(job.wasm_hash) {
            return Ok(VerificationResult::Error("Wasm hash mismatch".into()));
        }

        if sha256(input) != H256::from(job.input_hash) {
            return Ok(VerificationResult::Error("Input hash mismatch".into()));
        }

        // Re-execute
        let result = self.execute_wasm(
            wasm,
            input,
            job.fuel_limit,
            job.mem_limit,
        )?;

        let computed_hash = sha256(&result);

        if computed_hash == claimed_output_hash {
            Ok(VerificationResult::Valid)
        } else {
            warn!("Fraud detected: job {:?}", job.job_id);
            Ok(VerificationResult::Fraud {
                claimed: claimed_output_hash,
                computed: computed_hash,
            })
        }
    }

    /// Execute Wasm for verification
    fn execute_wasm(
        &self,
        wasm: &[u8],
        input: &[u8],
        fuel_limit: u64,
        mem_limit: u64,
    ) -> Result<Vec<u8>> {
        let module = Module::new(&self.engine, wasm)?;
        let mut store = Store::new(&self.engine, ());

        store.set_fuel(fuel_limit)?;

        let mut linker = Linker::new(&self.engine);
        let memory_ty = MemoryType::new(1, Some((mem_limit / 65536) as u32));
        let memory = Memory::new(&mut store, memory_ty)?;
        linker.define(&mut store, "env", "memory", memory)?;

        let instance = linker.instantiate(&mut store, &module)?;
        let main = instance.get_typed_func::<(i32, i32), i32>(&mut store, "main")?;

        memory.write(&mut store, 0, input)?;
        let output_ptr = main.call(&mut store, (0, input.len() as i32))?;

        let mut output = vec![0u8; 32];
        memory.read(&store, output_ptr as usize, &mut output)?;

        Ok(output)
    }

    /// Submit fraud proof
    pub async fn submit_fraud(
        &self,
        job_id: H256,
        wasm: &[u8],
        input: &[u8],
        claimed_output: &[u8],
    ) -> Result<()> {
        // MEV protection: compute commitment
        let nonce = U256::from(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs());
        let commitment_data = [
            job_id.as_bytes(),
            &sha256(wasm).0,
            &sha256(input).0,
            &sha256(claimed_output).0,
            &{
                let mut bytes = [0u8; 32];
                nonce.to_big_endian(&mut bytes);
                bytes
            },
            self.address.as_bytes(),
        ].concat();

        let commitment = sha256(&commitment_data);

        self.escrow.submit_fraud(
            job_id,
            commitment,
            wasm,
            input,
            claimed_output,
            nonce,
        ).await?;

        info!("Fraud proof submitted for job: {:?}", job_id);

        Ok(())
    }

    /// Fetch Wasm bytecode from distributed storage
    async fn fetch_wasm(&self, hash: &[u8; 32]) -> Result<Vec<u8>> {
        let hash_hex = hex::encode(hash);

        // Query on-chain storage first (for modules <24KB)
        let stored = self.escrow.get_stored_wasm(hash).await?;
        if !stored.is_empty() {
            // Verify integrity
            if sha256(&stored).0 != *hash {
                return Err(anyhow::anyhow!("Wasm integrity check failed"));
            }
            return Ok(stored);
        }

        // Fallback to IPFS for larger modules
        let ipfs_url = format!("https://ipfs.io/ipfs/{}", hash_hex);
        let response = reqwest::get(&ipfs_url).await?;
        let wasm = response.bytes().await?.to_vec();

        // Verify integrity
        if sha256(&wasm).0 != *hash {
            return Err(anyhow::anyhow!("Wasm integrity check failed"));
        }

        Ok(wasm)
    }

    /// Fetch input data from distributed storage
    async fn fetch_input(&self, hash: &[u8; 32]) -> Result<Vec<u8>> {
        let hash_hex = hex::encode(hash);

        // Query on-chain storage first (for inputs <100KB)
        let stored = self.escrow.get_stored_input(hash).await?;
        if !stored.is_empty() {
            // Verify integrity
            if sha256(&stored).0 != *hash {
                return Err(anyhow::anyhow!("Input integrity check failed"));
            }
            return Ok(stored);
        }

        // Fallback to Arweave for larger inputs
        let arweave_url = format!("https://arweave.net/{}", hash_hex);
        let response = reqwest::get(&arweave_url).await?;
        let input = response.bytes().await?.to_vec();

        // Verify integrity
        if sha256(&input).0 != *hash {
            return Err(anyhow::anyhow!("Input integrity check failed"));
        }

        Ok(input)
    }
}
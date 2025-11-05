use certus_common::{
    contracts::EscrowClient,
    crypto::{sha256, sign_receipt},
    types::{JobSpec, ExecReceipt},
};
use crate::sandbox::WasmSandbox;
use ethers::{
    middleware::SignerMiddleware,
    providers::{Provider, Http},
    signers::{LocalWallet, Signer},
    types::{Address, H256, U256},
};
use ed25519_dalek::SigningKey;
use anyhow::Result;
use tracing::info;
use std::str::FromStr;
use std::sync::Arc;
use hex;

/// Executor node
pub struct ExecutorNode {
    escrow: EscrowClient,
    sandbox: WasmSandbox,
    signing_key: SigningKey,
    address: Address,
    max_collateral: U256,
}

impl ExecutorNode {
    /// Initialize executor
    pub async fn new(
        rpc_url: &str,
        private_key: &str,
        contract_addr: &str,
    ) -> Result<Self> {
        // Setup provider and wallet
        let provider = Provider::<Http>::try_from(rpc_url)?;
        let wallet = private_key.parse::<LocalWallet>()?.with_chain_id(421614u64);
        let address = wallet.address();

        let client = Arc::new(SignerMiddleware::new(
            provider,
            wallet, // Already has chain_id
        ));

        let escrow = EscrowClient::new(
            Address::from_str(contract_addr)?,
            client,
        );

        // Generate signing key (deterministic from private key for now)
        let mut seed = [0u8; 32];
        seed[..20].copy_from_slice(&address.0);
        let signing_key = SigningKey::from_bytes(&seed);

        let sandbox = WasmSandbox::new()?;

        Ok(Self {
            escrow,
            sandbox,
            signing_key,
            address,
            max_collateral: U256::from(10000) * U256::exp10(6), // $10k USDC
        })
    }

    /// Main execution loop
    pub async fn run(&self) -> Result<()> {
        info!("Executor running: {}", self.address);
        info!("Max collateral: {} USDC", self.max_collateral / U256::exp10(6));

        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

            let jobs = self.escrow.get_pending_jobs().await?;

            for job in jobs {
                info!("Found job: {:?}", job.job_id);

                let required_collateral = match job.pay_amt.checked_mul(U256::from(2)) {
                    Some(c) => c,
                    None => {
                        info!("Collateral overflow for job {:?}", job.job_id);
                        continue;
                    }
                };
                if required_collateral > self.max_collateral {
                    info!("Job requires {} collateral, max is {}", required_collateral, self.max_collateral);
                    continue;
                }

                match self.escrow.accept_job(
                    H256::from(job.job_id),
                    job.pay_amt,
                    job.pay_token,
                ).await {
                    Ok(_) => {
                        info!("Accepted job {:?}", job.job_id);

                        match self.execute_job(&job).await {
                            Ok(receipt) => {
                                info!("Job executed, output: {:?}", receipt.output_hash);
                                self.escrow.submit_receipt(
                                    H256::from(job.job_id),
                                    H256::from(receipt.output_hash),
                                    &receipt.executor_sig,
                                ).await?;
                            }
                            Err(e) => {
                                info!("Execution failed: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        info!("Failed to accept job: {}", e);
                    }
                }
            }
        }
    }

    /// Execute WebAssembly job and generate signed receipt
    pub async fn execute_job(
        &self,
        job: &JobSpec,
    ) -> Result<ExecReceipt> {
        info!("Executing job: {:?}", job.job_id);

        // Retrieve data from distributed storage
        let wasm = self.fetch_wasm(&job.wasm_hash).await?;
        let input = self.fetch_input(&job.input_hash).await?;

        // Validate module constraints
        self.sandbox.validate(&wasm)?;

        // Check collateral with overflow protection
        let required = job.pay_amt.checked_mul(U256::from(2))
            .ok_or_else(|| anyhow::anyhow!("Collateral overflow"))?;
        if required > self.max_collateral {
            return Err(anyhow::anyhow!("Collateral {} exceeds limit {}", required, self.max_collateral));
        }

        // Accept on-chain
        self.escrow.accept_job(
            H256::from(job.job_id),
            job.pay_amt,
            job.pay_token,
        ).await?;

        // Execute with resource constraints
        let result = self.sandbox.execute(
            &wasm,
            &input,
            job.fuel_limit,
            job.mem_limit,
        )?;

        let output_hash = sha256(&result.output);

        // Sign receipt
        let signature = sign_receipt(
            &self.signing_key,
            &H256::from(job.job_id),
            &output_hash,
        );

        let receipt = ExecReceipt {
            job_id: job.job_id,
            output_hash: output_hash.0,
            executor_sig: signature,
            executor_addr: self.address,
            collateral: required,
        };

        // Submit receipt
        self.escrow.submit_receipt(
            H256::from(job.job_id),
            H256::from(output_hash.0),
            &signature,
        ).await?;

        info!("Receipt submitted: {:?}", output_hash);

        Ok(receipt)
    }

    /// Fetch Wasm bytecode from distributed storage
    async fn fetch_wasm(&self, hash: &[u8; 32]) -> Result<Vec<u8>> {
        let hash_hex = hex::encode(hash);

        // Query on-chain storage first (modules <24KB)
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

        // Query on-chain storage first (inputs <100KB)
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
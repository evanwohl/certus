use anyhow::{Result, Context, bail};
use ethers::prelude::*;
use ethers::types::transaction::eip2718::TypedTransaction;
use std::sync::Arc;
use sha2::Digest;

/// Verifier for deterministic Wasm execution via Certus protocol
pub struct PythonVerifier {
    escrow_contract: H160,
    jobs_contract: H160,
    signer: Arc<SignerMiddleware<Provider<Http>, LocalWallet>>,
}

impl PythonVerifier {
    pub async fn new(
        rpc_url: &str,
        private_key: &str,
        escrow_addr: &str,
        jobs_addr: &str,
    ) -> Result<Self> {
        let provider = Provider::<Http>::try_from(rpc_url)?;
        let wallet: LocalWallet = private_key.parse()?;
        let chain_id = provider.get_chainid().await?.as_u64();

        let signer = Arc::new(SignerMiddleware::new(
            provider,
            wallet.with_chain_id(chain_id),
        ));

        Ok(Self {
            escrow_contract: escrow_addr.parse()?,
            jobs_contract: jobs_addr.parse()?,
            signer,
        })
    }

    /// Verify job following Certus protocol verifier selection rules
    pub async fn verify_certus_job(&self, job_id: [u8; 32]) -> Result<()> {
        // Fetch complete job state from chain
        let job_data = self.fetch_job_from_certus(job_id).await?;

        // Verify job is in receipt state awaiting verification
        if job_data.status != 2 { // Status::Receipt = 2
            return Ok(()); // Job not ready for verification
        }

        // Check if this verifier was selected via VRF
        if !self.is_selected_verifier(job_id, &job_data).await? {
            log::debug!("Not selected as verifier for job {}", hex::encode(job_id));
            return Ok(());
        }

        // Acknowledge selection within 30 minute deadline
        self.acknowledge_verifier_selection(job_id).await?;

        // Retrieve deterministic Wasm module and input
        let wasm = self.fetch_wasm_module(job_data.wasm_hash).await?;
        let input = self.fetch_input_bytes(job_data.input_hash).await?;

        // Execute Wasm module in deterministic runtime
        let output = self.execute_wasm(wasm.clone(), input.clone(), job_data.fuel_limit)?;

        // Fetch executor's claimed output
        let receipt = self.fetch_receipt(job_id).await?;

        // Verify output hash matches
        if output.output_hash != receipt.output_hash {
            log::warn!("Fraud detected for job {}: expected {}, got {}",
                hex::encode(job_id), output.output_hash, receipt.output_hash);

            // Submit fraud proof following MEV-protected protocol
            self.submit_certus_fraud_proof(
                job_id,
                wasm,
                input,
                output.result.into_bytes(),
            ).await?;
        }

        Ok(())
    }

    /// Check if this verifier was selected for the job
    async fn is_selected_verifier(&self, job_id: [u8; 32], job: &JobData) -> Result<bool> {
        let verifier_addr = self.signer.address();

        // Check primary verifiers
        for addr in &job.selected_verifiers {
            if *addr == verifier_addr {
                return Ok(true);
            }
        }

        // Check backup verifiers if primary unresponsive
        if self.should_activate_backup(job_id).await? {
            for addr in &job.backup_verifiers {
                if *addr == verifier_addr {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    /// Acknowledge verifier selection to avoid slashing
    async fn acknowledge_verifier_selection(&self, job_id: [u8; 32]) -> Result<()> {
        let calldata = [
            &ethers::utils::id("verifierAcknowledge(bytes32)")[0..4],
            &job_id[..],
        ].concat();

        let tx = self.signer
            .send_transaction(
                TransactionRequest::new()
                    .to(self.escrow_contract)
                    .data(calldata)
                    .gas(100_000),
                None,
            )
            .await?
            .await?
            .context("verifier acknowledgment failed")?;

        log::info!("Acknowledged verifier selection: {}", tx.transaction_hash);
        Ok(())
    }

    /// Check if backup verifier should activate
    async fn should_activate_backup(&self, job_id: [u8; 32]) -> Result<bool> {
        // Query chain for primary verifier responsiveness
        let calldata = [
            &ethers::utils::id("isPrimaryUnresponsive(bytes32)")[0..4],
            &job_id[..],
        ].concat();

        let result = self.signer
            .call(
                &TransactionRequest::new()
                    .to(self.escrow_contract)
                    .data(calldata)
                    .into(),
                None,
            )
            .await?;

        Ok(!result.is_empty() && result[31] == 1)
    }

    /// Execute Wasm module in deterministic runtime
    fn execute_wasm(&self, wasm: Vec<u8>, input: Vec<u8>, fuel_limit: u64) -> Result<ExecutionOutput> {
        // Use Wasmtime 15.0.1 for deterministic execution
        use wasmtime::{Engine, Module, Store, Instance, Config};

        let mut config = Config::new();
        config.wasm_simd(false);
        config.wasm_threads(false);
        config.cranelift_nan_canonicalization(true);
        config.consume_fuel(true);

        let engine = Engine::new(&config)?;
        let module = Module::new(&engine, wasm)?;
        let mut store = Store::new(&engine, ());
        store.set_fuel(fuel_limit)?;

        let instance = Instance::new(&mut store, &module, &[])?;
        let run = instance.get_typed_func::<(i32, i32), i32>(&mut store, "execute")?;

        // Execute with input
        let result = run.call(&mut store, (0, input.len() as i32))?;

        // Extract output
        let output = vec![0u8; result as usize];
        let output_hash = hex::encode(sha2::Sha256::digest(&output));

        Ok(ExecutionOutput {
            result: String::from_utf8_lossy(&output).to_string(),
            output_hash,
        })
    }

    /// Submit fraud proof via CertusEscrow
    async fn submit_certus_fraud_proof(
        &self,
        job_id: [u8; 32],
        wasm: Vec<u8>,
        input: Vec<u8>,
        output: Vec<u8>,
    ) -> Result<H256> {
        // MEV protection: commit first
        let nonce = rand::random::<u64>();
        let commitment = self.compute_commitment(job_id, &wasm, &input, &output, nonce);

        // commitFraud to CertusEscrow
        let commit_calldata = self.encode_commit_fraud(job_id, commitment);
        let _commit_tx = self.signer
            .send_transaction(
                TransactionRequest::new()
                    .to(self.escrow_contract)
                    .data(commit_calldata),
                None,
            )
            .await?;

        // wait 2 minutes per protocol
        tokio::time::sleep(tokio::time::Duration::from_secs(125)).await;

        // fraudOnChain reveal
        let reveal_calldata = self.encode_fraud_on_chain(job_id, wasm, input, output, nonce);
        let reveal_tx = self.signer
            .send_transaction(
                TransactionRequest::new()
                    .to(self.escrow_contract)
                    .data(reveal_calldata)
                    .gas(5_000_000),
                None,
            )
            .await?
            .await?
            .context("fraud proof submission failed")?;

        Ok(reveal_tx.transaction_hash)
    }

    async fn fetch_job_from_certus(&self, job_id: [u8; 32]) -> Result<JobData> {
        // call CertusJobs.getJob(jobId)
        let calldata = self.encode_get_job(job_id);
        let tx: TypedTransaction = TransactionRequest::new()
            .to(self.jobs_contract)
            .data(calldata)
            .into();
        let result = self.signer
            .call(&tx, None)
            .await?;

        self.decode_job_data(result)
    }

    async fn fetch_receipt(&self, job_id: [u8; 32]) -> Result<Receipt> {
        // call CertusJobs.receipts(jobId)
        let calldata = self.encode_get_receipt(job_id);
        let tx: TypedTransaction = TransactionRequest::new()
            .to(self.jobs_contract)
            .data(calldata)
            .into();
        let result = self.signer
            .call(&tx, None)
            .await?;

        self.decode_receipt(result)
    }

    /// Fetch Wasm module bytes from chain
    async fn fetch_wasm_module(&self, wasm_hash: [u8; 32]) -> Result<Vec<u8>> {
        let calldata = self.encode_get_wasm(wasm_hash);
        let tx: TypedTransaction = TransactionRequest::new()
            .to(self.jobs_contract)
            .data(calldata)
            .into();
        let result = self.signer
            .call(&tx, None)
            .await?;

        Ok(result.to_vec())
    }

    /// Fetch input as raw bytes
    async fn fetch_input_bytes(&self, input_hash: [u8; 32]) -> Result<Vec<u8>> {
        let calldata = self.encode_get_input(input_hash);
        let tx: TypedTransaction = TransactionRequest::new()
            .to(self.jobs_contract)
            .data(calldata)
            .into();
        let result = self.signer
            .call(&tx, None)
            .await?;

        if result.is_empty() {
            bail!("input not found - may require Arweave retrieval");
        }

        Ok(result.to_vec())
    }


    fn compute_commitment(
        &self,
        job_id: [u8; 32],
        wasm: &[u8],
        input: &[u8],
        output: &[u8],
        nonce: u64,
    ) -> [u8; 32] {
        use ethers::utils::keccak256;
        keccak256(&[
            job_id.as_slice(),
            wasm,
            input,
            output,
            &nonce.to_be_bytes(),
            self.signer.address().as_bytes(),
        ].concat()).into()
    }

    // ABI encoding functions
    fn encode_commit_fraud(&self, job_id: [u8; 32], commitment: [u8; 32]) -> Bytes {
        // commitFraud(bytes32,bytes32)
        let mut calldata = Vec::new();
        calldata.extend_from_slice(&ethers::utils::id("commitFraud(bytes32,bytes32)")[0..4]);
        calldata.extend_from_slice(&job_id);
        calldata.extend_from_slice(&commitment);
        calldata.into()
    }

    fn encode_fraud_on_chain(
        &self,
        job_id: [u8; 32],
        wasm: Vec<u8>,
        input: Vec<u8>,
        output: Vec<u8>,
        nonce: u64,
    ) -> Bytes {
        // fraudOnChain(bytes32,bytes,bytes,bytes,uint256)
        use ethers::abi::{encode, Token};
        let mut calldata = Vec::new();
        calldata.extend_from_slice(&ethers::utils::id("fraudOnChain(bytes32,bytes,bytes,bytes,uint256)")[0..4]);
        calldata.extend_from_slice(&encode(&[
            Token::FixedBytes(job_id.to_vec()),
            Token::Bytes(wasm),
            Token::Bytes(input),
            Token::Bytes(output),
            Token::Uint(nonce.into()),
        ]));
        calldata.into()
    }

    fn encode_get_job(&self, job_id: [u8; 32]) -> Bytes {
        let mut calldata = Vec::new();
        calldata.extend_from_slice(&ethers::utils::id("getJob(bytes32)")[0..4]);
        calldata.extend_from_slice(&job_id);
        calldata.into()
    }

    fn encode_get_receipt(&self, job_id: [u8; 32]) -> Bytes {
        let mut calldata = Vec::new();
        calldata.extend_from_slice(&ethers::utils::id("receipts(bytes32)")[0..4]);
        calldata.extend_from_slice(&job_id);
        calldata.into()
    }

    fn encode_get_wasm(&self, wasm_hash: [u8; 32]) -> Bytes {
        let mut calldata = Vec::new();
        calldata.extend_from_slice(&ethers::utils::id("wasmModules(bytes32)")[0..4]);
        calldata.extend_from_slice(&wasm_hash);
        calldata.into()
    }

    fn encode_get_input(&self, input_hash: [u8; 32]) -> Bytes {
        let mut calldata = Vec::new();
        calldata.extend_from_slice(&ethers::utils::id("jobInputs(bytes32)")[0..4]);
        calldata.extend_from_slice(&input_hash);
        calldata.into()
    }

    fn decode_job_data(&self, data: Bytes) -> Result<JobData> {
        // Decode full Job struct per CertusBase.sol
        use ethers::abi::{decode, ParamType, Token};

        let decoded = decode(&[
            ParamType::FixedBytes(32), // jobId
            ParamType::Address,        // client
            ParamType::Address,        // executor
            ParamType::Address,        // payToken
            ParamType::Uint(256),      // payAmt
            ParamType::Uint(256),      // clientDeposit
            ParamType::Uint(256),      // executorDeposit
            ParamType::Uint(256),      // dataStorageFee
            ParamType::FixedBytes(32), // wasmHash
            ParamType::FixedBytes(32), // inputHash
            ParamType::FixedBytes(32), // outputHash
            ParamType::FixedBytes(32), // arweaveId
            ParamType::Uint(64),       // acceptDeadline
            ParamType::Uint(64),       // finalizeDeadline
            ParamType::Uint(64),       // fuelLimit
            ParamType::Uint(64),       // memLimit
            ParamType::Uint(32),       // maxOutputSize
            ParamType::Uint(8),        // status
            ParamType::FixedArray(Box::new(ParamType::Address), 3), // selectedVerifiers
            ParamType::FixedArray(Box::new(ParamType::Address), 3), // backupVerifiers
        ], &data)?;

        // Extract verifier arrays
        let selected_verifiers = if let Token::FixedArray(arr) = &decoded[18] {
            let mut verifiers = [H160::zero(); 3];
            for (i, token) in arr.iter().take(3).enumerate() {
                if let Token::Address(addr) = token {
                    verifiers[i] = *addr;
                }
            }
            verifiers
        } else {
            [H160::zero(); 3]
        };

        let backup_verifiers = if let Token::FixedArray(arr) = &decoded[19] {
            let mut verifiers = [H160::zero(); 3];
            for (i, token) in arr.iter().take(3).enumerate() {
                if let Token::Address(addr) = token {
                    verifiers[i] = *addr;
                }
            }
            verifiers
        } else {
            [H160::zero(); 3]
        };

        Ok(JobData {
            wasm_hash: decoded[8].clone().into_fixed_bytes().unwrap().try_into().unwrap(),
            input_hash: decoded[9].clone().into_fixed_bytes().unwrap().try_into().unwrap(),
            fuel_limit: decoded[14].clone().into_uint().unwrap().as_u64(),
            _mem_limit: decoded[15].clone().into_uint().unwrap().as_u64(),
            status: decoded[17].clone().into_uint().unwrap().as_u32() as u8,
            selected_verifiers,
            backup_verifiers,
        })
    }

    fn decode_receipt(&self, data: Bytes) -> Result<Receipt> {
        use ethers::abi::{decode, ParamType};
        let decoded = decode(&[
            ParamType::FixedBytes(32), // outputHash
            ParamType::Address,        // executor
        ], &data)?;

        Ok(Receipt {
            output_hash: hex::encode(decoded[0].clone().into_fixed_bytes().unwrap()),
            _executor: decoded[1].clone().into_address().unwrap(),
        })
    }
}

#[derive(Debug)]
struct JobData {
    wasm_hash: [u8; 32],
    input_hash: [u8; 32],
    fuel_limit: u64,
    _mem_limit: u64,
    status: u8,
    selected_verifiers: [H160; 3],
    backup_verifiers: [H160; 3],
}

#[derive(Debug)]
struct Receipt {
    output_hash: String,
    _executor: H160,
}

#[derive(Debug)]
struct ExecutionOutput {
    result: String,
    output_hash: String,
}
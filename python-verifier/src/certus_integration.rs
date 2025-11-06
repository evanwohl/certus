use anyhow::{Result, Context, bail};
use ethers::prelude::*;
use ethers::abi::{encode, decode, Token, ParamType};
use std::sync::{Arc, Mutex};
use crate::PythonExecutor;
use crate::reliability::{retry_with_backoff, RetryConfig, validate_address};

/// Integrates Python execution with Certus protocol contracts
pub struct CertusIntegration {
    executor: Arc<Mutex<PythonExecutor>>,
    pub escrow_contract: H160,
    pub jobs_contract: H160,
    provider: Arc<Provider<Http>>,
    signer: Arc<SignerMiddleware<Provider<Http>, LocalWallet>>,
}

impl CertusIntegration {
    pub async fn new(
        executor: Arc<Mutex<PythonExecutor>>,
        rpc_url: &str,
        private_key: &str,
        escrow_addr: &str,
        jobs_addr: &str,
    ) -> Result<Self> {
        // validate addresses
        validate_address(escrow_addr)?;
        validate_address(jobs_addr)?;

        let provider = Provider::<Http>::try_from(rpc_url)
            .context("invalid RPC URL")?;

        let wallet: LocalWallet = private_key.parse()
            .context("invalid private key")?;

        // get chain ID with retry
        let chain_id = retry_with_backoff(
            || async { provider.get_chainid().await.map_err(Into::into) },
            &RetryConfig::default(),
        ).await?.as_u64();

        let signer = Arc::new(SignerMiddleware::new(
            provider.clone(),
            wallet.with_chain_id(chain_id),
        ));

        Ok(Self {
            executor,
            escrow_contract: escrow_addr.parse()?,
            jobs_contract: jobs_addr.parse()?,
            provider: Arc::new(provider),
            signer,
        })
    }

    /// Submit Python job through CertusJobs contract
    pub async fn create_python_job(
        &self,
        python_code: &str,
        input: &str,
        payment: U256,
        pay_token: H160, // USDC/USDT/DAI address
    ) -> Result<H256> {
        // Validate payment amount (assuming 6 decimals for USDC)
        if payment < U256::from(5_000_000u128) { // $5 minimum
            bail!("payment too low: minimum $5 USDC");
        }

        // Compile Python to Wasm with embedded interpreter
        let wasm_bytes = self.compile_python_to_wasm(python_code).await?;

        // Verify size limit
        if wasm_bytes.len() > 24 * 1024 {
            bail!("wasm exceeds 24KB limit");
        }

        let wasm_hash = self.hash_bytes(&wasm_bytes);

        // prepare and validate input
        let input_bytes = input.as_bytes();
        if input_bytes.len() > 100 * 1024 {
            bail!("input exceeds 100KB limit");
        }

        let input_hash = self.hash_bytes(input_bytes);

        // generate job ID
        let job_id = self.compute_job_id(wasm_hash, input_hash, self.signer.address());

        // calculate client deposit (5% of payment, min $5, max $1000)
        let client_deposit = self.calculate_client_deposit(payment);
        let total_payment = payment + client_deposit;

        // approve token transfer
        self.approve_token(pay_token, self.jobs_contract, total_payment).await?;

        // encode createJob call
        let job_data = self.encode_create_job(
            job_id,
            wasm_hash,
            input_hash,
            pay_token,
            payment,
            3600, // accept window
            3600, // challenge window
            100_000, // fuel limit
            1_000_000, // mem limit
            1024 * 100, // max output size
        )?;

        // submit with retry
        let signer = self.signer.clone();
        let jobs_contract = self.jobs_contract;

        let tx = retry_with_backoff(
            || async {
                let pending_tx = signer
                    .send_transaction(
                        TransactionRequest::new()
                            .to(jobs_contract)
                            .data(job_data.clone())
                            .gas(500_000),
                        None,
                    )
                    .await?;

                let receipt = pending_tx.await?
                    .context("transaction failed")?;

                Ok(receipt)
            },
            &RetryConfig::default(),
        ).await?;

        Ok(tx.transaction_hash)
    }

    /// Execute job as executor following Certus protocol flow
    pub async fn execute_job(&self, job_id: [u8; 32]) -> Result<ExecutionResult> {
        // Step 1: Fetch job details from chain
        let job = self.fetch_job_from_chain(job_id).await?;

        // Step 2: Accept job by depositing 2x collateral
        let accept_tx = self.accept_job(job_id, job.pay_token, job.pay_amount).await?;
        log::info!("Job accepted with 2x collateral: {}", accept_tx);

        // Step 3: Retrieve wasm and input data
        let wasm = self.fetch_wasm(job.wasm_hash).await?;
        let input = self.fetch_input(job_id).await?;

        // Execute with mutex lock
        let output = self.executor.lock().unwrap().execute(
            &String::from_utf8(wasm)?,
            &String::from_utf8(input)?,
            job.fuel_limit,
        )?;

        // Step 5: Submit execution receipt with output hash
        let receipt_tx = self.submit_receipt(
            job_id,
            output.output_hash.clone(),
            output.result.len() as u32,
        ).await?;

        Ok(ExecutionResult {
            job_id: hex::encode(job_id),
            output: output.result,
            output_hash: output.output_hash,
            receipt_tx: receipt_tx.to_string(),
        })
    }

    /// Accept job by depositing 2x collateral per Certus protocol
    async fn accept_job(&self, job_id: [u8; 32], pay_token: H160, pay_amount: U256) -> Result<H256> {
        // Calculate 2x collateral requirement
        let collateral = pay_amount.saturating_mul(U256::from(2));

        // Approve token transfer for collateral
        self.approve_token(pay_token, self.jobs_contract, collateral).await?;

        // Encode acceptJob call
        let accept_data = encode(&[
            Token::FixedBytes(job_id.to_vec()),
        ]);

        let calldata = [
            &ethers::utils::id("acceptJob(bytes32)")[0..4],
            &accept_data[..],
        ].concat();

        // Submit transaction with retry logic
        let tx = retry_with_backoff(
            || async {
                let pending_tx = self.signer
                    .send_transaction(
                        TransactionRequest::new()
                            .to(self.jobs_contract)
                            .data(calldata.clone())
                            .gas(300_000),
                        None,
                    )
                    .await?;

                let receipt = pending_tx.await?
                    .context("job acceptance failed")?;

                Ok(receipt)
            },
            &RetryConfig::default(),
        ).await?;

        Ok(tx.transaction_hash)
    }

    /// Verify job as verifier
    pub async fn verify_job(&self, job_id: [u8; 32]) -> Result<VerificationResult> {
        // fetch job and receipt
        let job = self.fetch_job_from_chain(job_id).await?;
        let receipt = self.fetch_receipt(job_id).await?;

        // re-execute
        let wasm = self.fetch_wasm(job.wasm_hash).await?;
        let input = self.fetch_input(job_id).await?;

        let output = self.executor.lock().unwrap().execute(
            &String::from_utf8(wasm.clone())?,
            &String::from_utf8(input.clone())?,
            job.fuel_limit,
        )?;

        // check if matches
        let matches = output.output_hash == receipt.output_hash;

        if !matches {
            // submit fraud proof via CertusEscrow
            let fraud_tx = self.submit_fraud_proof(
                job_id,
                wasm,
                input,
                output.result.as_bytes().to_vec(),
            ).await?;

            Ok(VerificationResult {
                job_id: hex::encode(job_id),
                verified: false,
                fraud_detected: true,
                fraud_tx: Some(fraud_tx.to_string()),
            })
        } else {
            Ok(VerificationResult {
                job_id: hex::encode(job_id),
                verified: true,
                fraud_detected: false,
                fraud_tx: None,
            })
        }
    }

    /// Submit fraud proof through CertusEscrow
    async fn submit_fraud_proof(
        &self,
        job_id: [u8; 32],
        wasm: Vec<u8>,
        input: Vec<u8>,
        claimed_output: Vec<u8>,
    ) -> Result<H256> {
        // first commit (MEV protection)
        let nonce = rand::random::<u64>();
        let commitment = self.compute_fraud_commitment(
            &job_id,
            &wasm,
            &input,
            &claimed_output,
            nonce,
        );

        // commit fraud
        let commit_data = self.encode_commit_fraud(job_id, commitment)?;
        let _commit_tx = self.signer
            .send_transaction(
                TransactionRequest::new()
                    .to(self.escrow_contract)
                    .data(commit_data),
                None,
            )
            .await?
            .await?
            .context("fraud commit failed")?;

        // wait for commit confirmation + 2 minutes
        tokio::time::sleep(tokio::time::Duration::from_secs(125)).await;

        // reveal fraud proof
        let reveal_data = self.encode_fraud_on_chain(
            job_id,
            wasm,
            input,
            claimed_output,
            nonce,
        )?;

        let reveal_tx = self.signer
            .send_transaction(
                TransactionRequest::new()
                    .to(self.escrow_contract)
                    .data(reveal_data),
                None,
            )
            .await?
            .await?
            .context("fraud reveal failed")?;

        Ok(reveal_tx.transaction_hash)
    }

    /// Compile Python to deterministic Wasm module
    async fn compile_python_to_wasm(&self, code: &str) -> Result<Vec<u8>> {
        // Validate determinism constraints
        self.executor.lock().unwrap().validate_python(code)?;

        // Compile to Wasm bytecode
        let mut compiler = crate::python_compiler::PythonCompiler::new();
        let wasm_module = compiler.compile(code)?;

        // Verify module is valid Wasm
        wasmparser::validate(&wasm_module)
            .context("generated Wasm module is invalid")?;

        Ok(wasm_module)
    }

    fn hash_bytes(&self, data: &[u8]) -> [u8; 32] {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher.finalize().into()
    }

    fn compute_fraud_commitment(
        &self,
        job_id: &[u8; 32],
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

    /// Fetch job data from CertusJobs contract
    async fn fetch_job_from_chain(&self, job_id: [u8; 32]) -> Result<JobData> {
        // Encode getJob(bytes32) call
        let calldata = [
            &ethers::utils::id("getJob(bytes32)")[0..4],
            &job_id[..],
        ].concat();

        let result = self.provider
            .call(&TransactionRequest::new().to(self.jobs_contract).data(calldata).into(), None)
            .await?;

        // Decode Job struct from contract
        // Job struct layout per CertusBase.sol:
        // bytes32 jobId, address client, address executor, address payToken,
        // uint256 payAmt, uint256 clientDeposit, uint256 executorDeposit,
        // uint256 dataStorageFee, bytes32 wasmHash, bytes32 inputHash,
        // bytes32 outputHash, bytes32 arweaveId, uint64 acceptDeadline,
        // uint64 finalizeDeadline, uint64 fuelLimit, uint64 memLimit,
        // uint32 maxOutputSize, uint8 status, address[3] selectedVerifiers,
        // address[3] backupVerifiers

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
        ], &result[..result.len().min(576)])?; // Limit to avoid verifier arrays

        Ok(JobData {
            wasm_hash: decoded[8].clone().into_fixed_bytes().unwrap().try_into().unwrap(),
            _input_hash: decoded[9].clone().into_fixed_bytes().unwrap().try_into().unwrap(),
            fuel_limit: decoded[14].clone().into_uint().unwrap().as_u64(),
            _mem_limit: decoded[15].clone().into_uint().unwrap().as_u64(),
            pay_token: decoded[3].clone().into_address().unwrap(),
            pay_amount: decoded[4].clone().into_uint().unwrap(),
        })
    }

    async fn fetch_receipt(&self, job_id: [u8; 32]) -> Result<ReceiptData> {
        let data = [
            &ethers::utils::id("receipts(bytes32)")[0..4],
            &job_id[..],
        ].concat();

        let result = self.provider
            .call(&TransactionRequest::new().to(self.jobs_contract).data(data).into(), None)
            .await?;

        let decoded = decode(&[
            ParamType::FixedBytes(32),
            ParamType::Address,
        ], &result)?;

        Ok(ReceiptData {
            output_hash: hex::encode(decoded[0].clone().into_fixed_bytes().unwrap()),
            _executor: decoded[1].clone().into_address().unwrap(),
        })
    }

    async fn fetch_wasm(&self, wasm_hash: [u8; 32]) -> Result<Vec<u8>> {
        let data = [
            &ethers::utils::id("wasmModules(bytes32)")[0..4],
            &wasm_hash[..],
        ].concat();

        let result = self.provider
            .call(&TransactionRequest::new().to(self.jobs_contract).data(data).into(), None)
            .await?;

        Ok(result.to_vec())
    }

    async fn fetch_input(&self, job_id: [u8; 32]) -> Result<Vec<u8>> {
        let data = [
            &ethers::utils::id("jobInputs(bytes32)")[0..4],
            &job_id[..],
        ].concat();

        let result = self.provider
            .call(&TransactionRequest::new().to(self.jobs_contract).data(data).into(), None)
            .await?;

        Ok(result.to_vec())
    }

    /// Submit execution receipt per CertusJobs protocol
    async fn submit_receipt(&self, job_id: [u8; 32], output_hash: String, output_size: u32) -> Result<H256> {
        let output_hash_bytes: [u8; 32] = hex::decode(&output_hash)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid output hash"))?;

        // Generate Ed25519 signature for cryptographic proof
        let exec_sig = self.generate_execution_signature(job_id, output_hash_bytes);

        // Encode submitReceipt call per contract ABI
        let receipt_data = encode(&[
            Token::FixedBytes(job_id.to_vec()),
            Token::FixedBytes(output_hash_bytes.to_vec()),
            Token::Bytes(exec_sig),
            Token::Uint(U256::from(output_size)),
        ]);

        let calldata = [
            &ethers::utils::id("submitReceipt(bytes32,bytes32,bytes,uint32)")[0..4],
            &receipt_data[..],
        ].concat();

        // Submit with retry for resilience
        let tx = retry_with_backoff(
            || async {
                let pending_tx = self.signer
                    .send_transaction(
                        TransactionRequest::new()
                            .to(self.jobs_contract)
                            .data(calldata.clone())
                            .gas(250_000),
                        None,
                    )
                    .await?;

                let receipt = pending_tx.await?
                    .context("receipt submission failed")?;

                Ok(receipt)
            },
            &RetryConfig::default(),
        ).await?;

        Ok(tx.transaction_hash)
    }

    /// Generate Ed25519 signature for execution proof
    fn generate_execution_signature(&self, job_id: [u8; 32], output_hash: [u8; 32]) -> Vec<u8> {
        // For demo purposes, generate a placeholder 64-byte signature
        // In production, use proper Ed25519 signing
        let mut sig = vec![0u8; 64];
        sig[..32].copy_from_slice(&job_id);
        sig[32..].copy_from_slice(&output_hash);
        sig
    }

    /// Compute deterministic job ID per Certus protocol specification
    /// JobId = SHA256(wasmHash || inputHash || clientPubKey || nonce)
    fn compute_job_id(&self, wasm_hash: [u8; 32], input_hash: [u8; 32], client: H160) -> [u8; 32] {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(wasm_hash);
        hasher.update(input_hash);
        hasher.update(client.as_bytes());
        hasher.update(&rand::random::<u64>().to_be_bytes()); // nonce for uniqueness
        hasher.finalize().into()
    }

    /// Calculate client deposit per Certus economic model
    /// deposit = max(min($5, 5% of payment), $1000)
    fn calculate_client_deposit(&self, payment: U256) -> U256 {
        let five_percent = payment / 20; // 5% = payment / 20
        let min_deposit = U256::from(5_000_000u128); // $5 in USDC (6 decimals)
        let max_deposit = U256::from(1_000_000_000u128); // $1000 in USDC

        if five_percent < min_deposit {
            min_deposit
        } else if five_percent > max_deposit {
            max_deposit
        } else {
            five_percent
        }
    }

    /// Approve ERC20 token spending per EIP-20 standard
    async fn approve_token(&self, token: H160, spender: H160, amount: U256) -> Result<()> {
        let approve_data = encode(&[
            Token::Address(spender),
            Token::Uint(amount),
        ]);

        let calldata = [
            &ethers::utils::id("approve(address,uint256)")[0..4],
            &approve_data[..],
        ].concat();

        let tx = self.signer
            .send_transaction(
                TransactionRequest::new()
                    .to(token)
                    .data(calldata)
                    .gas(100_000),
                None,
            )
            .await?
            .await?
            .context("token approval failed")?;

        // Verify approval succeeded
        if tx.status != Some(U64::from(1)) {
            bail!("token approval transaction failed");
        }

        Ok(())
    }

    /// Encode createJob call per CertusJobs ABI
    fn encode_create_job(
        &self,
        job_id: [u8; 32],
        wasm_hash: [u8; 32],
        input_hash: [u8; 32],
        pay_token: H160,
        pay_amt: U256,
        accept_window: u64,
        challenge_window: u64,
        fuel_limit: u64,
        mem_limit: u64,
        max_output_size: u32,
    ) -> Result<Vec<u8>> {
        let data = encode(&[
            Token::FixedBytes(job_id.to_vec()),
            Token::FixedBytes(wasm_hash.to_vec()),
            Token::FixedBytes(input_hash.to_vec()),
            Token::Address(pay_token),
            Token::Uint(pay_amt),
            Token::Uint(U256::from(accept_window)),
            Token::Uint(U256::from(challenge_window)),
            Token::Uint(U256::from(fuel_limit)),
            Token::Uint(U256::from(mem_limit)),
            Token::Uint(U256::from(max_output_size)),
        ]);

        Ok([
            &ethers::utils::id("createJob(bytes32,bytes32,bytes32,address,uint256,uint64,uint64,uint64,uint64,uint32)")[0..4],
            &data[..],
        ].concat())
    }

    fn encode_commit_fraud(&self, job_id: [u8; 32], commitment: [u8; 32]) -> Result<Vec<u8>> {
        let data = encode(&[
            Token::FixedBytes(job_id.to_vec()),
            Token::FixedBytes(commitment.to_vec()),
        ]);

        Ok([
            &ethers::utils::id("commitFraud(bytes32,bytes32)")[0..4],
            &data[..],
        ].concat())
    }

    fn encode_fraud_on_chain(
        &self,
        job_id: [u8; 32],
        wasm: Vec<u8>,
        input: Vec<u8>,
        output: Vec<u8>,
        nonce: u64,
    ) -> Result<Vec<u8>> {
        let data = encode(&[
            Token::FixedBytes(job_id.to_vec()),
            Token::Bytes(wasm),
            Token::Bytes(input),
            Token::Bytes(output),
            Token::Uint(U256::from(nonce)),
        ]);

        Ok([
            &ethers::utils::id("fraudOnChain(bytes32,bytes,bytes,bytes,uint256)")[0..4],
            &data[..],
        ].concat())
    }
}

#[derive(Debug)]
struct JobData {
    wasm_hash: [u8; 32],
    _input_hash: [u8; 32],
    fuel_limit: u64,
    _mem_limit: u64,
    pay_token: H160,
    pay_amount: U256,
}

#[derive(Debug)]
struct ReceiptData {
    output_hash: String,
    _executor: H160,
}

#[derive(Debug, serde::Serialize)]
pub struct ExecutionResult {
    pub job_id: String,
    pub output: String,
    pub output_hash: String,
    pub receipt_tx: String,
}

#[derive(Debug, serde::Serialize)]
pub struct VerificationResult {
    pub job_id: String,
    pub verified: bool,
    pub fraud_detected: bool,
    pub fraud_tx: Option<String>,
}

pub struct VrfStatus {
    pub fulfilled: bool,
    pub elapsed: u64,
}

impl CertusIntegration {
    /// Get jobs in receipt state awaiting verification
    pub async fn get_pending_verification_jobs(&self) -> Result<Vec<[u8; 32]>> {
        // Query CertusJobs for jobs in Status::Receipt
        let calldata = ethers::utils::id("getPendingVerificationJobs()")[0..4].to_vec();

        let result = self.provider
            .call(&TransactionRequest::new().to(self.jobs_contract).data(calldata).into(), None)
            .await?;

        // Decode array of job IDs
        if result.len() >= 64 {
            let decoded = ethers::abi::decode(&[ParamType::Array(Box::new(ParamType::FixedBytes(32)))], &result)?;
            if let Some(Token::Array(jobs)) = decoded.first() {
                return Ok(jobs.iter()
                    .filter_map(|t| {
                        if let Token::FixedBytes(b) = t {
                            Some(b.clone().try_into().ok()?)
                        } else {
                            None
                        }
                    })
                    .collect());
            }
        }
        Ok(Vec::new())
    }

    /// Check VRF fulfillment status
    pub async fn check_vrf_status(&self, job_id: [u8; 32]) -> Result<VrfStatus> {
        // Query vrfRequestFulfilled and vrfRequestTime
        let calldata = [
            &ethers::utils::id("getVrfStatus(bytes32)")[0..4],
            &job_id[..],
        ].concat();

        let result = self.provider
            .call(&TransactionRequest::new().to(self.jobs_contract).data(calldata).into(), None)
            .await?;

        if result.len() >= 64 {
            let decoded = decode(&[
                ParamType::Bool,    // fulfilled
                ParamType::Uint(256), // request time
            ], &result)?;

            let fulfilled = decoded[0].clone().into_bool().unwrap_or(false);
            let request_time = decoded[1].clone().into_uint().unwrap_or(U256::zero()).as_u64();
            let elapsed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs()
                .saturating_sub(request_time);

            return Ok(VrfStatus { fulfilled, elapsed });
        }

        Ok(VrfStatus { fulfilled: false, elapsed: 0 })
    }

    /// Trigger fallback verifier selection
    pub async fn trigger_fallback_selection(&self, job_id: [u8; 32]) -> Result<H256> {
        let calldata = [
            &ethers::utils::id("fallbackVerifierSelection(bytes32)")[0..4],
            &job_id[..],
        ].concat();

        let tx = self.signer
            .send_transaction(
                TransactionRequest::new()
                    .to(self.jobs_contract)
                    .data(calldata)
                    .gas(500_000),
                None,
            )
            .await?
            .await?
            .context("fallback selection failed")?;

        Ok(tx.transaction_hash)
    }


    pub async fn execute_python_job(&self, job_id: &str, code: &str, input: &str) -> Result<ExecutionResult> {
        // execute locally first
        let output = self.executor.lock().unwrap().execute(code, input, 1_000_000)?;

        // submit receipt to chain
        let job_id_bytes: [u8; 32] = hex::decode(job_id.trim_start_matches("0x"))?
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid job id"))?;

        let receipt_data = [
            ethers::abi::Token::FixedBytes(job_id_bytes.to_vec()),
            ethers::abi::Token::FixedBytes(hex::decode(&output.output_hash)?.to_vec()),
        ];

        let receipt_tx = self.signer
            .send_transaction(
                TransactionRequest::new()
                    .to(self.jobs_contract)
                    .data(ethers::abi::encode(&receipt_data))
                    .gas(200_000),
                None,
            )
            .await?
            .await?
            .context("receipt submission failed")?;

        Ok(ExecutionResult {
            job_id: job_id.to_string(),
            output: output.result,
            output_hash: output.output_hash,
            receipt_tx: format!("0x{}", hex::encode(receipt_tx.transaction_hash)),
        })
    }
}
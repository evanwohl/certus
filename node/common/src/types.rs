use ethers::types::{H256, U256, Address};
use serde::{Deserialize, Serialize};

/// Job specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSpec {
    pub job_id: [u8; 32],
    pub wasm_hash: [u8; 32],
    pub input_hash: [u8; 32],
    pub pay_token: Address,
    pub pay_amt: U256,
    pub client_deposit: U256,
    pub fuel_limit: u64,
    pub mem_limit: u64,
    pub max_output_size: u32,
}

/// Execution receipt
#[derive(Debug, Clone)]
pub struct ExecReceipt {
    pub job_id: [u8; 32],
    pub output_hash: [u8; 32],
    pub executor_sig: [u8; 64],
    pub executor_addr: Address,
    pub collateral: U256,
}

/// Job status
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JobStatus {
    Created,
    Accepted,
    Receipt,
    Challenged,
    Finalized,
    Aborted,
}

/// Wasm execution result
#[derive(Debug)]
pub struct ExecutionResult {
    pub output: Vec<u8>,
    pub fuel_consumed: u64,
    pub success: bool,
}

/// Verification result
#[derive(Debug)]
pub enum VerificationResult {
    Valid,
    Fraud {
        claimed: H256,
        computed: H256,
    },
    Error(String),
}
use axum::{
    extract::{Path, State, Json},
    http::StatusCode,
    response::IntoResponse,
    routing::{post, get},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use std::collections::HashMap;
use sha2::Digest;
use crate::certus_integration::CertusIntegration;

/// API server - all ops through Certus contracts
pub struct ApiServer {
    certus: Arc<CertusIntegration>,
    jobs: Arc<RwLock<HashMap<String, CertusJobRecord>>>,
}

impl ApiServer {
    pub async fn new(
        executor: Arc<Mutex<crate::PythonExecutor>>,
        rpc_url: &str,
        private_key: &str,
        escrow_addr: &str,
        jobs_addr: &str,
    ) -> anyhow::Result<Self> {
        let certus = Arc::new(
            CertusIntegration::new(executor, rpc_url, private_key, escrow_addr, jobs_addr).await?
        );

        Ok(Self {
            certus,
            jobs: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub fn routes(self) -> Router {
        let state = Arc::new(self);

        Router::new()
            .route("/api/submit", post(submit_python_job))
            .route("/api/execute/:id", post(execute_job))
            .route("/api/verify/:id", post(verify_job))
            .route("/api/job/:id", get(get_job))
            .route("/api/jobs", get(list_jobs))
            .route("/api/examples", get(get_examples))
            .layer(CorsLayer::permissive())
            .with_state(state)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CertusJobRecord {
    job_id: String, // bytes32 on chain
    python_code: String,
    input: serde_json::Value,
    tx_hash: Option<String>,
    output_hash: Option<String>,
    status: CertusJobStatus,
    created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum CertusJobStatus {
    Pending,      // submitted to Certus
    Executed,     // executor posted receipt
    Challenged,   // under verification
    Verified,     // passed challenge window
    Fraudulent,   // fraud proven
}

#[derive(Debug, Deserialize)]
struct SubmitJobRequest {
    python_code: String,
    input: serde_json::Value,
    payment_amount: String, // payment amount in token units (e.g., USDC with 6 decimals)
    pay_token: String,      // ERC20 token address (USDC/USDT/DAI)
}

#[derive(Debug, Serialize)]
struct SubmitJobResponse {
    job_id: String,
    tx_hash: String,
    escrow_address: String,
    jobs_address: String,
}

/// Submit Python job to Certus
async fn submit_python_job(
    State(state): State<Arc<ApiServer>>,
    Json(req): Json<SubmitJobRequest>,
) -> impl IntoResponse {
    // Parse payment amount (assuming token with 6 decimals like USDC)
    let payment = match req.payment_amount.parse::<ethers::types::U256>() {
        Ok(p) => p,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid payment amount").into_response(),
    };

    // Parse token address
    let pay_token = match req.pay_token.parse::<ethers::types::H160>() {
        Ok(t) => t,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid token address").into_response(),
    };

    // Submit to Certus contracts with token parameter
    match state.certus.create_python_job(
        &req.python_code,
        &serde_json::to_string(&req.input).unwrap(),
        payment,
        pay_token,
    ).await {
        Ok(tx_hash) => {
            // generate job ID (would get from tx receipt)
            let job_id = format!("0x{}", hex::encode(sha2::Sha256::digest(
                format!("{}{}", req.python_code, req.input).as_bytes()
            )));

            // store locally
            let record = CertusJobRecord {
                job_id: job_id.clone(),
                python_code: req.python_code,
                input: req.input,
                tx_hash: Some(format!("{:?}", tx_hash)),
                output_hash: None,
                status: CertusJobStatus::Pending,
                created_at: chrono::Utc::now().timestamp() as u64,
            };

            state.jobs.write().await.insert(job_id.clone(), record);

            Json(SubmitJobResponse {
                job_id,
                tx_hash: format!("{:?}", tx_hash),
                escrow_address: format!("{:?}", state.certus.escrow_contract),
                jobs_address: format!("{:?}", state.certus.jobs_contract),
            }).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
    }
}

/// Execute job as executor (via Certus)
async fn execute_job(
    State(state): State<Arc<ApiServer>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // parse job ID
    let job_id_bytes = match hex::decode(id.trim_start_matches("0x")) {
        Ok(b) if b.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&b);
            arr
        }
        _ => return (StatusCode::BAD_REQUEST, "Invalid job ID").into_response(),
    };

    // execute via Certus
    match state.certus.execute_job(job_id_bytes).await {
        Ok(result) => {
            // update local record
            if let Some(record) = state.jobs.write().await.get_mut(&id) {
                record.output_hash = Some(result.output_hash.clone());
                record.status = CertusJobStatus::Executed;
            }

            Json(result).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
    }
}

/// Verify job (via Certus verifier flow)
async fn verify_job(
    State(state): State<Arc<ApiServer>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // parse job ID
    let job_id_bytes = match hex::decode(id.trim_start_matches("0x")) {
        Ok(b) if b.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&b);
            arr
        }
        _ => return (StatusCode::BAD_REQUEST, "Invalid job ID").into_response(),
    };

    // verify via Certus
    match state.certus.verify_job(job_id_bytes).await {
        Ok(result) => {
            // update local record
            if let Some(record) = state.jobs.write().await.get_mut(&id) {
                record.status = if result.fraud_detected {
                    CertusJobStatus::Fraudulent
                } else {
                    CertusJobStatus::Verified
                };
            }

            Json(result).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
    }
}

/// Get job status
async fn get_job(
    State(state): State<Arc<ApiServer>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let jobs = state.jobs.read().await;

    match jobs.get(&id) {
        Some(job) => Json(job.clone()).into_response(),
        None => (StatusCode::NOT_FOUND, "Job not found").into_response()
    }
}

/// List all jobs
async fn list_jobs(
    State(state): State<Arc<ApiServer>>,
) -> impl IntoResponse {
    let jobs = state.jobs.read().await;
    let mut job_list: Vec<CertusJobRecord> = jobs.values().cloned().collect();
    job_list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Json(job_list)
}

/// Get example scripts
async fn get_examples() -> impl IntoResponse {
    Json(serde_json::json!([
        {
            "name": "fibonacci",
            "code": "def fib(n):\n    if n <= 1:\n        return n\n    return fib(n-1) + fib(n-2)\n\nresult = fib(input['n'])",
            "input": {"n": 20},
            "payment_amount": "10000000", // $10 USDC (6 decimals)
            "pay_token": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48" // USDC on Arbitrum
        },
        {
            "name": "merkle_tree",
            "code": "def sha256(data):\n    return hash(data)\n\ndef merkle_root(leaves):\n    if len(leaves) == 1:\n        return leaves[0]\n    pairs = []\n    for i in range(0, len(leaves), 2):\n        pairs.append(sha256(leaves[i] + leaves[i+1]))\n    return merkle_root(pairs)\n\nresult = merkle_root(input['leaves'])",
            "input": {"leaves": ["tx1", "tx2", "tx3", "tx4"]},
            "payment_amount": "25000000", // $25 USDC
            "pay_token": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
        },
        {
            "name": "monte_carlo",
            "code": "def lcg(seed, a, c, m):\n    return (a * seed + c) % m\n\ndef monte_carlo(seed, iters):\n    inside = 0\n    for i in range(iters):\n        x = lcg(seed + i, 1103515245, 12345, 2147483647) % 10000\n        y = lcg(seed + i + 1, 1103515245, 12345, 2147483647) % 10000\n        if x*x + y*y < 10000*10000:\n            inside = inside + 1\n    return inside * 4\n\nresult = monte_carlo(input['seed'], input['iterations'])",
            "input": {"seed": 42, "iterations": 10000},
            "payment_amount": "50000000", // $50 USDC for compute-intensive task
            "pay_token": "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
        }
    ]))
}
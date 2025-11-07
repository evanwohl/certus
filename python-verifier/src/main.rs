use anyhow::Result;
use clap::Parser;
use std::sync::{Arc, Mutex};

mod compiler;
mod verifier;
mod api;
mod websocket;
mod queue;
mod certus_integration;
mod reliability;
mod validation;

use python_verifier::PythonExecutor;
use certus_integration::CertusIntegration;
use queue::JobQueue;
use websocket::{WsState, ws_handler, broadcast_update, JobUpdate};
use verifier::PythonVerifier;
use validation::{PythonValidator, validate_json_input, validate_output};
use reliability::{validate_job_id, validate_gas_params};


#[derive(Parser, Debug)]
#[clap(name = "python-verifier")]
#[clap(about = "Certus Python Verifier - Cryptographically verified Python via Certus protocol")]
struct Args {
    #[clap(short, long, default_value = "8080")]
    port: u16,

    #[clap(short, long, env = "ARBITRUM_RPC")]
    rpc: String,

    #[clap(short = 'k', long, env = "PRIVATE_KEY")]
    private_key: String,

    #[clap(short, long, env = "ESCROW_ADDRESS")]
    escrow: String,

    #[clap(short, long, env = "JOBS_ADDRESS")]
    jobs: String,

    #[clap(long, default_value = "./queue.db")]
    queue_path: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();

    log::info!("Starting Certus Python Verifier");
    log::info!("Escrow: {}", args.escrow);
    log::info!("Jobs: {}", args.jobs);
    log::info!("RPC: {}", args.rpc);

    // initialize executor
    let executor = Arc::new(Mutex::new(PythonExecutor::new()?));

    // initialize job queue
    let queue = Arc::new(JobQueue::new(&args.queue_path)?);

    // initialize WebSocket state
    let ws_state = Arc::new(WsState::new());

    // initialize Certus integration
    let integration = Arc::new(CertusIntegration::new(
        executor.clone(),
        &args.rpc,
        &args.private_key,
        &args.escrow,
        &args.jobs,
    ).await?);

    // initialize verifier
    let verifier = Arc::new(PythonVerifier::new(
        &args.rpc,
        &args.private_key,
        &args.escrow,
        &args.jobs,
    ).await?);

    // spawn queue processor
    let queue_clone = queue.clone();
    let integration_clone = integration.clone();
    let ws_state_clone = ws_state.clone();
    tokio::spawn(async move {
        loop {
            if let Ok(Some(job)) = queue_clone.next().await {
                log::info!("Processing job: {}", job.id);

                // validate input
                match validate_json_input(&serde_json::to_string(&job.input).unwrap()) {
                    Ok(validated) => {
                        // execute via integration
                        match integration_clone.execute_python_job(&job.id, &job.code, &validated.to_string()).await {
                            Ok(result) => {
                                // validate output
                                let _ = validate_output(&result.output);
                                // validate job id format
                                let _ = validate_job_id(&job.id);
                                // validate gas params
                                let _ = validate_gas_params(200_000, 5_000_000);
                                log::info!("Job {} completed: {}", job.id, result.output_hash);

                                // broadcast update
                                broadcast_update(&ws_state_clone, JobUpdate {
                                    job_id: job.id.clone(),
                                    status: "completed".to_string(),
                                    timestamp: chrono::Utc::now().timestamp() as u64,
                                    data: serde_json::json!({
                                        "output": result.output,
                                        "hash": result.output_hash,
                                    }),
                                });

                                let _ = queue_clone.complete(&job.id, serde_json::json!({
                                    "output": result.output,
                                    "hash": result.output_hash,
                                    "tx": result.receipt_tx,
                                })).await;
                            }
                            Err(e) => {
                                log::error!("Job {} failed: {}", job.id, e);
                                let _ = queue_clone.fail(&job.id, &e.to_string()).await;
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Invalid input for job {}: {}", job.id, e);
                        let _ = queue_clone.fail(&job.id, &e.to_string()).await;
                    }
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    });

    // spawn verifier task with VRF awareness
    let verifier_clone = verifier.clone();
    let integration_verifier = integration.clone();
    tokio::spawn(async move {
        loop {
            // Fetch jobs awaiting verification
            match integration_verifier.get_pending_verification_jobs().await {
                Ok(job_ids) => {
                    for job_id in job_ids {
                        // Check if VRF selection completed
                        match integration_verifier.check_vrf_status(job_id).await {
                            Ok(vrf_status) => {
                                if !vrf_status.fulfilled && vrf_status.elapsed > 1800 {
                                    // VRF grace period (30 min) expired, trigger fallback
                                    log::info!("Triggering fallback selection for job {}", hex::encode(job_id));
                                    if let Err(e) = integration_verifier.trigger_fallback_selection(job_id).await {
                                        log::error!("Fallback selection failed: {}", e);
                                        continue;
                                    }
                                }

                                // Attempt verification (will check if selected)
                                if let Err(e) = verifier_clone.verify_certus_job(job_id).await {
                                    log::error!("Verification failed for job {}: {}", hex::encode(job_id), e);
                                } else {
                                    log::debug!("Processed job: {}", hex::encode(job_id));
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to check VRF status: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to fetch pending verification jobs: {}", e);
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        }
    });

    // spawn cleanup task
    let queue_clone = queue.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
            if let Ok(deleted) = queue_clone.cleanup_old(86400 * 7) {
                log::info!("Cleaned up {} old jobs", deleted);
            }
        }
    });

    // validate Python code syntax
    PythonValidator::validate_code("OUTPUT = INPUT['x'] * 2")?;

    // submit sample job to queue
    let _ = queue.submit(queue::QueuedJob {
        id: "sample".to_string(),
        code: "OUTPUT = INPUT['x'] * 2".to_string(),
        input: serde_json::json!({"x": 21}),
        priority: 1,
        created_at: chrono::Utc::now().timestamp() as u64,
        retry_count: 0,
        max_retries: 3,
    }).await;

    // create API server
    let api_server = api::ApiServer::new(
        executor.clone(),
        &args.rpc,
        &args.private_key,
        &args.escrow,
        &args.jobs,
    ).await?;

    // build routes
    use axum::{Router, routing::get};
    let api_routes = api_server.routes();
    let app = Router::new()
        .route("/ws", get(move |ws, state| ws_handler(ws, state)))
        .with_state(ws_state.clone())
        .nest("/", api_routes);

    // start server
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], args.port));
    log::info!("API server listening on {}", addr);

    // could use api_server.run(port) if not using websockets
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
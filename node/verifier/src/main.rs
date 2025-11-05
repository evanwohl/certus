mod verifier;

use anyhow::Result;
use tracing::info;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("Starting Certus Verifier");

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("Usage: verifier <rpc_url> <private_key> <contract_address>");
        std::process::exit(1);
    }

    let rpc_url = &args[1];
    let private_key = &args[2];
    let contract_address = &args[3];

    let verifier = verifier::VerifierNode::new(
        rpc_url,
        private_key,
        contract_address,
    ).await?;

    verifier.run().await?;

    Ok(())
}
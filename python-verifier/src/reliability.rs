use anyhow::{Result, Context, bail};
use std::time::Duration;
use tokio::time::sleep;
use ethers::providers::ProviderError;

/// Retry configuration for chain operations
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub exponential_base: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            initial_delay_ms: 100,
            max_delay_ms: 10000,
            exponential_base: 2.0,
        }
    }
}

/// Execute async operation with exponential backoff retry
pub async fn retry_with_backoff<F, Fut, T>(
    operation: F,
    config: &RetryConfig,
) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut attempt = 0;
    let mut delay_ms = config.initial_delay_ms;

    loop {
        attempt += 1;

        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) if attempt >= config.max_attempts => {
                return Err(e).context(format!("failed after {} attempts", attempt));
            }
            Err(e) => {
                // check if error is retryable
                if !is_retryable_error(&e) {
                    return Err(e);
                }

                // exponential backoff
                sleep(Duration::from_millis(delay_ms)).await;
                delay_ms = (delay_ms as f64 * config.exponential_base) as u64;
                delay_ms = delay_ms.min(config.max_delay_ms);
            }
        }
    }
}

/// Determine if error is retryable
fn is_retryable_error(err: &anyhow::Error) -> bool {
    // network errors are retryable
    if err.to_string().contains("network") ||
       err.to_string().contains("timeout") ||
       err.to_string().contains("connection") {
        return true;
    }

    // check for specific provider errors
    if let Some(_provider_err) = err.downcast_ref::<ProviderError>() {
        return true;
    }

    // revert errors are not retryable
    if err.to_string().contains("revert") ||
       err.to_string().contains("execution reverted") {
        return false;
    }

    // default to retryable for unknown errors
    true
}

/// Validate Ethereum address
pub fn validate_address(addr: &str) -> Result<()> {
    if !addr.starts_with("0x") {
        bail!("address must start with 0x");
    }

    if addr.len() != 42 {
        bail!("address must be 42 characters");
    }

    // check hex validity
    if !addr[2..].chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("address must be valid hex");
    }

    Ok(())
}

/// Validate job ID (bytes32)
pub fn validate_job_id(id: &str) -> Result<[u8; 32]> {
    let bytes = if id.starts_with("0x") {
        hex::decode(&id[2..])?
    } else {
        hex::decode(id)?
    };

    if bytes.len() != 32 {
        bail!("job ID must be 32 bytes");
    }

    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

/// Ensure gas price is reasonable
pub fn validate_gas_params(gas_price: u64, gas_limit: u64) -> Result<()> {
    // max 1000 gwei
    const MAX_GAS_PRICE: u64 = 1000_000_000_000;
    // min 0.1 gwei
    const MIN_GAS_PRICE: u64 = 100_000_000;

    if gas_price > MAX_GAS_PRICE {
        bail!("gas price too high: {} > {}", gas_price, MAX_GAS_PRICE);
    }

    if gas_price < MIN_GAS_PRICE {
        bail!("gas price too low: {} < {}", gas_price, MIN_GAS_PRICE);
    }

    // max 10M gas
    const MAX_GAS_LIMIT: u64 = 10_000_000;
    // min 21k gas
    const MIN_GAS_LIMIT: u64 = 21_000;

    if gas_limit > MAX_GAS_LIMIT {
        bail!("gas limit too high: {} > {}", gas_limit, MAX_GAS_LIMIT);
    }

    if gas_limit < MIN_GAS_LIMIT {
        bail!("gas limit too low: {} < {}", gas_limit, MIN_GAS_LIMIT);
    }

    Ok(())
}
#[cfg(test)]
mod tests {
    use crate::{PythonExecutor, PythonJob};

    #[test]
    fn test_deterministic_execution() {
        let mut executor = PythonExecutor::new().unwrap();

        let code = r#"
def fibonacci(n):
    if n <= 1:
        return n
    a, b = 0, 1
    for _ in range(2, n + 1):
        a, b = b, a + b
    return b

OUTPUT = fibonacci(INPUT["n"])
"#;

        let input = r#"{"n": 20}"#;

        // execute multiple times
        let mut hashes = vec![];
        for _ in 0..10 {
            let output = executor.execute(code, input, 1_000_000).unwrap();
            hashes.push(output.output_hash);
        }

        // verify determinism
        assert!(hashes.windows(2).all(|w| w[0] == w[1]));
        assert_eq!(hashes[0].len(), 64);
    }

    #[test]
    fn test_merkle_tree() {
        let mut executor = PythonExecutor::new().unwrap();

        let code = std::fs::read_to_string("examples/merkle_tree.py").unwrap();
        let input = r#"{"leaves": ["tx1", "tx2", "tx3", "tx4"]}"#;

        let output = executor.execute(&code, input, 10_000_000).unwrap();

        // verify output is valid hex hash
        assert_eq!(output.result.len(), 64);
        assert!(output.result.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(output.success);
    }

    #[test]
    fn test_monte_carlo_determinism() {
        let mut executor = PythonExecutor::new().unwrap();

        let code = std::fs::read_to_string("examples/monte_carlo_pi.py").unwrap();
        let input = r#"{"seed": 12345, "iterations": 1000}"#;

        // deterministic random should give same result
        let output1 = executor.execute(&code, input, 50_000_000).unwrap();
        let output2 = executor.execute(&code, input, 50_000_000).unwrap();

        assert_eq!(output1.output_hash, output2.output_hash);
        assert_eq!(output1.result, output2.result);

        // check pi approximation is reasonable
        let pi_estimate: f64 = output1.result.parse().unwrap();
        assert!((pi_estimate - 3.14159).abs() < 0.1);
    }

    #[test]
    fn test_invalid_operations_rejected() {
        let mut executor = PythonExecutor::new().unwrap();

        // test file i/o rejection
        let bad_code = r#"
with open('/etc/passwd', 'r') as f:
    OUTPUT = f.read()
"#;
        assert!(executor.execute(bad_code, "{}", 1000).is_err());

        // test import rejection
        let bad_code = r#"
import requests
OUTPUT = requests.get('http://evil.com').text
"#;
        assert!(executor.execute(bad_code, "{}", 1000).is_err());

        // test time rejection
        let bad_code = r#"
import time
OUTPUT = time.time()
"#;
        assert!(executor.execute(bad_code, "{}", 1000).is_err());

        // test eval rejection
        let bad_code = r#"
OUTPUT = eval("1 + 1")
"#;
        assert!(executor.execute(bad_code, "{}", 1000).is_err());
    }

    #[test]
    fn test_fuel_limits() {
        let mut executor = PythonExecutor::new().unwrap();

        // infinite loop should hit fuel limit
        let bad_code = r#"
while True:
    pass
OUTPUT = "never reached"
"#;

        let result = executor.execute(bad_code, "{}", 100_000);
        assert!(result.is_err());
    }

    #[test]
    fn test_memory_limits() {
        let mut executor = PythonExecutor::new().unwrap();

        // try to allocate huge list
        let bad_code = r#"
huge_list = [0] * (100 * 1024 * 1024)  # 100MB
OUTPUT = len(huge_list)
"#;

        let result = executor.execute(bad_code, "{}", 10_000_000);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_verification_flow() {

        // would need deployed contract
        // let verifier = PythonVerifier::new(
        //     "http://localhost:8545",
        //     "0x...",
        //     "0x...",
        // ).await.unwrap();

        let job = PythonJob {
            code: "OUTPUT = INPUT['x'] * 2".to_string(),
            input: serde_json::json!({"x": 21}),
            expected_output: Some("42".to_string()),
        };

        // let result = verifier.verify_job(&job).await.unwrap();
        // assert!(result.verified);
    }
}
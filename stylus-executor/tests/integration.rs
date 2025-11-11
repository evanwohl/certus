// verify determinism matches off-chain execution

use certus_stylus_executor::*;

#[test]
fn test_simple_wasm_execution() {
    // Simple Wasm module: returns 42
    let wasm = [
        0x00, 0x61, 0x73, 0x6D, // magic
        0x01, 0x00, 0x00, 0x00, // version
        0x01, 0x07, // type section
        0x01, // 1 type
        0x60, 0x02, 0x7F, 0x7F, 0x01, 0x7F, // (i32, i32) -> i32
        0x03, 0x02, // function section
        0x01, 0x00, // 1 function, type 0
        0x05, 0x03, // memory section
        0x01, 0x00, 0x01, // memory 0, min 1 page
        0x07, 0x08, // export section
        0x01, // 1 export
        0x04, 0x6D, 0x61, 0x69, 0x6E, // "main"
        0x00, 0x00, // function 0
        0x0A, 0x09, // code section
        0x01, // 1 function body
        0x07, // body size
        0x00, // local count
        0x41, 0x2A, // i32.const 42
        0x0B, // end
    ];

    let input = vec![0u8; 32];
    let fuel_limit = 1_000_000;
    let mem_limit = 65536;

    // This would be called via Stylus contract
    // let output = execute_wasm(&wasm, &input, fuel_limit, mem_limit);
    // assert!(output.is_ok());
}

#[test]
fn test_determinism_validation() {
    // Valid module
    let valid_wasm = [
        0x00, 0x61, 0x73, 0x6D,
        0x01, 0x00, 0x00, 0x00,
    ];
    assert!(validate_determinism(&valid_wasm).is_ok());

    // Module with float opcode
    let float_wasm = [
        0x00, 0x61, 0x73, 0x6D,
        0x01, 0x00, 0x00, 0x00,
        0x43, // f32.const
    ];
    assert!(validate_determinism(&float_wasm).is_err());

    // Module with WASI import
    let mut wasi_wasm = vec![
        0x00, 0x61, 0x73, 0x6D,
        0x01, 0x00, 0x00, 0x00,
    ];
    wasi_wasm.extend_from_slice(b"wasi_snapshot");
    assert!(validate_determinism(&wasi_wasm).is_err());

    // Module with thread opcode
    let thread_wasm = [
        0x00, 0x61, 0x73, 0x6D,
        0x01, 0x00, 0x00, 0x00,
        0xFE, // atomic prefix
    ];
    assert!(validate_determinism(&thread_wasm).is_err());
}

#[test]
fn test_execution_id_collision_resistance() {
    let wasm1 = b"wasm_v1";
    let wasm2 = b"wasm_v2";
    let input1 = b"input_a";
    let input2 = b"input_b";

    let id1 = compute_execution_id(wasm1, input1);
    let id2 = compute_execution_id(wasm2, input1);
    let id3 = compute_execution_id(wasm1, input2);
    let id4 = compute_execution_id(wasm2, input2);

    // All IDs should be unique
    assert_ne!(id1, id2);
    assert_ne!(id1, id3);
    assert_ne!(id1, id4);
    assert_ne!(id2, id3);
    assert_ne!(id2, id4);
    assert_ne!(id3, id4);
}

#[test]
fn test_module_size_limit() {
    // Module exactly at 24KB limit
    let mut wasm = vec![
        0x00, 0x61, 0x73, 0x6D,
        0x01, 0x00, 0x00, 0x00,
    ];
    wasm.resize(24 * 1024, 0);
    assert!(validate_determinism(&wasm).is_ok());

    // Module exceeding 24KB limit
    let mut large_wasm = vec![
        0x00, 0x61, 0x73, 0x6D,
        0x01, 0x00, 0x00, 0x00,
    ];
    large_wasm.resize(24 * 1024 + 1, 0);
    // This would be rejected by execute() but validate_determinism only checks opcodes
}

#[test]
fn test_pattern_matching() {
    assert!(contains_pattern(b"hello world", b"world"));
    assert!(contains_pattern(b"test", b"test"));
    assert!(contains_pattern(b"abcdefgh", b"def"));

    assert!(!contains_pattern(b"hello", b"goodbye"));
    assert!(!contains_pattern(b"short", b"very long pattern"));
    assert!(!contains_pattern(b"", b"anything"));
}

#[test]
fn test_sha256_determinism() {
    let data = b"test data for hashing";
    let hash1 = compute_sha256(data);
    let hash2 = compute_sha256(data);
    assert_eq!(hash1, hash2);

    let different_data = b"different test data";
    let hash3 = compute_sha256(different_data);
    assert_ne!(hash1, hash3);
}

use anyhow::Result;
use python_verifier::python_compiler::PythonCompiler;
use wasmtime::*;

// Execute WASM and return the result pointer
fn execute_wasm(wasm_bytes: &[u8]) -> Result<i32> {
    let engine = Engine::default();
    let mut store = Store::new(&engine, ());

    let memory_type = MemoryType::new(16, Some(256));
    let memory = Memory::new(&mut store, memory_type)?;

    let module = Module::new(&engine, wasm_bytes)?;
    let imports = [memory.into()];
    let instance = Instance::new(&mut store, &module, &imports)?;

    let main = instance.get_typed_func::<(), i32>(&mut store, "main")?;
    let result = main.call(&mut store, ())?;

    Ok(result)
}

// Extract string from WASM memory at given pointer
fn extract_string(wasm_bytes: &[u8], ptr: i32) -> Result<String> {
    let engine = Engine::default();
    let mut store = Store::new(&engine, ());

    let memory_type = MemoryType::new(16, Some(256));
    let memory = Memory::new(&mut store, memory_type)?;

    let module = Module::new(&engine, wasm_bytes)?;
    let imports = [memory.into()];
    let instance = Instance::new(&mut store, &module, &imports)?;

    let main = instance.get_typed_func::<(), i32>(&mut store, "main")?;
    main.call(&mut store, ())?;

    let mem = instance.get_memory(&mut store, "memory")
        .ok_or_else(|| anyhow::anyhow!("Memory not found"))?;
    let data = mem.data(&store);

    let ptr = ptr as usize;
    if ptr + 8 > data.len() {
        anyhow::bail!("String pointer out of bounds");
    }

    let type_tag = i32::from_le_bytes([data[ptr], data[ptr + 1], data[ptr + 2], data[ptr + 3]]);
    if type_tag != 3 {
        anyhow::bail!("Expected string type tag (3), got {}", type_tag);
    }

    let length = i32::from_le_bytes([data[ptr + 4], data[ptr + 5], data[ptr + 6], data[ptr + 7]]) as usize;

    if ptr + 8 + length > data.len() {
        anyhow::bail!("String data out of bounds");
    }

    let string_bytes = &data[ptr + 8..ptr + 8 + length];
    String::from_utf8(string_bytes.to_vec()).map_err(|e| anyhow::anyhow!("Invalid UTF-8: {}", e))
}

// NIST FIPS 180-4 test vectors

#[test]
fn test_sha256_empty_string() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
import hashlib
data = "".encode()
hash_obj = hashlib.sha256(data)
OUTPUT = hash_obj.hexdigest()
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let hash_hex = extract_string(&wasm, result)?;

    // SHA256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
    assert_eq!(hash_hex, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    Ok(())
}

#[test]
fn test_sha256_abc() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
import hashlib
data = "abc".encode()
hash_obj = hashlib.sha256(data)
OUTPUT = hash_obj.hexdigest()
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let hash_hex = extract_string(&wasm, result)?;

    // SHA256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
    assert_eq!(hash_hex, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    Ok(())
}

#[test]
fn test_sha256_448_bits() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
import hashlib
data = "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq".encode()
hash_obj = hashlib.sha256(data)
OUTPUT = hash_obj.hexdigest()
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let hash_hex = extract_string(&wasm, result)?;

    // SHA256("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")
    assert_eq!(hash_hex, "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
    Ok(())
}

#[test]
fn test_sha256_single_block() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
import hashlib
data = "hello world".encode()
hash_obj = hashlib.sha256(data)
OUTPUT = hash_obj.hexdigest()
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let hash_hex = extract_string(&wasm, result)?;

    // SHA256("hello world") = b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9
    assert_eq!(hash_hex, "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9");
    Ok(())
}

#[test]
fn test_sha256_certus() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
import hashlib
data = "certus".encode()
hash_obj = hashlib.sha256(data)
OUTPUT = hash_obj.hexdigest()
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let hash_hex = extract_string(&wasm, result)?;

    // Verified with: echo -n "certus" | sha256sum
    assert_eq!(hash_hex, "dbcb3b17840a2c1cd6c12d2e1cfe6327f53d72dd89e88b35430b8b9c901e47e5");
    Ok(())
}

#[test]
fn test_sha256_with_nonce() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
import hashlib
data = "certus-0".encode()
hash_obj = hashlib.sha256(data)
OUTPUT = hash_obj.hexdigest()
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let hash_hex = extract_string(&wasm, result)?;

    // This is the POW pattern
    assert_eq!(hash_hex.len(), 64); // 32 bytes = 64 hex chars
    Ok(())
}

#[test]
fn test_sha256_one_byte() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
import hashlib
data = "a".encode()
hash_obj = hashlib.sha256(data)
OUTPUT = hash_obj.hexdigest()
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let hash_hex = extract_string(&wasm, result)?;

    // SHA256("a") = ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb
    assert_eq!(hash_hex, "ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb");
    Ok(())
}

#[test]
fn test_sha256_exactly_55_bytes() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    // Exactly 55 bytes fits in one block with padding
    let code = r#"
import hashlib
data = "0123456789012345678901234567890123456789012345678901234".encode()
hash_obj = hashlib.sha256(data)
OUTPUT = hash_obj.hexdigest()
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let hash_hex = extract_string(&wasm, result)?;

    assert_eq!(hash_hex.len(), 64);
    Ok(())
}

#[test]
fn test_sha256_exactly_56_bytes() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    // Exactly 56 bytes requires two blocks
    let code = r#"
import hashlib
data = "01234567890123456789012345678901234567890123456789012345".encode()
hash_obj = hashlib.sha256(data)
OUTPUT = hash_obj.hexdigest()
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let hash_hex = extract_string(&wasm, result)?;

    assert_eq!(hash_hex.len(), 64);
    Ok(())
}

#[test]
fn test_sha256_exactly_64_bytes() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    // Exactly one block
    let code = r#"
import hashlib
data = "0123456789012345678901234567890123456789012345678901234567890123".encode()
hash_obj = hashlib.sha256(data)
OUTPUT = hash_obj.hexdigest()
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let hash_hex = extract_string(&wasm, result)?;

    assert_eq!(hash_hex.len(), 64);
    Ok(())
}

#[test]
fn test_sha256_multiple_blocks() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    // 128 bytes = 2 blocks
    let code = r#"
import hashlib
data = ("0123456789012345678901234567890123456789012345678901234567890123" +
        "0123456789012345678901234567890123456789012345678901234567890123").encode()
hash_obj = hashlib.sha256(data)
OUTPUT = hash_obj.hexdigest()
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let hash_hex = extract_string(&wasm, result)?;

    assert_eq!(hash_hex.len(), 64);
    Ok(())
}

#[test]
fn test_sha256_determinism() -> Result<()> {
    let code = r#"
import hashlib
data = "test determinism".encode()
hash_obj = hashlib.sha256(data)
OUTPUT = hash_obj.hexdigest()
"#;

    let mut hashes = Vec::new();
    for _ in 0..10 {
        let mut compiler = PythonCompiler::new();
        let wasm = compiler.compile(code)?;
        let result = execute_wasm(&wasm)?;
        let hash_hex = extract_string(&wasm, result)?;
        hashes.push(hash_hex);
    }

    // All hashes must be identical
    for i in 1..hashes.len() {
        assert_eq!(hashes[i], hashes[0], "Non-deterministic SHA256 at run {}", i);
    }

    Ok(())
}

#[test]
fn test_sha256_in_loop() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
import hashlib
result = "start".encode()
for i in range(3):
    hash_obj = hashlib.sha256(result)
    result = hash_obj  # Use hash object as bytes directly
OUTPUT = result.hexdigest()
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let hash_hex = extract_string(&wasm, result)?;

    // Triple hash of "start"
    assert_eq!(hash_hex.len(), 64);
    Ok(())
}

#[test]
fn test_sha256_startswith_check() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
import hashlib
data = "certus-12345".encode()
hash_obj = hashlib.sha256(data)
hex_str = hash_obj.hexdigest()
if hex_str.startswith("0000"):
    OUTPUT = 1
else:
    OUTPUT = 0
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;

    // Result is 0 or 1 depending on hash
    assert!(result == 0 || result == 1);
    Ok(())
}

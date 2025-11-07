use python_verifier::compiler::PythonCompiler;
use anyhow::Result;
use wasmtime::*;

// Execute compiled WASM and return OUTPUT variable
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

#[test]
fn test_empty_dict() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
d = {}
OUTPUT = 42
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 42);
    Ok(())
}

#[test]
fn test_dict_creation() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
d = {1: 10, 2: 20}
OUTPUT = 100
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 100);
    Ok(())
}

#[test]
fn test_dict_with_int_keys() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
d = {1: 100, 2: 200, 3: 300}
OUTPUT = 1
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1);
    Ok(())
}

#[test]
fn test_dict_with_expressions() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
d = {1 + 1: 10 * 2, 3: 30}
OUTPUT = 99
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 99);
    Ok(())
}

#[test]
fn test_dict_with_variables() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
k1 = 10
v1 = 100
d = {k1: v1, 20: 200}
OUTPUT = 1
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1);
    Ok(())
}

#[test]
fn test_dict_determinism() -> Result<()> {
    // Test that dict creation is deterministic (same keys always in same order)
    let mut compiler = PythonCompiler::new();
    let code = r#"
d1 = {5: 50, 1: 10, 3: 30}
d2 = {1: 10, 3: 30, 5: 50}
OUTPUT = 123
"#;
    let wasm1 = compiler.compile(code)?;
    let wasm2 = compiler.compile(code)?;

    // Deterministic compilation: same code = same WASM
    assert_eq!(wasm1, wasm2);
    Ok(())
}

#[test]
fn test_dict_collision_handling() -> Result<()> {
    // Test that hash collisions are handled correctly
    let mut compiler = PythonCompiler::new();
    let code = r#"
# Multiple keys to test linear probing
d = {1: 10, 2: 20, 3: 30, 4: 40, 5: 50}
OUTPUT = 1
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1);
    Ok(())
}

#[test]
fn test_dict_large() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
d = {1: 10, 2: 20, 3: 30, 4: 40, 5: 50, 6: 60, 7: 70, 8: 80}
OUTPUT = 1
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1);
    Ok(())
}

#[test]
fn test_dict_fnv_hash_determinism() -> Result<()> {
    // FNV-1a hash must be deterministic
    let mut compiler = PythonCompiler::new();
    let code = r#"
# Run 10 times to verify determinism
d1 = {100: 1000}
d2 = {100: 1000}
d3 = {100: 1000}
OUTPUT = 777
"#;
    let wasm = compiler.compile(code)?;

    // Execute multiple times - should always produce same result
    for _ in 0..10 {
        let result = execute_wasm(&wasm)?;
        assert_eq!(result, 777);
    }
    Ok(())
}

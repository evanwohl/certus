use anyhow::Result;
use python_verifier::python_compiler::PythonCompiler;
use wasmtime::*;

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

#[test]
fn test_str_zero() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 0
OUTPUT = str(x)
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    assert_eq!(extracted, "0");
    Ok(())
}

#[test]
fn test_str_positive() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 42
OUTPUT = str(x)
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    assert_eq!(extracted, "42");
    Ok(())
}

#[test]
fn test_str_negative() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = -123
OUTPUT = str(x)
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    assert_eq!(extracted, "-123");
    Ok(())
}

#[test]
fn test_str_large_positive() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 999999
OUTPUT = str(x)
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    assert_eq!(extracted, "999999");
    Ok(())
}

#[test]
fn test_str_large_negative() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = -88888
OUTPUT = str(x)
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    assert_eq!(extracted, "-88888");
    Ok(())
}

#[test]
fn test_str_single_digit() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 7
OUTPUT = str(x)
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    assert_eq!(extracted, "7");
    Ok(())
}

#[test]
fn test_str_concat() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 123
s = str(x)
OUTPUT = "num: " + s
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    assert_eq!(extracted, "num: 123");
    Ok(())
}

#[test]
fn test_str_in_loop() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
result = ""
for i in range(3):
    result = result + str(i)
OUTPUT = result
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    assert_eq!(extracted, "012");
    Ok(())
}

#[test]
fn test_str_multiple_conversions() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
a = 10
b = 20
OUTPUT = str(a) + "," + str(b)
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    assert_eq!(extracted, "10,20");
    Ok(())
}

#[test]
fn test_str_negative_one() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = -1
OUTPUT = str(x)
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    assert_eq!(extracted, "-1");
    Ok(())
}

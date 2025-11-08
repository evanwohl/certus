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

// Extract string from WASM memory
fn extract_string(wasm_bytes: &[u8], output: i32) -> Result<String> {
    let engine = Engine::default();
    let mut store = Store::new(&engine, ());

    let memory_type = MemoryType::new(16, Some(256));
    let memory = Memory::new(&mut store, memory_type)?;

    let module = Module::new(&engine, wasm_bytes)?;

    let imports = [memory.into()];
    let instance = Instance::new(&mut store, &module, &imports)?;

    let main = instance.get_typed_func::<(), i32>(&mut store, "main")?;
    main.call(&mut store, ())?;

    let memory = instance.get_memory(&mut store, "memory")
        .ok_or_else(|| anyhow::anyhow!("Memory not found"))?;
    let data = memory.data(&store);

    // String layout: [type:i32][length:i32][bytes...]
    let str_ptr = output as usize;
    if str_ptr + 8 > data.len() {
        return Err(anyhow::anyhow!("String pointer out of bounds"));
    }

    let length = i32::from_le_bytes([
        data[str_ptr + 4],
        data[str_ptr + 5],
        data[str_ptr + 6],
        data[str_ptr + 7],
    ]) as usize;

    if str_ptr + 8 + length > data.len() {
        return Err(anyhow::anyhow!("String data out of bounds"));
    }

    let bytes = &data[str_ptr + 8..str_ptr + 8 + length];
    Ok(String::from_utf8_lossy(bytes).to_string())
}

#[test]
fn test_empty_string() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s = ""
OUTPUT = 0
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 0);
    Ok(())
}

#[test]
fn test_string_literal() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s = "hello"
OUTPUT = s
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    assert_eq!(extracted, "hello");
    Ok(())
}

#[test]
fn test_string_literal_longer() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s = "certus deterministic compute"
OUTPUT = s
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    assert_eq!(extracted, "certus deterministic compute");
    Ok(())
}

#[test]
fn test_string_slice_full() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s = "hello"
OUTPUT = s[:]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    assert_eq!(extracted, "hello");
    Ok(())
}

#[test]
fn test_string_slice_start() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s = "hello"
OUTPUT = s[2:]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    assert_eq!(extracted, "llo");
    Ok(())
}

#[test]
fn test_string_slice_end() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s = "hello"
OUTPUT = s[:3]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    assert_eq!(extracted, "hel");
    Ok(())
}

#[test]
fn test_string_slice_middle() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s = "hello"
OUTPUT = s[1:4]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    assert_eq!(extracted, "ell");
    Ok(())
}

#[test]
fn test_string_slice_single_char() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s = "hello"
OUTPUT = s[1:2]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    assert_eq!(extracted, "e");
    Ok(())
}

#[test]
fn test_string_slice_empty() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s = "hello"
OUTPUT = s[2:2]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    assert_eq!(extracted, "");
    Ok(())
}

#[test]
fn test_string_slice_negative_start_clamped() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s = "hello"
OUTPUT = s[-10:3]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    // Negative start should clamp to 0
    assert_eq!(extracted, "hel");
    Ok(())
}

#[test]
fn test_string_slice_overflow_end() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s = "hello"
OUTPUT = s[2:100]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    // Overflow should clamp to length
    assert_eq!(extracted, "llo");
    Ok(())
}

#[test]
fn test_string_slice_first_char() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s = "hello"
OUTPUT = s[:1]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    assert_eq!(extracted, "h");
    Ok(())
}

#[test]
fn test_string_slice_last_char() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s = "hello"
OUTPUT = s[4:]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    assert_eq!(extracted, "o");
    Ok(())
}

#[test]
fn test_string_concat_simple() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s1 = "hello"
s2 = "world"
OUTPUT = s1 + s2
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    assert_eq!(extracted, "helloworld");
    Ok(())
}

#[test]
fn test_string_concat_empty() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s1 = ""
s2 = "test"
OUTPUT = s1 + s2
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    assert_eq!(extracted, "test");
    Ok(())
}

#[test]
fn test_string_concat_multiple() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s1 = "a"
s2 = "b"
s3 = "c"
OUTPUT = s1 + s2 + s3
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    assert_eq!(extracted, "abc");
    Ok(())
}

#[test]
fn test_string_concat_with_slice() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s1 = "hello"
s2 = "world"
OUTPUT = s1[:2] + s2[2:]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    assert_eq!(extracted, "herld");
    Ok(())
}

#[test]
fn test_string_equals_true() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s1 = "hello"
s2 = "hello"
OUTPUT = 1 if s1 == s2 else 0
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1);
    Ok(())
}

#[test]
fn test_string_equals_false() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s1 = "hello"
s2 = "world"
OUTPUT = 1 if s1 == s2 else 0
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 0);
    Ok(())
}

#[test]
fn test_string_equals_length_diff() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s1 = "hi"
s2 = "hello"
OUTPUT = 1 if s1 == s2 else 0
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 0);
    Ok(())
}

#[test]
fn test_string_equals_empty() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s1 = ""
s2 = ""
OUTPUT = 1 if s1 == s2 else 0
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1);
    Ok(())
}

#[test]
fn test_string_equals_concat() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s1 = "hello"
s2 = "hel" + "lo"
OUTPUT = 1 if s1 == s2 else 0
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1);
    Ok(())
}

#[test]
fn test_string_index_first() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s = "hello"
OUTPUT = s[0]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, b'h' as i32);
    Ok(())
}

#[test]
fn test_string_index_middle() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s = "hello"
OUTPUT = s[2]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, b'l' as i32);
    Ok(())
}

#[test]
fn test_string_index_last() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s = "hello"
OUTPUT = s[4]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, b'o' as i32);
    Ok(())
}

#[test]
fn test_string_index_concat() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s1 = "ab"
s2 = "cd"
s3 = s1 + s2
OUTPUT = s3[2]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, b'c' as i32);
    Ok(())
}

#[test]
fn test_string_operations_combined() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
s1 = "certus"
s2 = "protocol"
full = s1 + s2
slice = full[0:6]
OUTPUT = 1 if slice == "certus" else 0
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1);
    Ok(())
}

#[test]
fn test_string_loop_concat() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
result = ""
for i in range(3):
    result = result + "a"
OUTPUT = result
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    let extracted = extract_string(&wasm, result)?;
    assert_eq!(extracted, "aaa");
    Ok(())
}

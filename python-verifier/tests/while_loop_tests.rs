use python_verifier::python_compiler::PythonCompiler;
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
fn test_simple_while() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
count = 0
while count < 5:
    count = count + 1
OUTPUT = count
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 5);
    Ok(())
}

#[test]
fn test_while_with_accumulator() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
total = 0
i = 0
while i < 10:
    total = total + i
    i = i + 1
OUTPUT = total
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 45);  // 0+1+2+...+9 = 45
    Ok(())
}

#[test]
fn test_while_condition_false_initially() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 10
while x < 5:
    x = x + 1
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 10);  // Loop never executes
    Ok(())
}

#[test]
fn test_nested_while_loops() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
total = 0
outer = 0
while outer < 3:
    inner = 0
    while inner < 3:
        total = total + 1
        inner = inner + 1
    outer = outer + 1
OUTPUT = total
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 9);  // 3×3 = 9
    Ok(())
}

#[test]
fn test_while_with_comparison_ops() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 100
while x > 90:
    x = x - 1
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 90);
    Ok(())
}

#[test]
fn test_while_with_le() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 0
while x <= 5:
    x = x + 1
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 6);
    Ok(())
}

#[test]
fn test_while_with_ge() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 10
while x >= 5:
    x = x - 1
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 4);
    Ok(())
}

#[test]
fn test_while_with_ne() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 0
while x != 5:
    x = x + 1
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 5);
    Ok(())
}

#[test]
fn test_while_with_eq() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 0
y = 1
while x == 0:
    x = y
    y = y + 1
OUTPUT = y
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 2);
    Ok(())
}

#[test]
fn test_while_with_arithmetic_condition() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 0
while x + 5 < 10:
    x = x + 1
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 5);
    Ok(())
}

#[test]
fn test_while_counting_down() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
count = 10
total = 0
while count > 0:
    total = total + count
    count = count - 1
OUTPUT = total
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 55);  // 10+9+8+...+1 = 55
    Ok(())
}

#[test]
fn test_while_with_modulo() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 0
count = 0
while x < 20:
    if x % 3 == 0:
        count = count + 1
    x = x + 1
OUTPUT = count
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 7);  // 0, 3, 6, 9, 12, 15, 18
    Ok(())
}

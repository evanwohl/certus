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
fn test_empty_list() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = []
OUTPUT = 0
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 0);
    Ok(())
}

#[test]
fn test_list_creation() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [1, 2, 3]
OUTPUT = 42
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 42);
    Ok(())
}

#[test]
fn test_list_subscript() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [10, 20, 30]
OUTPUT = x[1]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 20);
    Ok(())
}

#[test]
fn test_list_first_element() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [100, 200, 300]
OUTPUT = x[0]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 100);
    Ok(())
}

#[test]
fn test_list_last_element() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [5, 10, 15, 20]
OUTPUT = x[3]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 20);
    Ok(())
}

#[test]
fn test_list_with_expressions() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [1 + 2, 3 * 4, 10 - 5]
OUTPUT = x[1]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 12);
    Ok(())
}

#[test]
fn test_list_with_variables() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
a = 10
b = 20
c = 30
x = [a, b, c]
OUTPUT = x[2]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 30);
    Ok(())
}

#[test]
fn test_list_computed_index() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [100, 200, 300, 400]
i = 1 + 1
OUTPUT = x[i]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 300);
    Ok(())
}

#[test]
fn test_nested_list_operations() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [1, 2, 3]
y = [4, 5, 6]
a = x[0]
b = y[1]
OUTPUT = a + b
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 6); // 1 + 5
    Ok(())
}

#[test]
fn test_list_in_loop() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [10, 20, 30, 40, 50]
total = 0
for i in range(5):
    total = total + x[i]
OUTPUT = total
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 150); // 10+20+30+40+50
    Ok(())
}

#[test]
fn test_list_sum_partial() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
nums = [5, 10, 15, 20, 25]
total = nums[0] + nums[2] + nums[4]
OUTPUT = total
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 45); // 5+15+25
    Ok(())
}

#[test]
fn test_list_max_element() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
nums = [3, 7, 2, 9, 4]
max_val = nums[0]
for i in range(5):
    if nums[i] > max_val:
        max_val = nums[i]
OUTPUT = max_val
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 9);
    Ok(())
}

#[test]
fn test_list_element_swap_logic() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
a = [1, 2]
x = a[0]
y = a[1]
result = y * 10 + x
OUTPUT = result
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 21); // 2*10 + 1
    Ok(())
}

#[test]
fn test_list_large() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
OUTPUT = x[7]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 8);
    Ok(())
}

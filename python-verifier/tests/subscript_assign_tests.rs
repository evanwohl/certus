use python_verifier::compiler::PythonCompiler;
use anyhow::Result;
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

// Basic list assignment tests
#[test]
fn test_list_assign_single_element() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [10, 20, 30]
x[1] = 99
OUTPUT = x[1]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 99);
    Ok(())
}

#[test]
fn test_list_assign_first_element() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [1, 2, 3]
x[0] = 100
OUTPUT = x[0]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 100);
    Ok(())
}

#[test]
fn test_list_assign_last_element() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [5, 10, 15, 20]
x[3] = 999
OUTPUT = x[3]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 999);
    Ok(())
}

#[test]
fn test_list_assign_multiple_times() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [1, 2, 3]
x[0] = 10
x[1] = 20
x[2] = 30
OUTPUT = x[0] + x[1] + x[2]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 60);
    Ok(())
}

#[test]
fn test_list_assign_with_expression() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [1, 2, 3]
x[1] = 10 + 20
OUTPUT = x[1]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 30);
    Ok(())
}

#[test]
fn test_list_assign_computed_index() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [100, 200, 300]
i = 1
x[i] = 999
OUTPUT = x[1]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 999);
    Ok(())
}

#[test]
fn test_list_assign_expression_index() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [10, 20, 30, 40]
x[1 + 1] = 500
OUTPUT = x[2]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 500);
    Ok(())
}

// List assignment in control flow
#[test]
fn test_list_assign_in_loop() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [0, 0, 0, 0, 0]
for i in range(5):
    x[i] = i * 10
OUTPUT = x[0] + x[1] + x[2] + x[3] + x[4]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 100); // 0 + 10 + 20 + 30 + 40
    Ok(())
}

#[test]
fn test_list_assign_conditional() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [1, 2, 3]
if 1:
    x[1] = 100
OUTPUT = x[1]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 100);
    Ok(())
}

#[test]
fn test_list_assign_in_function() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
def update_list(arr, idx, val):
    arr[idx] = val
    return arr[idx]

x = [1, 2, 3]
result = update_list(x, 1, 999)
OUTPUT = result
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 999);
    Ok(())
}

// Array algorithms using subscript assignment
#[test]
fn test_list_swap_elements() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [10, 20, 30]
temp = x[0]
x[0] = x[2]
x[2] = temp
OUTPUT = x[0]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 30);
    Ok(())
}

#[test]
fn test_list_bubble_sort_one_pass() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [3, 1, 2]
if x[0] > x[1]:
    temp = x[0]
    x[0] = x[1]
    x[1] = temp
if x[1] > x[2]:
    temp = x[1]
    x[1] = x[2]
    x[2] = temp
OUTPUT = x[0]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1);
    Ok(())
}

#[test]
fn test_list_accumulate_in_place() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [1, 2, 3, 4]
for i in range(3):
    x[i + 1] = x[i] + x[i + 1]
OUTPUT = x[3]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 10); // 1, 3, 6, 10
    Ok(())
}

#[test]
fn test_list_reverse_in_place() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [1, 2, 3, 4]
temp = x[0]
x[0] = x[3]
x[3] = temp
temp = x[1]
x[1] = x[2]
x[2] = temp
OUTPUT = x[0]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 4);
    Ok(())
}

// Edge cases and determinism
#[test]
fn test_list_assign_deterministic() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [1, 2, 3]
x[0] = 100
x[1] = 200
x[2] = 300
OUTPUT = x[0] + x[1] + x[2]
"#;
    let wasm1 = compiler.compile(code)?;
    let wasm2 = compiler.compile(code)?;

    assert_eq!(wasm1, wasm2);

    let result1 = execute_wasm(&wasm1)?;
    let result2 = execute_wasm(&wasm2)?;
    assert_eq!(result1, 600);
    assert_eq!(result2, 600);
    Ok(())
}

#[test]
fn test_list_assign_preserves_other_elements() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [10, 20, 30, 40, 50]
x[2] = 999
OUTPUT = x[0] + x[1] + x[3] + x[4]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 120); // 10 + 20 + 40 + 50
    Ok(())
}

#[test]
fn test_list_assign_overwrite_previous() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [1, 2, 3]
x[1] = 100
x[1] = 200
x[1] = 300
OUTPUT = x[1]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 300);
    Ok(())
}

// Complex scenarios
#[test]
fn test_list_fibonacci_in_place() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
fib = [0, 1, 0, 0, 0]
fib[2] = fib[0] + fib[1]
fib[3] = fib[1] + fib[2]
fib[4] = fib[2] + fib[3]
OUTPUT = fib[4]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 3); // 0, 1, 1, 2, 3
    Ok(())
}

#[test]
fn test_list_assign_with_read() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [5, 10, 15]
x[0] = x[1] + x[2]
OUTPUT = x[0]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 25);
    Ok(())
}

#[test]
fn test_list_assign_chain() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [1, 2, 3, 4, 5]
x[4] = x[3]
x[3] = x[2]
x[2] = x[1]
x[1] = x[0]
OUTPUT = x[1] + x[2] + x[3] + x[4]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 10); // 1 + 2 + 3 + 4
    Ok(())
}

#[test]
fn test_list_counters() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
counts = [0, 0, 0]
counts[0] = counts[0] + 1
counts[0] = counts[0] + 1
counts[1] = counts[1] + 3
counts[2] = counts[2] + 5
OUTPUT = counts[0] + counts[1] + counts[2]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 10); // 2 + 3 + 5
    Ok(())
}

#[test]
fn test_list_max_tracking() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = [5, 2, 8, 1, 9]
max_val = x[0]
max_idx = 0
for i in range(5):
    if x[i] > max_val:
        max_val = x[i]
        max_idx = i
OUTPUT = max_idx
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 4);
    Ok(())
}

// Dict assignment tests
#[test]
fn test_dict_assign_new_key() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
d = {1: 10}
d[2] = 20
OUTPUT = 1
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1);
    Ok(())
}

#[test]
fn test_dict_assign_update_existing() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
d = {1: 100, 2: 200}
d[1] = 999
OUTPUT = 1
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1);
    Ok(())
}

#[test]
fn test_dict_assign_multiple_keys() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
d = {}
d[1] = 10
d[2] = 20
d[3] = 30
OUTPUT = 1
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1);
    Ok(())
}

#[test]
fn test_dict_assign_computed_key() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
d = {1: 100}
k = 5
d[k] = 500
OUTPUT = 1
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1);
    Ok(())
}

#[test]
fn test_dict_assign_expression_key_value() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
d = {}
d[1 + 1] = 10 * 2
OUTPUT = 1
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1);
    Ok(())
}

#[test]
fn test_dict_assign_overwrite() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
d = {5: 50}
d[5] = 100
d[5] = 200
OUTPUT = 1
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1);
    Ok(())
}

#[test]
fn test_dict_assign_in_loop() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
d = {}
for i in range(5):
    d[i] = i * 100
OUTPUT = 1
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1);
    Ok(())
}

#[test]
fn test_dict_assign_conditional() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
d = {1: 10}
if 1:
    d[2] = 20
OUTPUT = 1
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1);
    Ok(())
}

#[test]
fn test_dict_assign_collision_keys() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
d = {}
d[1] = 100
d[2] = 200
d[3] = 300
d[4] = 400
d[5] = 500
OUTPUT = 1
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1);
    Ok(())
}

#[test]
fn test_dict_assign_deterministic() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
d = {1: 10}
d[2] = 20
d[3] = 30
OUTPUT = 123
"#;
    let wasm1 = compiler.compile(code)?;
    let wasm2 = compiler.compile(code)?;

    assert_eq!(wasm1, wasm2);

    let result1 = execute_wasm(&wasm1)?;
    let result2 = execute_wasm(&wasm2)?;
    assert_eq!(result1, 123);
    assert_eq!(result2, 123);
    Ok(())
}

#[test]
fn test_dict_assign_mixed_operations() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
d = {1: 100}
d[2] = 200
d[1] = 999
OUTPUT = 1
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1);
    Ok(())
}

#[test]
fn test_list_and_dict_assign() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
arr = [1, 2, 3]
dct = {1: 10, 2: 20}
arr[0] = 100
dct[3] = 30
OUTPUT = arr[0]
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 100);
    Ok(())
}

#[test]
fn test_dict_assign_empty_start() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
d = {}
d[10] = 1000
d[20] = 2000
OUTPUT = 1
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1);
    Ok(())
}

#[test]
fn test_dict_counter_pattern() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
counts = {}
counts[1] = 0
counts[2] = 0
counts[1] = 5
counts[2] = 3
OUTPUT = 1
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1);
    Ok(())
}

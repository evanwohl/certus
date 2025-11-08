use anyhow::Result;
use python_verifier::python_compiler::PythonCompiler;
use wasmtime::*;

// Execute WASM and return the result
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

// Verify determinism: compile N times and ensure identical WASM output
fn verify_determinism(code: &str, runs: usize) -> Result<()> {
    let mut wasms = Vec::new();

    for _ in 0..runs {
        let mut compiler = PythonCompiler::new();
        let wasm = compiler.compile(code)?;
        wasms.push(wasm);
    }

    // All WASM outputs must be identical
    for i in 1..wasms.len() {
        if wasms[i] != wasms[0] {
            anyhow::bail!("Non-deterministic compilation detected at run {}", i);
        }
    }

    Ok(())
}

// WHITE-BOX TESTS: Test individual operators with edge cases

#[test]
fn test_add_assign_basic() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 10
x += 5
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 15);
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_add_assign_zero() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 42
x += 0
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 42);
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_add_assign_negative() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 10
x += -15
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, -5);
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_sub_assign_basic() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 20
x -= 7
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 13);
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_sub_assign_negative_result() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 5
x -= 10
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, -5);
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_mul_assign_basic() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 6
x *= 7
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 42);
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_mul_assign_zero() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 999
x *= 0
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 0);
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_mul_assign_negative() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 8
x *= -3
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, -24);
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_div_assign_basic() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 20
x /= 4
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 5);
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_div_assign_truncation() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 17
x /= 5
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 3); // Truncating division
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_floordiv_assign_basic() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 23
x //= 5
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 4);
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_floordiv_assign_exact() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 100
x //= 10
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 10);
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_mod_assign_basic() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 17
x %= 5
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 2);
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_mod_assign_zero_remainder() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 20
x %= 5
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 0);
    verify_determinism(code, 10)?;
    Ok(())
}

// BLACK-BOX TESTS: Test complex realistic scenarios

#[test]
fn test_counter_loop() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
counter = 0
for i in range(10):
    counter += 1
OUTPUT = counter
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 10);
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_accumulator() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
total = 0
for i in range(5):
    total += i
OUTPUT = total
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 0 + 1 + 2 + 3 + 4);
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_factorial_with_mul_assign() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
result = 1
for i in range(1, 6):
    result *= i
OUTPUT = result
"#;
    // Note: range(1, 6) not yet supported, so we use workaround
    let code = r#"
result = 1
n = 5
for i in range(n):
    result *= (i + 1)
OUTPUT = result
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 120); // 5! = 120
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_countdown() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 100
for i in range(10):
    x -= 5
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 50);
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_mixed_operations() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 10
x += 5      # x = 15
x *= 2      # x = 30
x -= 10     # x = 20
x //= 4     # x = 5
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 5);
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_chained_in_loop() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
a = 0
b = 1
for i in range(5):
    a += i
    b *= 2
OUTPUT = a + b
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    // a = 0+1+2+3+4 = 10
    // b = 1*2*2*2*2*2 = 32
    assert_eq!(result, 42);
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_conditional_increment() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
count = 0
for i in range(20):
    if i % 2 == 0:
        count += 1
OUTPUT = count
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 10); // Even numbers: 0,2,4,6,8,10,12,14,16,18
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_while_with_increment() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 0
while x < 10:
    x += 2
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 10);
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_nested_loop_counter() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
total = 0
for i in range(5):
    for j in range(3):
        total += 1
OUTPUT = total
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 15); // 5 * 3
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_multiple_variables() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
a = 10
b = 20
c = 5
a += b
b -= c
c *= 2
OUTPUT = a + b + c
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    // a = 10 + 20 = 30
    // b = 20 - 5 = 15
    // c = 5 * 2 = 10
    assert_eq!(result, 55);
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_pow_style_increment() -> Result<()> {
    // Simulates POW-style nonce increment pattern
    let mut compiler = PythonCompiler::new();
    let code = r#"
nonce = 0
found = 0
while nonce < 100:
    if nonce % 17 == 0:
        found = nonce
        break
    nonce += 1
OUTPUT = found
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 0); // First multiple of 17 in range
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_sum_of_squares() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
total = 0
for i in range(5):
    square = i * i
    total += square
OUTPUT = total
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 0 + 1 + 4 + 9 + 16); // 30
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_division_accumulation() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 1000
for i in range(3):
    x //= 10
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1); // 1000 / 10 / 10 / 10 = 1
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_modulo_cycle() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 0
for i in range(25):
    x += 1
    x %= 7
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 4); // 25 % 7 = 4
    verify_determinism(code, 10)?;
    Ok(())
}

// EDGE CASE TESTS: Boundary conditions and stress tests

#[test]
fn test_large_increment() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 1000000
x += 999999
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1999999);
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_negative_accumulation() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 0
for i in range(10):
    x -= 5
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, -50);
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_single_operation() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 42
x += 1
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 43);
    verify_determinism(code, 10)?;
    Ok(())
}

#[test]
fn test_identity_operations() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 100
x += 0
x -= 0
x *= 1
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 100);
    verify_determinism(code, 10)?;
    Ok(())
}

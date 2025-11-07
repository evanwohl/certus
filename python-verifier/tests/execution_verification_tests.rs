
use python_verifier::python_compiler::PythonCompiler;
use anyhow::Result;
use wasmtime::*;

fn execute_wasm(wasm_bytes: &[u8]) -> Result<i32> {
    let engine = Engine::default();
    let mut store = Store::new(&engine, ());

    // Create memory for the module
    let memory_type = MemoryType::new(16, Some(256));
    let memory = Memory::new(&mut store, memory_type)?;

    let module = Module::new(&engine, wasm_bytes)?;

    let imports = [memory.into()];
    let instance = Instance::new(&mut store, &module, &imports)?;

    let main = instance.get_typed_func::<(), i32>(&mut store, "main")?;
    let result = main.call(&mut store, ())?;

    Ok(result)
}

// ARITHMETIC VERIFICATION

#[test]
fn verify_simple_addition() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
OUTPUT = 10 + 20
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 30, "10 + 20 should equal 30");
    Ok(())
}

#[test]
fn verify_subtraction() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
OUTPUT = 100 - 42
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 58, "100 - 42 should equal 58");
    Ok(())
}

#[test]
fn verify_multiplication() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
OUTPUT = 7 * 6
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 42, "7 * 6 should equal 42");
    Ok(())
}

#[test]
fn verify_integer_division() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
OUTPUT = 100 / 3
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 33, "100 / 3 should equal 33 (integer division)");
    Ok(())
}

#[test]
fn verify_floor_division() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
OUTPUT = 100 // 3
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 33, "100 // 3 should equal 33");
    Ok(())
}

#[test]
fn verify_modulo() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
OUTPUT = 100 % 7
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 2, "100 % 7 should equal 2");
    Ok(())
}

#[test]
fn verify_negative_modulo() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
OUTPUT = -10 % 3
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    // Python's modulo: -10 % 3 = 2 (not -1 like C)
    assert_eq!(result, 2, "-10 % 3 should equal 2 (Python semantics)");
    Ok(())
}

#[test]
fn verify_complex_expression() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
OUTPUT = (10 + 20) * 3 - 15
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 75, "(10 + 20) * 3 - 15 should equal 75");
    Ok(())
}

// UNARY OPERATORS

#[test]
fn verify_negation() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 42
OUTPUT = -x
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, -42, "negation of 42 should be -42");
    Ok(())
}

#[test]
fn verify_not_operator() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
OUTPUT = not 0
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1, "not 0 should be 1 (True)");
    Ok(())
}

#[test]
fn verify_not_nonzero() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
OUTPUT = not 42
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 0, "not 42 should be 0 (False)");
    Ok(())
}

// COMPARISONS

#[test]
fn verify_equality() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
OUTPUT = (10 == 10) + (10 == 5)
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1, "(10 == 10) is True, (10 == 5) is False");
    Ok(())
}

#[test]
fn verify_less_than() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
OUTPUT = (5 < 10) + (10 < 5)
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1, "(5 < 10) is True, (10 < 5) is False");
    Ok(())
}

#[test]
fn verify_greater_equal() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
OUTPUT = (10 >= 10) + (9 >= 10)
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1, "(10 >= 10) is True, (9 >= 10) is False");
    Ok(())
}

// CONTROL FLOW - IF

#[test]
fn verify_if_true_branch() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 100
if x > 50:
    OUTPUT = 1
else:
    OUTPUT = 0
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1, "x > 50 is true, should take then branch");
    Ok(())
}

#[test]
fn verify_if_false_branch() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 10
if x > 50:
    OUTPUT = 1
else:
    OUTPUT = 0
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 0, "x > 50 is false, should take else branch");
    Ok(())
}

#[test]
fn verify_nested_if() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 75
if x > 50:
    if x > 80:
        OUTPUT = 100
    else:
        OUTPUT = 75
else:
    OUTPUT = 50
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 75, "x=75 should hit nested else branch");
    Ok(())
}

// CONTROL FLOW - WHILE

#[test]
fn verify_while_loop_sum() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
i = 0
total = 0
while i < 10:
    total = total + i
    i = i + 1
OUTPUT = total
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 45, "sum(0..9) should equal 45");
    Ok(())
}

#[test]
fn verify_while_countdown() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
n = 100
count = 0
while n > 0:
    n = n - 7
    count = count + 1
OUTPUT = count
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 15, "100 / 7 = 14.28, so 15 iterations");
    Ok(())
}

// CONTROL FLOW - FOR

#[test]
fn verify_for_loop_sum() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
total = 0
for i in range(10):
    total = total + i
OUTPUT = total
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 45, "sum(0..9) should equal 45");
    Ok(())
}

#[test]
fn verify_for_loop_product() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
product = 1
for i in range(5):
    product = product * (i + 1)
OUTPUT = product
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 120, "5! should equal 120");
    Ok(())
}

#[test]
fn verify_nested_for_loops() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
total = 0
for i in range(5):
    for j in range(3):
        total = total + 1
OUTPUT = total
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 15, "5 * 3 iterations should give 15");
    Ok(())
}

// FUNCTIONS

#[test]
fn verify_simple_function() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
def add(a, b):
    return a + b

OUTPUT = add(17, 25)
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 42, "add(17, 25) should equal 42");
    Ok(())
}

#[test]
fn verify_function_with_conditionals() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
def max_of_two(a, b):
    if a > b:
        return a
    else:
        return b

OUTPUT = max_of_two(42, 100)
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 100, "max(42, 100) should equal 100");
    Ok(())
}

#[test]
fn verify_function_with_loop() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
def sum_to_n(n):
    total = 0
    for i in range(n):
        total = total + i
    return total

OUTPUT = sum_to_n(10)
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 45, "sum_to_n(10) should equal 45");
    Ok(())
}

// RECURSION

#[test]
fn verify_factorial_recursion() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
def factorial(n):
    if n <= 1:
        return 1
    return n * factorial(n - 1)

OUTPUT = factorial(5)
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 120, "5! should equal 120");
    Ok(())
}

#[test]
fn verify_fibonacci_recursion() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
def fib(n):
    if n <= 1:
        return n
    return fib(n - 1) + fib(n - 2)

OUTPUT = fib(10)
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 55, "fib(10) should equal 55");
    Ok(())
}

#[test]
fn verify_gcd_recursion() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
def gcd(a, b):
    if b == 0:
        return a
    return gcd(b, a % b)

OUTPUT = gcd(48, 18)
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 6, "gcd(48, 18) should equal 6");
    Ok(())
}

// COMPLEX ALGORITHMS

#[test]
fn verify_is_prime() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
def is_prime(n):
    if n < 2:
        return 0
    i = 2
    while i * i <= n:
        if n % i == 0:
            return 0
        i = i + 1
    return 1

OUTPUT = is_prime(97)
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1, "97 is prime");
    Ok(())
}

#[test]
fn verify_not_prime() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
def is_prime(n):
    if n < 2:
        return 0
    i = 2
    while i * i <= n:
        if n % i == 0:
            return 0
        i = i + 1
    return 1

OUTPUT = is_prime(100)
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 0, "100 is not prime");
    Ok(())
}

#[test]
fn verify_collatz_steps() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
def collatz_steps(n):
    steps = 0
    while n != 1:
        if n % 2 == 0:
            n = n / 2
        else:
            n = 3 * n + 1
        steps = steps + 1
    return steps

OUTPUT = collatz_steps(27)
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 111, "Collatz(27) takes 111 steps");
    Ok(())
}

#[test]
fn verify_power_function() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
def power(base, exp):
    if exp == 0:
        return 1
    result = base
    for i in range(exp - 1):
        result = result * base
    return result

OUTPUT = power(2, 10)
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 1024, "2^10 should equal 1024");
    Ok(())
}

#[test]
fn verify_sum_of_squares() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
def square(n):
    return n * n

def sum_of_squares(n):
    total = 0
    for i in range(n):
        total = total + square(i)
    return total

OUTPUT = sum_of_squares(10)
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 285, "sum of squares 0..9 should equal 285");
    Ok(())
}

// EDGE CASES

#[test]
fn verify_division_by_negative() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
OUTPUT = 10 / (-3)
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, -3, "10 / -3 should equal -3 (integer division)");
    Ok(())
}

#[test]
fn verify_negative_floor_division() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
OUTPUT = (-10) // 3
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, -4, "-10 // 3 should equal -4 (Python semantics)");
    Ok(())
}

#[test]
fn verify_zero_result() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
OUTPUT = 5 - 5
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 0, "5 - 5 should equal 0");
    Ok(())
}

#[test]
fn verify_boolean_arithmetic() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
a = True
b = False
OUTPUT = a + b + 10
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 11, "True + False + 10 should equal 11");
    Ok(())
}

// DETERMINISM VERIFICATION

#[test]
fn verify_deterministic_execution() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
def compute(n):
    total = 0
    for i in range(n):
        total = total + i * i
    return total

OUTPUT = compute(100)
"#;
    let wasm = compiler.compile(code)?;

    // Run 10 times, should always get same result
    let mut results = Vec::new();
    for _ in 0..10 {
        results.push(execute_wasm(&wasm)?);
    }

    assert!(results.iter().all(|&r| r == results[0]),
            "Execution should be deterministic");
    assert_eq!(results[0], 328350, "sum of squares 0..99 should equal 328350");
    Ok(())
}

#[test]
fn verify_tuple_unpacking() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
a, b = 10, 20
OUTPUT = a + b
"#;
    let wasm = compiler.compile(code)?;
    let result = execute_wasm(&wasm)?;
    assert_eq!(result, 30, "tuple unpacking: a=10, b=20, a+b=30");
    Ok(())
}

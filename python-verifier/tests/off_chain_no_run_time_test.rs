use python_verifier::python_compiler::PythonCompiler;
use anyhow::Result;

// BASIC ARITHMETIC AND ASSIGNMENT

#[test]
fn test_simple_assignment() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 42
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_multiple_assignments() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
a = 10
b = 20
c = 30
OUTPUT = a
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_chained_arithmetic() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 1 + 2 + 3 + 4 + 5
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_complex_expressions() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 10
y = 20
z = 30
result = (x + y) * z - (x * y) / (z + 1)
OUTPUT = result
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_all_binary_operators() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
a = 100
b = 30
add_result = a + b
sub_result = a - b
mul_result = a * b
div_result = a / b
floor_div_result = a // b
mod_result = a % b
OUTPUT = add_result
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_negative_numbers() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = -42
y = -10 + 5
z = 100 - 200
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

// COMPARISON OPERATIONS

#[test]
fn test_simple_comparisons() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 10
y = 20
eq = x == y
ne = x != y
lt = x < y
le = x <= y
gt = x > y
ge = x >= y
OUTPUT = eq
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_comparison_in_expression() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 100
y = 50
result = (x > y) + (x < y)
OUTPUT = result
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

// BOOLEAN VALUES

#[test]
fn test_boolean_literals() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
a = True
b = False
OUTPUT = a
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_boolean_arithmetic() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
a = True
b = False
result = a + b + 10
OUTPUT = result
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

// CONTROL FLOW - IF STATEMENTS

#[test]
fn test_simple_if() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 100
if x > 50:
    OUTPUT = 1
else:
    OUTPUT = 0
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_nested_if() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 75
if x > 50:
    if x > 80:
        OUTPUT = 3
    else:
        OUTPUT = 2
else:
    OUTPUT = 1
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_if_with_comparisons() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 42
y = 100
if x < y:
    result = 1
else:
    result = 0
OUTPUT = result
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

// CONTROL FLOW - WHILE LOOPS

#[test]
fn test_simple_while_loop() -> Result<()> {
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
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_while_with_complex_condition() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 100
count = 0
while x > 10:
    x = x - 7
    count = count + 1
OUTPUT = count
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_nested_while_loops() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
i = 0
total = 0
while i < 5:
    j = 0
    while j < 3:
        total = total + 1
        j = j + 1
    i = i + 1
OUTPUT = total
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

// CONTROL FLOW - FOR LOOPS

#[test]
fn test_simple_for_loop() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
total = 0
for i in range(10):
    total = total + i
OUTPUT = total
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_for_with_large_range() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
count = 0
for i in range(1000):
    count = count + 1
OUTPUT = count
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_nested_for_loops() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
total = 0
for i in range(5):
    for j in range(3):
        total = total + i + j
OUTPUT = total
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_for_with_dynamic_range() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
n = 15
total = 0
for i in range(n):
    total = total + i
OUTPUT = total
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

// FUNCTIONS - BASIC

#[test]
fn test_simple_function() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
def add(a, b):
    return a + b

OUTPUT = add(10, 20)
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_function_with_multiple_params() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
def compute(a, b, c, d):
    return a + b * c - d

OUTPUT = compute(10, 5, 3, 2)
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_function_with_conditionals() -> Result<()> {
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
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_function_with_loop() -> Result<()> {
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
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

// FUNCTIONS - RECURSION

#[test]
fn test_simple_recursion() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
def factorial(n):
    if n <= 1:
        return 1
    return n * factorial(n - 1)

OUTPUT = factorial(5)
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_fibonacci_recursion() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
def fib(n):
    if n <= 1:
        return n
    return fib(n - 1) + fib(n - 2)

OUTPUT = fib(10)
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_recursive_sum() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
def sum_recursive(n):
    if n == 0:
        return 0
    return n + sum_recursive(n - 1)

OUTPUT = sum_recursive(100)
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

// FUNCTIONS - MULTIPLE FUNCTIONS

#[test]
fn test_multiple_function_definitions() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
def add(a, b):
    return a + b

def multiply(a, b):
    return a * b

def compute(x):
    return add(x, 10) + multiply(x, 2)

OUTPUT = compute(5)
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_function_calling_another_function() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
def square(n):
    return n * n

def sum_of_squares(a, b):
    return square(a) + square(b)

OUTPUT = sum_of_squares(3, 4)
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

// DATA STRUCTURES - LISTS

#[test]
fn test_list_creation() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
nums = [1, 2, 3, 4, 5]
OUTPUT = 42
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_list_with_expressions() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 10
nums = [x, x + 1, x + 2, x * 2]
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_empty_list() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
empty = []
OUTPUT = 0
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

// DATA STRUCTURES - DICTS

#[test]
fn test_dict_with_int_keys() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
data = {1: 10, 2: 20, 3: 30}
OUTPUT = 42
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_dict_with_expressions() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 5
data = {x: x * 2, x + 1: x * 3}
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_empty_dict() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
empty = {}
OUTPUT = 0
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

// TUPLE UNPACKING

#[test]
fn test_tuple_unpacking() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
a, b = 10, 20
OUTPUT = a + b
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_multiple_tuple_unpacking() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x, y, z = 1, 2, 3
result = x + y + z
OUTPUT = result
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

// MIXED COMPLEX SCENARIOS

#[test]
fn test_complex_computation() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
def power(base, exp):
    if exp == 0:
        return 1
    result = base
    for i in range(exp - 1):
        result = result * base
    return result

def sum_of_powers(n):
    total = 0
    for i in range(n):
        total = total + power(2, i)
    return total

OUTPUT = sum_of_powers(10)
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_collatz_conjecture() -> Result<()> {
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
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_greatest_common_divisor() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
def gcd(a, b):
    while b != 0:
        temp = b
        b = a % b
        a = temp
    return a

OUTPUT = gcd(48, 18)
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_is_prime() -> Result<()> {
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
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

// COMPILER PROPERTIES

#[test]
fn test_deterministic_compilation() -> Result<()> {
    let mut compiler1 = PythonCompiler::new();
    let mut compiler2 = PythonCompiler::new();

    let code = r#"
def compute(n):
    return n * n

OUTPUT = compute(42)
"#;

    let wasm1 = compiler1.compile(code)?;
    let wasm2 = compiler2.compile(code)?;

    // Same code should produce identical wasm
    assert_eq!(wasm1, wasm2);
    Ok(())
}

#[test]
fn test_wasm_size_reasonable() -> Result<()> {
    let mut compiler = PythonCompiler::new();

    // Large but valid program
    let mut code = String::from("x = 0\n");
    for i in 0..100 {
        code.push_str(&format!("x = x + {}\n", i));
    }
    code.push_str("OUTPUT = x\n");

    let wasm = compiler.compile(&code)?;

    // Should be under 24KB limit
    assert!(wasm.len() < 24 * 1024);
    Ok(())
}

#[test]
fn test_reject_excessive_locals() -> Result<()> {
    let mut compiler = PythonCompiler::new();

    // Create code with too many local variables
    let mut code = String::new();
    for i in 0..300 {
        code.push_str(&format!("var{} = {}\n", i, i));
    }
    code.push_str("OUTPUT = var0\n");

    let result = compiler.compile(&code);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn test_cache_works() -> Result<()> {
    let mut compiler = PythonCompiler::new();

    let code = "x = 42\nOUTPUT = x\n";

    // Compile twice
    let wasm1 = compiler.compile(code)?;
    let wasm2 = compiler.compile(code)?;

    // Should be identical (cache hit)
    assert_eq!(wasm1, wasm2);
    Ok(())
}

// EDGE CASES

#[test]
fn test_zero_values() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = 0
y = 0 + 0
z = 0 * 100
OUTPUT = x + y + z
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_large_numbers() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
big = 1000000
OUTPUT = big + big
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_deeply_nested_expressions() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = ((((1 + 2) * 3) - 4) / 5) + 6
OUTPUT = x
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

#[test]
fn test_none_value() -> Result<()> {
    let mut compiler = PythonCompiler::new();
    let code = r#"
x = None
OUTPUT = 42
"#;
    let wasm = compiler.compile(code)?;
    assert!(wasm.starts_with(b"\0asm"));
    Ok(())
}

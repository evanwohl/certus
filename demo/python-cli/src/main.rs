use anyhow::{Result, anyhow};
use std::io::{self, Read};
use serde_json::json;
use base64::Engine;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: python-cli <compile|execute>");
        std::process::exit(1);
    }

    let command = &args[1];

    match command.as_str() {
        "compile" => handle_compile(),
        "execute" => handle_execute(),
        _ => {
            eprintln!("Unknown command: {}", command);
            eprintln!("Available commands: compile, execute");
            std::process::exit(1);
        }
    }
}

/// Read Python code from stdin, compile to Wasm, output JSON with base64
fn handle_compile() -> Result<()> {
    let mut python_code = String::new();
    io::stdin().read_to_string(&mut python_code)?;

    if python_code.trim().is_empty() {
        return Err(anyhow!("No Python code provided"));
    }

    // Just compile, don't execute
    use python_verifier::python_compiler::PythonCompiler;
    let mut compiler = PythonCompiler::new();

    match compiler.compile(&python_code) {
        Ok(wasm_bytes) => {
            let wasm_b64 = base64::engine::general_purpose::STANDARD.encode(&wasm_bytes);
            let result = json!({
                "wasm": wasm_b64,
                "size": wasm_bytes.len()
            });
            println!("{}", serde_json::to_string(&result)?);
            Ok(())
        }
        Err(e) => {
            let result = json!({"error": e.to_string()});
            println!("{}", serde_json::to_string(&result)?);
            Ok(())
        }
    }
}

/// Execute Wasm using the same pattern as tests
fn handle_execute() -> Result<()> {
    let mut python_code = String::new();
    io::stdin().read_to_string(&mut python_code)?;

    if python_code.trim().is_empty() {
        return Err(anyhow!("No Python code provided"));
    }

    use python_verifier::python_compiler::PythonCompiler;
    use wasmtime::*;

    let mut compiler = PythonCompiler::new();
    let wasm_bytes = compiler.compile(&python_code)?;

    // Execute using test pattern
    let engine = Engine::default();
    let mut store = Store::new(&engine, ());

    let memory_type = MemoryType::new(16, Some(256));
    let memory = Memory::new(&mut store, memory_type)?;

    let module = Module::new(&engine, &wasm_bytes)?;
    let imports = [memory.into()];
    let instance = Instance::new(&mut store, &module, &imports)?;

    let main = instance.get_typed_func::<(), i32>(&mut store, "main")?;
    let output_val = main.call(&mut store, ())?;

    // Hash the output
    use sha2::{Sha256, Digest};
    let output_str = output_val.to_string();
    let mut hasher = Sha256::new();
    hasher.update(output_str.as_bytes());
    let output_hash = hex::encode(hasher.finalize());

    let result = json!({
        "output": output_str,
        "output_hash": output_hash,
        "stdout": []
    });
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}

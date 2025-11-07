use wasmtime::*;
use anyhow::{Result, bail, Context};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};

pub mod compiler;
pub mod python_compiler;
pub mod verifier;
pub mod api;
pub mod websocket;
pub mod queue;
pub mod certus_integration;
pub mod reliability;
pub mod validation;

use python_compiler::PythonCompiler;
use validation::{PythonValidator, validate_json_input, validate_output};

pub struct PythonExecutor {
    engine: Engine,
    compiler: PythonCompiler,
}

impl PythonExecutor {
    pub fn new() -> Result<Self> {
        let mut config = Config::new();

        // Deterministic configuration
        config.wasm_threads(false);
        config.wasm_reference_types(false);
        config.cranelift_nan_canonicalization(true);
        config.consume_fuel(true);
        config.epoch_interruption(true);

        // Memory bounds
        config.static_memory_maximum_size(64 * 1024 * 1024);
        config.max_wasm_stack(1024 * 1024);

        let engine = Engine::new(&config)?;
        let compiler = PythonCompiler::new();

        Ok(Self { engine, compiler })
    }

    pub fn execute(
        &mut self,
        python_code: &str,
        input_json: &str,
        fuel_limit: u64,
    ) -> Result<ExecutionOutput> {
        // Validate
        PythonValidator::validate_code(python_code)?;
        validate_json_input(input_json)?;
        self.validate_python(python_code)?;

        // compile
        let wasm_module = self.compiler.compile(python_code)?;
        self.validate_wasm(&wasm_module)?;

        // sandbox setup
        let mut store = Store::new(&self.engine, ());
        let fuel = fuel_limit.min(100_000_000).max(1_000);
        store.set_fuel(fuel)?;
        store.set_epoch_deadline(100);

        let module = Module::new(&self.engine, &wasm_module)?;
        let instance = self.instantiate(&mut store, &module)?;

        // Execute with panic guard
        let output = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.run_module(&mut store, &instance, input_json)
        })) {
            Ok(Ok(result)) => result,
            Ok(Err(e)) => bail!("execution failed: {}", e),
            Err(_) => bail!("panic during execution"),
        };

        validate_output(&output)?;

        // Output hash
        let mut hasher = Sha256::new();
        hasher.update(output.as_bytes());
        let hash = hex::encode(hasher.finalize());

        Ok(ExecutionOutput {
            result: output,
            output_hash: hash,
            fuel_consumed: fuel - store.get_fuel().unwrap_or(0),
            success: true,
        })
    }

    pub fn validate_python(&self, code: &str) -> Result<()> {
        // only json/hashlib imports
        if code.contains("import ") || code.contains("from ") {
            let allowed = code.contains("import json") ||
                         code.contains("import hashlib") ||
                         code.contains("from hashlib");
            if !allowed {
                bail!("only json and hashlib imports allowed");
            }
        }

        // No file I/O
        if code.contains("open(") || code.contains("file(") {
            bail!("file operations not allowed");
        }

        // No network
        if code.contains("urllib") || code.contains("requests") || code.contains("socket") {
            bail!("network operations not allowed");
        }

        // no time/random
        if code.contains("time.") || code.contains("random.") || code.contains("datetime") {
            bail!("time/random not allowed");
        }

        // no eval/exec
        if code.contains("eval(") || code.contains("exec(") || code.contains("compile(") {
            bail!("dynamic code execution not allowed");
        }

        // no subprocess
        if code.contains("subprocess") || code.contains("os.system") {
            bail!("subprocess not allowed");
        }

        Ok(())
    }

    fn validate_wasm(&self, wasm: &[u8]) -> Result<()> {
        // 24KB on-chain limit
        const MAX_SIZE: usize = 24 * 1024;
        if wasm.len() > MAX_SIZE {
            bail!("wasm exceeds 24KB: {} bytes", wasm.len());
        }

        // check magic bytes
        if wasm.len() < 8 || &wasm[0..4] != b"\0asm" {
            bail!("invalid wasm magic");
        }

        // scan for float opcodes
        for (i, &byte) in wasm.iter().enumerate().skip(8) {
            match byte {
                0x43..=0x98 | 0x99..=0xBF => {
                    bail!("float opcode 0x{:02x} at offset {}", byte, i)
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn instantiate(&self, store: &mut Store<()>, module: &Module) -> Result<Instance> {
        let mut linker = Linker::new(&self.engine);

        // minimal env
        let memory_ty = MemoryType::new(1, Some(256)); // 16MB max
        let memory = Memory::new(&mut *store, memory_ty)?;
        linker.define(&mut *store, "env", "memory", memory)?;

        // Abort handler
        linker.func_wrap("env", "abort", |_msg: i32| -> Result<()> {
            bail!("abort called")
        })?;

        linker.instantiate(&mut *store, module)
            .context("failed to instantiate module")
    }

    fn run_module(
        &self,
        store: &mut Store<()>,
        instance: &Instance,
        input: &str,
    ) -> Result<String> {
        let run = instance
            .get_typed_func::<(i32, i32), i32>(&mut *store, "python_main")
            .context("missing python_main export")?;

        let memory = instance
            .get_memory(&mut *store, "memory")
            .context("missing memory export")?;

        let input_bytes = input.as_bytes();
        let input_ptr = 0x1000;
        memory.write(&mut *store, input_ptr, input_bytes)?;

        let output_ptr = run.call(&mut *store, (input_ptr as i32, input_bytes.len() as i32))?;

        let mut output = vec![0u8; 4096];
        memory.read(&mut *store, output_ptr as usize, &mut output)?;

        // null terminator
        let len = output.iter().position(|&b| b == 0).unwrap_or(output.len());

        String::from_utf8(output[..len].to_vec())
            .context("invalid utf-8 in output")
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionOutput {
    pub result: String,
    pub output_hash: String,
    pub fuel_consumed: u64,
    pub success: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PythonJob {
    pub code: String,
    pub input: serde_json::Value,
    pub expected_output: Option<String>,
}
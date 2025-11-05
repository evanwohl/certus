use wasmtime::*;
use anyhow::{Result, bail};
use certus_common::ExecutionResult;

/// Deterministic Wasm sandbox
pub struct WasmSandbox {
    engine: Engine,
}

impl WasmSandbox {
    /// Create sandbox with deterministic settings
    pub fn new() -> Result<Self> {
        let mut config = Config::new();

        // Determinism settings
        config.wasm_threads(false);
        config.wasm_simd(false);
        config.wasm_reference_types(false);
        config.cranelift_nan_canonicalization(true);
        config.consume_fuel(true);
        config.epoch_interruption(true);

        // Memory limits
        config.static_memory_maximum_size(64 * 1024 * 1024); // 64MB max
        config.max_wasm_stack(1024 * 1024); // 1MB stack

        let engine = Engine::new(&config)?;
        Ok(Self { engine })
    }

    /// Validate Wasm module for determinism
    pub fn validate(&self, wasm: &[u8]) -> Result<()> {
        // Size constraint (24KB max for on-chain storage)
        const MAX_MODULE_SIZE: usize = 24 * 1024;
        if wasm.len() > MAX_MODULE_SIZE {
            bail!("Module exceeds 24KB limit: {} bytes", wasm.len());
        }

        // Check magic and version
        if wasm.len() < 8 {
            bail!("Module too small: {} bytes", wasm.len());
        }

        if &wasm[0..4] != b"\0asm" {
            bail!("Invalid Wasm magic");
        }

        if &wasm[4..8] != &[1, 0, 0, 0] {
            bail!("Unsupported Wasm version");
        }

        // Scan for float opcodes (comprehensive check)
        for (i, &byte) in wasm.iter().enumerate().skip(8) {
            match byte {
                0x43..=0x98 => bail!("f32 opcode 0x{:02x} at offset {}", byte, i),
                0x99..=0xBF => bail!("f64 opcode 0x{:02x} at offset {}", byte, i),
                _ => {}
            }
        }

        // Verify module compiles with deterministic config
        Module::new(&self.engine, wasm)?;

        Ok(())
    }

    /// Execute Wasm with resource limits
    pub fn execute(
        &self,
        wasm: &[u8],
        input: &[u8],
        fuel_limit: u64,
        mem_limit: u64,
    ) -> Result<ExecutionResult> {
        let module = Module::new(&self.engine, wasm)?;
        let mut store = Store::new(&self.engine, ());

        // Set fuel with bounds check
        if fuel_limit == 0 || fuel_limit > u64::MAX / 2 {
            bail!("Invalid fuel limit: {}", fuel_limit);
        }
        store.set_fuel(fuel_limit)?;

        // Create instance with minimal imports
        let mut linker = Linker::new(&self.engine);

        // Memory limits (pages of 64KB)
        const PAGE_SIZE: u64 = 65536;
        if mem_limit < PAGE_SIZE || mem_limit > 10 * 1024 * 1024 {
            bail!("Invalid memory limit: {} bytes", mem_limit);
        }
        let max_pages = (mem_limit / PAGE_SIZE) as u32;
        let memory_ty = MemoryType::new(1, Some(max_pages));
        let memory = Memory::new(&mut store, memory_ty)?;
        linker.define(&mut store, "env", "memory", memory)?;

        let instance = linker.instantiate(&mut store, &module)?;

        // Get main function
        let main = instance
            .get_typed_func::<(i32, i32), i32>(&mut store, "main")?;

        // Write input to memory
        memory.write(&mut store, 0, input)?;

        // Execute
        let output_ptr = main.call(&mut store, (0, input.len() as i32))?;

        // Read output (assume 32 bytes for now)
        let mut output = vec![0u8; 32];
        memory.read(&store, output_ptr as usize, &mut output)?;

        let fuel_consumed = fuel_limit - store.get_fuel()?;

        Ok(ExecutionResult {
            output,
            fuel_consumed,
            success: true,
        })
    }
}
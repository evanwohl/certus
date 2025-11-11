// Certus Stylus Wasm Executor
// Deploys to Arbitrum as a Stylus contract (Rust compiled to Wasm)

#![cfg_attr(not(feature = "export-abi"), no_main)]
extern crate alloc;

mod wasm_interpreter;

use stylus_sdk::{
    alloy_primitives::{U256, B256},
    prelude::*,
    call::RawCall,
};
use alloc::{vec, vec::Vec};
use wasm_interpreter::Interpreter;

/// Execution error codes
#[derive(Debug)]
pub enum ExecutionError {
    ModuleTooLarge,
    InvalidWasmMagic,
    InvalidWasmVersion,
    FloatOpcodeDetected,
    WasiImportDetected,
    ThreadOpcodeDetected,
    CompilationFailed,
    InstantiationFailed,
    ExecutionFailed,
    InvalidFuelLimit,
    InvalidMemoryLimit,
    OutOfFuel,
    OutOfMemory,
}

impl From<ExecutionError> for Vec<u8> {
    fn from(err: ExecutionError) -> Vec<u8> {
        let code = match err {
            ExecutionError::ModuleTooLarge => 1u8,
            ExecutionError::InvalidWasmMagic => 2,
            ExecutionError::InvalidWasmVersion => 3,
            ExecutionError::FloatOpcodeDetected => 4,
            ExecutionError::WasiImportDetected => 5,
            ExecutionError::ThreadOpcodeDetected => 6,
            ExecutionError::CompilationFailed => 7,
            ExecutionError::InstantiationFailed => 8,
            ExecutionError::ExecutionFailed => 9,
            ExecutionError::InvalidFuelLimit => 10,
            ExecutionError::InvalidMemoryLimit => 11,
            ExecutionError::OutOfFuel => 12,
            ExecutionError::OutOfMemory => 13,
        };
        vec![0xFF, code]
    }
}

sol_storage! {
    #[entrypoint]
    pub struct CertusStylusExecutor {
        address owner;
        uint256 execution_count;
        mapping(bytes32 => bytes32) execution_results;
    }
}

#[external]
impl CertusStylusExecutor {
    /// Execute WebAssembly module with determinism guarantees.
    /// Called by CertusEscrow.fraudOnChain() during dispute resolution.
    /// Enforces identical constraints to off-chain nodes
    pub fn execute(
        &mut self,
        wasm: Vec<u8>,
        input: Vec<u8>,
        fuel_limit: U256,
        mem_limit: U256,
    ) -> Result<Vec<u8>, Vec<u8>> {
        let count = self.execution_count.get();
        self.execution_count.set(count + U256::from(1));

        const MAX_MODULE_SIZE: usize = 24 * 1024;
        if wasm.len() > MAX_MODULE_SIZE {
            return Err(ExecutionError::ModuleTooLarge.into());
        }

        let fuel_u64 = match fuel_limit.try_into() {
            Ok(f) if f > 0 && f <= u64::MAX / 2 => f,
            _ => return Err(ExecutionError::InvalidFuelLimit.into()),
        };

        const PAGE_SIZE: u64 = 65536;
        const MAX_MEMORY: u64 = 10 * 1024 * 1024;
        let mem_u64: u64 = match mem_limit.try_into() {
            Ok(m) if m >= PAGE_SIZE && m <= MAX_MEMORY => m,
            _ => return Err(ExecutionError::InvalidMemoryLimit.into()),
        };

        validate_determinism(&wasm)?;

        let output = execute_wasm(&wasm, &input, fuel_u64, mem_u64)?;

        let exec_id = compute_execution_id(&wasm, &input);
        let output_hash = compute_sha256(&output);
        self.execution_results.setter(exec_id).set(output_hash);

        Ok(output)
    }

    pub fn get_execution_count(&self) -> U256 {
        self.execution_count.get()
    }

    pub fn get_execution_result(&self, execution_id: B256) -> B256 {
        self.execution_results.get(execution_id)
    }
}

/// Validate Wasm module determinism constraints.
/// Rejects modules with float operations, WASI imports, or thread operations.
/// Must match node/executor/src/sandbox.rs validation logic
fn validate_determinism(wasm: &[u8]) -> Result<(), Vec<u8>> {
    if wasm.len() < 8 {
        return Err(ExecutionError::InvalidWasmMagic.into());
    }

    if &wasm[0..4] != b"\0asm" {
        return Err(ExecutionError::InvalidWasmMagic.into());
    }

    if &wasm[4..8] != &[1, 0, 0, 0] {
        return Err(ExecutionError::InvalidWasmVersion.into());
    }

    for &byte in &wasm[8..] {
        if (0x43..=0x98).contains(&byte) || (0x99..=0xBF).contains(&byte) {
            return Err(ExecutionError::FloatOpcodeDetected.into());
        }
    }

    if contains_pattern(wasm, b"wasi_snapshot") {
        return Err(ExecutionError::WasiImportDetected.into());
    }

    for &byte in &wasm[8..] {
        if byte == 0xFE {
            return Err(ExecutionError::ThreadOpcodeDetected.into());
        }
    }

    Ok(())
}

/// Execute Wasm instruction and return state hash.
/// Input encodes: [opcode, initial_state_data].
/// Returns SHA256(stack + locals + memory + pc + fuel) after execution.
fn execute_wasm(
    wasm: &[u8],
    input: &[u8],
    fuel_limit: u64,
    mem_limit: u64,
) -> Result<Vec<u8>, Vec<u8>> {
    if wasm.is_empty() {
        return Err(ExecutionError::ExecutionFailed.into());
    }

    if fuel_limit == 0 {
        return Err(ExecutionError::OutOfFuel.into());
    }

    let memory_size = (mem_limit as usize).min(10 * 1024 * 1024);
    let mut interpreter = Interpreter::new(memory_size, fuel_limit);

    if input.is_empty() {
        return Err(ExecutionError::ExecutionFailed.into());
    }

    let opcode = input[0];

    interpreter.execute_opcode(opcode, wasm)
        .map_err(|_| ExecutionError::ExecutionFailed)?;

    let state_hash = interpreter.compute_state_hash();
    Ok(state_hash.to_vec())
}

fn contains_pattern(data: &[u8], pattern: &[u8]) -> bool {
    if pattern.len() > data.len() {
        return false;
    }

    for i in 0..=(data.len() - pattern.len()) {
        if &data[i..i + pattern.len()] == pattern {
            return true;
        }
    }
    false
}

fn compute_execution_id(wasm: &[u8], input: &[u8]) -> B256 {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(wasm);
    hasher.update(input);
    let result = hasher.finalize();
    B256::from_slice(&result)
}

fn compute_sha256(data: &[u8]) -> B256 {
    use sha2::{Sha256, Digest};
    let result = Sha256::digest(data);
    B256::from_slice(&result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_determinism_valid() {
        let wasm = [
            0x00, 0x61, 0x73, 0x6D, // magic
            0x01, 0x00, 0x00, 0x00, // version
            0x00, // empty module
        ];
        assert!(validate_determinism(&wasm).is_ok());
    }

    #[test]
    fn test_validate_determinism_invalid_magic() {
        let wasm = [
            0xFF, 0x61, 0x73, 0x6D,
            0x01, 0x00, 0x00, 0x00,
        ];
        assert!(validate_determinism(&wasm).is_err());
    }

    #[test]
    fn test_validate_determinism_float_opcode() {
        let wasm = [
            0x00, 0x61, 0x73, 0x6D,
            0x01, 0x00, 0x00, 0x00,
            0x43, // f32.const opcode
        ];
        assert!(validate_determinism(&wasm).is_err());
    }

    #[test]
    fn test_validate_determinism_wasi_import() {
        let mut wasm = vec![
            0x00, 0x61, 0x73, 0x6D,
            0x01, 0x00, 0x00, 0x00,
        ];
        wasm.extend_from_slice(b"wasi_snapshot");
        assert!(validate_determinism(&wasm).is_err());
    }

    #[test]
    fn test_validate_determinism_thread_opcode() {
        let wasm = [
            0x00, 0x61, 0x73, 0x6D,
            0x01, 0x00, 0x00, 0x00,
            0xFE, // atomic operations prefix
        ];
        assert!(validate_determinism(&wasm).is_err());
    }

    #[test]
    fn test_contains_pattern() {
        assert!(contains_pattern(b"hello world", b"world"));
        assert!(!contains_pattern(b"hello", b"goodbye"));
        assert!(!contains_pattern(b"hi", b"hello"));
    }

    #[test]
    fn test_execution_id_deterministic() {
        let wasm1 = b"wasm_code";
        let input1 = b"input_data";
        let id1 = compute_execution_id(wasm1, input1);
        let id2 = compute_execution_id(wasm1, input1);
        assert_eq!(id1, id2);
    }
}

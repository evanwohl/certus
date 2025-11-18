// Standalone integration tests for the Wasm interpreter

#[cfg(test)]
mod tests {
    use stylus_executor::wasm_interpreter::{Interpreter, BytecodeValidator, StateHasher, Value};

    #[test]
    fn test_modular_architecture_compiles() {
        assert!(true, "Modular architecture compiles successfully");
    }

    #[test]
    fn test_validator_rejects_invalid_magic() {
        let invalid_wasm = vec![0xFF, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        assert!(BytecodeValidator::validate(&invalid_wasm).is_err());
    }

    #[test]
    fn test_validator_accepts_valid_wasm() {
        let valid_wasm = vec![
            0x00, 0x61, 0x73, 0x6D,
            0x01, 0x00, 0x00, 0x00,
        ];
        assert!(BytecodeValidator::validate(&valid_wasm).is_ok());
    }

    #[test]
    fn test_validator_builds_jump_table() {
        let mut wasm = vec![
            0x00, 0x61, 0x73, 0x6D,
            0x01, 0x00, 0x00, 0x00,
        ];
        wasm.push(0x02);
        wasm.push(0x40);
        wasm.push(0x01);
        wasm.push(0x0B);

        let table = BytecodeValidator::validate(&wasm).unwrap();
        assert_eq!(table.get(8), Some(11));
    }

    #[test]
    fn test_state_hash_includes_full_memory() {
        let mut large_memory = vec![0u8; 200_000];
        large_memory[150_000] = 0x42;

        let hash1 = StateHasher::hash(&[], &[], &large_memory, 0, 1000, &[], &[]);

        large_memory[150_000] = 0x43;
        let hash2 = StateHasher::hash(&[], &[], &large_memory, 0, 1000, &[], &[]);

        assert_ne!(hash1, hash2, "State hash must capture changes beyond 1KB");
    }

    #[test]
    fn test_state_hash_includes_control_flow() {
        use stylus_executor::wasm_interpreter::BlockFrame;

        let block = BlockFrame {
            start_pc: 100,
            end_pc: 200,
            stack_height: 5,
            is_loop: false,
            result_type: None,
        };

        let hash1 = StateHasher::hash(&[], &[], &[], 0, 1000, &[], &[]);
        let hash2 = StateHasher::hash(&[], &[], &[], 0, 1000, &[block], &[]);

        assert_ne!(hash1, hash2, "State hash must include block_stack");
    }

    #[test]
    fn test_state_hash_deterministic() {
        let stack = vec![Value::I32(42), Value::I64(100)];
        let locals = vec![Value::I32(1), Value::I32(2)];
        let memory = vec![0u8; 65536];

        let hash1 = StateHasher::hash(&stack, &locals, &memory, 100, 500, &[], &[]);
        let hash2 = StateHasher::hash(&stack, &locals, &memory, 100, 500, &[], &[]);

        assert_eq!(hash1, hash2, "State hash must be deterministic");
    }
}

// Deterministic state hashing with full VM state capture
// Hash must be identical across all platforms for fraud proofs

use alloc::vec::Vec;
use sha2::{Sha256, Digest};
use super::types::{Value, BlockFrame, CallFrame};

const MEMORY_PAGE_SIZE: usize = 65536;

pub struct StateHasher;

impl StateHasher {
    /// Compute deterministic hash of complete VM state
    pub fn hash(
        stack: &[Value],
        locals: &[Value],
        memory: &[u8],
        pc: usize,
        fuel: u64,
        block_stack: &[BlockFrame],
        call_stack: &[CallFrame],
    ) -> [u8; 32] {
        let mut hasher = Sha256::new();

        Self::hash_stack(&mut hasher, stack);
        Self::hash_locals(&mut hasher, locals);
        Self::hash_memory_merkle(&mut hasher, memory);
        Self::hash_pc_fuel(&mut hasher, pc, fuel);
        Self::hash_control_flow(&mut hasher, block_stack, call_stack);

        let result = hasher.finalize();
        let mut output = [0u8; 32];
        output.copy_from_slice(&result);
        output
    }

    fn hash_stack(hasher: &mut Sha256, stack: &[Value]) {
        hasher.update(&[0x01]);
        hasher.update(&(stack.len() as u32).to_le_bytes());

        for val in stack {
            match val {
                Value::I32(v) => {
                    hasher.update(&[0x7F]);
                    hasher.update(&v.to_le_bytes());
                }
                Value::I64(v) => {
                    hasher.update(&[0x7E]);
                    hasher.update(&v.to_le_bytes());
                }
            }
        }
    }

    fn hash_locals(hasher: &mut Sha256, locals: &[Value]) {
        hasher.update(&[0x02]);
        hasher.update(&(locals.len() as u32).to_le_bytes());

        for val in locals {
            match val {
                Value::I32(v) => {
                    hasher.update(&[0x7F]);
                    hasher.update(&v.to_le_bytes());
                }
                Value::I64(v) => {
                    hasher.update(&[0x7E]);
                    hasher.update(&v.to_le_bytes());
                }
            }
        }
    }

    /// Hash memory using page-level Merkle tree for gas efficiency
    /// For memory <= 64KB: hash entire contents
    /// For memory > 64KB: hash each page (64KB chunk), then Merkle combine
    fn hash_memory_merkle(hasher: &mut Sha256, memory: &[u8]) {
        hasher.update(&[0x03]);
        hasher.update(&(memory.len() as u32).to_le_bytes());

        if memory.len() <= MEMORY_PAGE_SIZE {
            hasher.update(memory);
        } else {
            let page_count = (memory.len() + MEMORY_PAGE_SIZE - 1) / MEMORY_PAGE_SIZE;
            let mut page_hashes = Vec::with_capacity(page_count);

            for page_idx in 0..page_count {
                let start = page_idx * MEMORY_PAGE_SIZE;
                let end = (start + MEMORY_PAGE_SIZE).min(memory.len());
                let page_hash = Sha256::digest(&memory[start..end]);
                page_hashes.push(page_hash);
            }

            let merkle_root = Self::merkle_root(&page_hashes);
            hasher.update(&merkle_root);
        }
    }

    fn merkle_root(hashes: &[[u8; 32]]) -> [u8; 32] {
        if hashes.is_empty() {
            return [0u8; 32];
        }
        if hashes.len() == 1 {
            return hashes[0];
        }

        let mut level: Vec<[u8; 32]> = hashes.to_vec();

        while level.len() > 1 {
            let mut next_level = Vec::with_capacity((level.len() + 1) / 2);

            for i in (0..level.len()).step_by(2) {
                let left = &level[i];
                let right = if i + 1 < level.len() {
                    &level[i + 1]
                } else {
                    left
                };

                let mut hasher = Sha256::new();
                hasher.update(left);
                hasher.update(right);
                let combined = hasher.finalize();

                let mut hash = [0u8; 32];
                hash.copy_from_slice(&combined);
                next_level.push(hash);
            }

            level = next_level;
        }

        level[0]
    }

    fn hash_pc_fuel(hasher: &mut Sha256, pc: usize, fuel: u64) {
        hasher.update(&[0x04]);
        hasher.update(&pc.to_le_bytes());
        hasher.update(&fuel.to_le_bytes());
    }

    fn hash_control_flow(
        hasher: &mut Sha256,
        block_stack: &[BlockFrame],
        call_stack: &[CallFrame],
    ) {
        hasher.update(&[0x05]);
        hasher.update(&(block_stack.len() as u32).to_le_bytes());

        for block in block_stack {
            hasher.update(&block.start_pc.to_le_bytes());
            hasher.update(&block.end_pc.to_le_bytes());
            hasher.update(&block.stack_height.to_le_bytes());
            hasher.update(&[if block.is_loop { 1 } else { 0 }]);
        }

        hasher.update(&[0x06]);
        hasher.update(&(call_stack.len() as u32).to_le_bytes());

        for frame in call_stack {
            hasher.update(&frame.return_pc.to_le_bytes());
            hasher.update(&frame.locals_start.to_le_bytes());
            hasher.update(&frame.locals_count.to_le_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_hash() {
        let stack = vec![Value::I32(42), Value::I64(100)];
        let locals = vec![Value::I32(1), Value::I32(2)];
        let memory = vec![0u8; 1024];

        let hash1 = StateHasher::hash(&stack, &locals, &memory, 100, 500, &[], &[]);
        let hash2 = StateHasher::hash(&stack, &locals, &memory, 100, 500, &[], &[]);

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_different_memory_different_hash() {
        let stack = vec![];
        let locals = vec![];
        let memory1 = vec![0u8; 2048];
        let mut memory2 = vec![0u8; 2048];
        memory2[2000] = 1;

        let hash1 = StateHasher::hash(&stack, &locals, &memory1, 0, 1000, &[], &[]);
        let hash2 = StateHasher::hash(&stack, &locals, &memory2, 0, 1000, &[], &[]);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_control_flow_affects_hash() {
        let stack = vec![];
        let locals = vec![];
        let memory = vec![];

        let block = BlockFrame {
            start_pc: 0,
            end_pc: 10,
            stack_height: 0,
            is_loop: false,
            result_type: None,
        };

        let hash1 = StateHasher::hash(&stack, &locals, &memory, 0, 1000, &[], &[]);
        let hash2 = StateHasher::hash(&stack, &locals, &memory, 0, 1000, &[block], &[]);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_merkle_root_single_hash() {
        let hash = [0x42u8; 32];
        let root = StateHasher::merkle_root(&[hash]);
        assert_eq!(root, hash);
    }

    #[test]
    fn test_merkle_root_deterministic() {
        let hashes = vec![[1u8; 32], [2u8; 32], [3u8; 32], [4u8; 32]];
        let root1 = StateHasher::merkle_root(&hashes);
        let root2 = StateHasher::merkle_root(&hashes);
        assert_eq!(root1, root2);
    }
}

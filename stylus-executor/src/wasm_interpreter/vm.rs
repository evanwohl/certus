// Virtual machine core: stack, memory, execution state

use alloc::vec::Vec;
use super::types::{Value, ValueType, CallFrame, BlockFrame};
use super::leb128::Leb128Reader;
use super::opcodes;
use super::state::StateHasher;
use super::validator::JumpTable;
use super::{MAX_STACK_DEPTH, MAX_CALL_DEPTH, MAX_BLOCK_DEPTH, MAX_MEMORY_PAGES, PAGE_SIZE};

pub struct Interpreter {
    stack: Vec<Value>,
    locals: Vec<Value>,
    memory: Vec<u8>,
    pc: usize,
    call_stack: Vec<CallFrame>,
    block_stack: Vec<BlockFrame>,
    fuel: u64,
    jump_table: JumpTable,
}

impl Interpreter {
    pub fn new(memory_pages: usize, fuel: u64, jump_table: JumpTable) -> Self {
        let memory_size = memory_pages * PAGE_SIZE;
        Self {
            stack: Vec::with_capacity(MAX_STACK_DEPTH),
            locals: Vec::with_capacity(256),
            memory: alloc::vec![0u8; memory_size],
            pc: 0,
            call_stack: Vec::with_capacity(MAX_CALL_DEPTH),
            block_stack: Vec::with_capacity(MAX_BLOCK_DEPTH),
            fuel,
            jump_table,
        }
    }

    pub fn execute_single_opcode(&mut self, bytecode: &[u8]) -> Result<(), &'static str> {
        if self.pc >= bytecode.len() {
            return Err("pc out of bounds");
        }

        let opcode = bytecode[self.pc];
        self.pc += 1;

        self.consume_fuel(1)?;
        opcodes::dispatch(self, opcode, bytecode)
    }

    pub fn push(&mut self, val: Value) -> Result<(), &'static str> {
        if self.stack.len() >= MAX_STACK_DEPTH {
            return Err("stack overflow");
        }
        self.stack.push(val);
        Ok(())
    }

    pub fn pop(&mut self) -> Result<Value, &'static str> {
        self.stack.pop().ok_or("stack underflow")
    }

    pub fn pop_i32(&mut self) -> Result<i32, &'static str> {
        self.pop()?.as_i32()
    }

    pub fn pop_i64(&mut self) -> Result<i64, &'static str> {
        self.pop()?.as_i64()
    }

    pub fn peek(&self) -> Option<&Value> {
        self.stack.last()
    }

    pub fn consume_fuel(&mut self, amount: u64) -> Result<(), &'static str> {
        if self.fuel < amount {
            return Err("out of fuel");
        }
        self.fuel -= amount;
        Ok(())
    }

    pub fn load_memory(&self, addr: usize, size: usize) -> Result<&[u8], &'static str> {
        let end = addr.checked_add(size).ok_or("address overflow")?;
        if end > self.memory.len() {
            return Err("memory access out of bounds");
        }
        Ok(&self.memory[addr..end])
    }

    pub fn store_memory(&mut self, addr: usize, data: &[u8]) -> Result<(), &'static str> {
        let end = addr.checked_add(data.len()).ok_or("address overflow")?;
        if end > self.memory.len() {
            return Err("memory access out of bounds");
        }
        self.memory[addr..end].copy_from_slice(data);
        Ok(())
    }

    pub fn grow_memory(&mut self, pages: u32) -> Result<i32, &'static str> {
        let current_pages = self.memory.len() / PAGE_SIZE;
        let new_pages = current_pages + pages as usize;

        if new_pages > MAX_MEMORY_PAGES {
            return Ok(-1);
        }

        let new_size = new_pages * PAGE_SIZE;
        self.memory.resize(new_size, 0);
        Ok(current_pages as i32)
    }

    pub fn memory_size(&self) -> i32 {
        (self.memory.len() / PAGE_SIZE) as i32
    }

    pub fn get_local(&self, idx: usize) -> Result<Value, &'static str> {
        self.locals.get(idx).copied().ok_or("local index out of bounds")
    }

    pub fn set_local(&mut self, idx: usize, val: Value) -> Result<(), &'static str> {
        if idx >= self.locals.len() {
            return Err("local index out of bounds");
        }
        self.locals[idx] = val;
        Ok(())
    }

    pub fn push_block(&mut self, frame: BlockFrame) -> Result<(), &'static str> {
        if self.block_stack.len() >= MAX_BLOCK_DEPTH {
            return Err("block depth exceeded");
        }
        self.block_stack.push(frame);
        Ok(())
    }

    pub fn pop_block(&mut self) -> Result<BlockFrame, &'static str> {
        self.block_stack.pop().ok_or("block stack underflow")
    }

    pub fn peek_block(&self) -> Option<&BlockFrame> {
        self.block_stack.last()
    }

    pub fn branch(&mut self, depth: usize) -> Result<(), &'static str> {
        if depth >= self.block_stack.len() {
            return Err("branch depth out of bounds");
        }

        let target_idx = self.block_stack.len() - 1 - depth;
        let target = &self.block_stack[target_idx];

        // Unwind stack to block height
        self.stack.truncate(target.stack_height);

        // Jump to target
        self.pc = if target.is_loop {
            target.start_pc
        } else {
            target.end_pc
        };

        // Remove inner blocks
        self.block_stack.truncate(target_idx + 1);
        Ok(())
    }

    pub fn push_call_frame(&mut self, frame: CallFrame) -> Result<(), &'static str> {
        if self.call_stack.len() >= MAX_CALL_DEPTH {
            return Err("call depth exceeded");
        }
        self.call_stack.push(frame);
        Ok(())
    }

    pub fn pop_call_frame(&mut self) -> Result<CallFrame, &'static str> {
        self.call_stack.pop().ok_or("call stack underflow")
    }

    pub fn get_pc(&self) -> usize {
        self.pc
    }

    pub fn set_pc(&mut self, pc: usize) {
        self.pc = pc;
    }

    pub fn stack_height(&self) -> usize {
        self.stack.len()
    }

    pub fn read_leb128_u32(&mut self, bytecode: &[u8]) -> Result<u32, &'static str> {
        let mut reader = Leb128Reader::new(bytecode, self.pc);
        let (value, bytes_read) = reader.read_u32()?;
        self.pc += bytes_read;
        Ok(value)
    }

    pub fn read_leb128_i32(&mut self, bytecode: &[u8]) -> Result<i32, &'static str> {
        let mut reader = Leb128Reader::new(bytecode, self.pc);
        let (value, bytes_read) = reader.read_i32()?;
        self.pc += bytes_read;
        Ok(value)
    }

    pub fn read_leb128_i64(&mut self, bytecode: &[u8]) -> Result<i64, &'static str> {
        let mut reader = Leb128Reader::new(bytecode, self.pc);
        let (value, bytes_read) = reader.read_i64()?;
        self.pc += bytes_read;
        Ok(value)
    }

    pub fn compute_state_hash(&self) -> [u8; 32] {
        StateHasher::hash(
            &self.stack,
            &self.locals,
            &self.memory,
            self.pc,
            self.fuel,
            &self.block_stack,
            &self.call_stack,
        )
    }

    pub fn find_matching_end(&self, start_pc: usize) -> Result<usize, &'static str> {
        self.jump_table.get(start_pc).ok_or("block not in jump table")
    }

    pub fn find_else_or_end(&self, bytecode: &[u8]) -> Result<usize, &'static str> {
        let mut depth = 1;
        let mut pc = self.pc;

        while pc < bytecode.len() && depth > 0 {
            match bytecode[pc] {
                0x02 | 0x03 | 0x04 => depth += 1,
                0x05 if depth == 1 => return Ok(pc),
                0x0B => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(pc);
                    }
                }
                _ => {}
            }
            pc += 1;
        }

        Err("unmatched if")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stack_operations() {
        use super::super::validator::JumpTable;
        let mut vm = Interpreter::new(1, 1000, JumpTable::new());

        vm.push(Value::I32(42)).unwrap();
        assert_eq!(vm.stack_height(), 1);

        let val = vm.pop().unwrap();
        assert_eq!(val.as_i32().unwrap(), 42);
        assert_eq!(vm.stack_height(), 0);
    }

    #[test]
    fn test_memory_operations() {
        use super::super::validator::JumpTable;
        let mut vm = Interpreter::new(1, 1000, JumpTable::new());

        vm.store_memory(0, &[1, 2, 3, 4]).unwrap();
        let data = vm.load_memory(0, 4).unwrap();
        assert_eq!(data, &[1, 2, 3, 4]);
    }

    #[test]
    fn test_memory_grow() {
        use super::super::validator::JumpTable;
        let mut vm = Interpreter::new(1, 1000, JumpTable::new());

        assert_eq!(vm.memory_size(), 1);
        let old_size = vm.grow_memory(1).unwrap();
        assert_eq!(old_size, 1);
        assert_eq!(vm.memory_size(), 2);
    }

    #[test]
    fn test_state_hash_determinism() {
        use super::super::validator::JumpTable;
        let mut vm1 = Interpreter::new(1, 1000, JumpTable::new());
        vm1.push(Value::I32(42)).unwrap();
        let hash1 = vm1.compute_state_hash();

        let mut vm2 = Interpreter::new(1, 1000, JumpTable::new());
        vm2.push(Value::I32(42)).unwrap();
        let hash2 = vm2.compute_state_hash();

        assert_eq!(hash1, hash2);
    }
}

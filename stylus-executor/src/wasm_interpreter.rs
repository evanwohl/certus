// Wasm interpreter for on-chain fraud proof verification

use alloc::vec::Vec;
use sha2::{Sha256, Digest};

const MAX_STACK_DEPTH: usize = 1024;
const MAX_CALL_DEPTH: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    I32(i32),
    I64(i64),
}

impl Value {
    pub fn as_i32(self) -> Result<i32, &'static str> {
        match self {
            Value::I32(v) => Ok(v),
            _ => Err("type mismatch: expected i32"),
        }
    }

    pub fn as_i64(self) -> Result<i64, &'static str> {
        match self {
            Value::I64(v) => Ok(v),
            _ => Err("type mismatch: expected i64"),
        }
    }
}

pub struct Interpreter {
    pub stack: Vec<Value>,
    pub locals: Vec<Value>,
    pub memory: Vec<u8>,
    pub pc: usize,
    pub call_stack: Vec<CallFrame>,
    pub fuel: u64,
}

pub struct CallFrame {
    pub return_pc: usize,
    pub locals_start: usize,
}

impl Interpreter {
    pub fn new(memory_size: usize, fuel: u64) -> Self {
        Self {
            stack: Vec::with_capacity(MAX_STACK_DEPTH),
            locals: Vec::with_capacity(256),
            memory: alloc::vec![0u8; memory_size],
            pc: 0,
            call_stack: Vec::with_capacity(MAX_CALL_DEPTH),
            fuel,
        }
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

    pub fn consume_fuel(&mut self, amount: u64) -> Result<(), &'static str> {
        if self.fuel < amount {
            return Err("out of fuel");
        }
        self.fuel -= amount;
        Ok(())
    }

    pub fn load_memory(&self, addr: usize, size: usize) -> Result<&[u8], &'static str> {
        if addr + size > self.memory.len() {
            return Err("memory access out of bounds");
        }
        Ok(&self.memory[addr..addr + size])
    }

    pub fn store_memory(&mut self, addr: usize, data: &[u8]) -> Result<(), &'static str> {
        if addr + data.len() > self.memory.len() {
            return Err("memory access out of bounds");
        }
        self.memory[addr..addr + data.len()].copy_from_slice(data);
        Ok(())
    }

    pub fn execute_opcode(&mut self, opcode: u8, bytecode: &[u8]) -> Result<(), &'static str> {
        self.consume_fuel(1)?;

        match opcode {
            // Control flow
            0x00 => Err("unreachable executed"),
            0x01 => Ok(()),

            // Constants
            0x41 => {
                let val = self.read_leb128_i32(bytecode)?;
                self.push(Value::I32(val))
            }
            0x42 => {
                let val = self.read_leb128_i64(bytecode)?;
                self.push(Value::I64(val))
            }

            // Variable access
            0x20 => {
                let idx = self.read_leb128_u32(bytecode)? as usize;
                if idx >= self.locals.len() {
                    return Err("local index out of bounds");
                }
                self.push(self.locals[idx])
            }
            0x21 => {
                let idx = self.read_leb128_u32(bytecode)? as usize;
                if idx >= self.locals.len() {
                    return Err("local index out of bounds");
                }
                let val = self.pop()?;
                self.locals[idx] = val;
                Ok(())
            }
            0x22 => {
                let idx = self.read_leb128_u32(bytecode)? as usize;
                if idx >= self.locals.len() {
                    return Err("local index out of bounds");
                }
                let val = self.pop()?;
                self.locals[idx] = val;
                self.push(val)
            }

            // Memory operations
            0x28 => {
                let _align = self.read_leb128_u32(bytecode)?;
                let offset = self.read_leb128_u32(bytecode)? as usize;
                let addr = self.pop_i32()? as usize + offset;
                let bytes = self.load_memory(addr, 4)?;
                let val = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                self.push(Value::I32(val))
            }
            0x29 => {
                let _align = self.read_leb128_u32(bytecode)?;
                let offset = self.read_leb128_u32(bytecode)? as usize;
                let addr = self.pop_i32()? as usize + offset;
                let bytes = self.load_memory(addr, 8)?;
                let val = i64::from_le_bytes([
                    bytes[0], bytes[1], bytes[2], bytes[3],
                    bytes[4], bytes[5], bytes[6], bytes[7],
                ]);
                self.push(Value::I64(val))
            }
            0x36 => {
                let _align = self.read_leb128_u32(bytecode)?;
                let offset = self.read_leb128_u32(bytecode)? as usize;
                let val = self.pop_i32()?;
                let addr = self.pop_i32()? as usize + offset;
                self.store_memory(addr, &val.to_le_bytes())
            }
            0x37 => {
                let _align = self.read_leb128_u32(bytecode)?;
                let offset = self.read_leb128_u32(bytecode)? as usize;
                let val = self.pop_i64()?;
                let addr = self.pop_i32()? as usize + offset;
                self.store_memory(addr, &val.to_le_bytes())
            }

            // i32 comparison
            0x45 => {
                let val = self.pop_i32()?;
                self.push(Value::I32(if val == 0 { 1 } else { 0 }))
            }
            0x46 => {
                let b = self.pop_i32()?;
                let a = self.pop_i32()?;
                self.push(Value::I32(if a == b { 1 } else { 0 }))
            }
            0x47 => {
                let b = self.pop_i32()?;
                let a = self.pop_i32()?;
                self.push(Value::I32(if a != b { 1 } else { 0 }))
            }
            0x48 => {
                let b = self.pop_i32()?;
                let a = self.pop_i32()?;
                self.push(Value::I32(if a < b { 1 } else { 0 }))
            }
            0x49 => {
                let b = self.pop_i32()?;
                let a = self.pop_i32()?;
                self.push(Value::I32(if (a as u32) < (b as u32) { 1 } else { 0 }))
            }
            0x4A => {
                let b = self.pop_i32()?;
                let a = self.pop_i32()?;
                self.push(Value::I32(if a > b { 1 } else { 0 }))
            }
            0x4B => {
                let b = self.pop_i32()?;
                let a = self.pop_i32()?;
                self.push(Value::I32(if (a as u32) > (b as u32) { 1 } else { 0 }))
            }
            0x4C => {
                let b = self.pop_i32()?;
                let a = self.pop_i32()?;
                self.push(Value::I32(if a <= b { 1 } else { 0 }))
            }
            0x4D => {
                let b = self.pop_i32()?;
                let a = self.pop_i32()?;
                self.push(Value::I32(if (a as u32) <= (b as u32) { 1 } else { 0 }))
            }
            0x4E => {
                let b = self.pop_i32()?;
                let a = self.pop_i32()?;
                self.push(Value::I32(if a >= b { 1 } else { 0 }))
            }
            0x4F => {
                let b = self.pop_i32()?;
                let a = self.pop_i32()?;
                self.push(Value::I32(if (a as u32) >= (b as u32) { 1 } else { 0 }))
            }

            // i64 comparison
            0x50 => {
                let val = self.pop_i64()?;
                self.push(Value::I32(if val == 0 { 1 } else { 0 }))
            }
            0x51 => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                self.push(Value::I32(if a == b { 1 } else { 0 }))
            }
            0x52 => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                self.push(Value::I32(if a != b { 1 } else { 0 }))
            }
            0x53 => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                self.push(Value::I32(if a < b { 1 } else { 0 }))
            }
            0x54 => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                self.push(Value::I32(if (a as u64) < (b as u64) { 1 } else { 0 }))
            }
            0x55 => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                self.push(Value::I32(if a > b { 1 } else { 0 }))
            }
            0x56 => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                self.push(Value::I32(if (a as u64) > (b as u64) { 1 } else { 0 }))
            }
            0x57 => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                self.push(Value::I32(if a <= b { 1 } else { 0 }))
            }
            0x58 => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                self.push(Value::I32(if (a as u64) <= (b as u64) { 1 } else { 0 }))
            }
            0x59 => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                self.push(Value::I32(if a >= b { 1 } else { 0 }))
            }
            0x5A => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                self.push(Value::I32(if (a as u64) >= (b as u64) { 1 } else { 0 }))
            }

            // i32 arithmetic
            0x67 => {
                let val = self.pop_i32()?;
                self.push(Value::I32(val.leading_zeros() as i32))
            }
            0x68 => {
                let val = self.pop_i32()?;
                self.push(Value::I32(val.trailing_zeros() as i32))
            }
            0x69 => {
                let val = self.pop_i32()?;
                self.push(Value::I32(val.count_ones() as i32))
            }
            0x6A => {
                let b = self.pop_i32()?;
                let a = self.pop_i32()?;
                self.push(Value::I32(a.wrapping_add(b)))
            }
            0x6B => {
                let b = self.pop_i32()?;
                let a = self.pop_i32()?;
                self.push(Value::I32(a.wrapping_sub(b)))
            }
            0x6C => {
                let b = self.pop_i32()?;
                let a = self.pop_i32()?;
                self.push(Value::I32(a.wrapping_mul(b)))
            }
            0x6D => {
                let b = self.pop_i32()?;
                let a = self.pop_i32()?;
                if b == 0 {
                    return Err("integer divide by zero");
                }
                self.push(Value::I32(a.wrapping_div(b)))
            }
            0x6E => {
                let b = self.pop_i32()?;
                let a = self.pop_i32()?;
                if b == 0 {
                    return Err("integer divide by zero");
                }
                self.push(Value::I32((a as u32).wrapping_div(b as u32) as i32))
            }
            0x6F => {
                let b = self.pop_i32()?;
                let a = self.pop_i32()?;
                if b == 0 {
                    return Err("integer divide by zero");
                }
                self.push(Value::I32(a.wrapping_rem(b)))
            }
            0x70 => {
                let b = self.pop_i32()?;
                let a = self.pop_i32()?;
                if b == 0 {
                    return Err("integer divide by zero");
                }
                self.push(Value::I32((a as u32).wrapping_rem(b as u32) as i32))
            }
            0x71 => {
                let b = self.pop_i32()?;
                let a = self.pop_i32()?;
                self.push(Value::I32(a & b))
            }
            0x72 => {
                let b = self.pop_i32()?;
                let a = self.pop_i32()?;
                self.push(Value::I32(a | b))
            }
            0x73 => {
                let b = self.pop_i32()?;
                let a = self.pop_i32()?;
                self.push(Value::I32(a ^ b))
            }
            0x74 => {
                let b = self.pop_i32()?;
                let a = self.pop_i32()?;
                self.push(Value::I32(a.wrapping_shl((b & 31) as u32)))
            }
            0x75 => {
                let b = self.pop_i32()?;
                let a = self.pop_i32()?;
                self.push(Value::I32(a.wrapping_shr((b & 31) as u32)))
            }
            0x76 => {
                let b = self.pop_i32()?;
                let a = self.pop_i32()?;
                self.push(Value::I32((a as u32).wrapping_shr((b & 31) as u32) as i32))
            }
            0x77 => {
                let b = self.pop_i32()?;
                let a = self.pop_i32()?;
                self.push(Value::I32(a.rotate_left((b & 31) as u32)))
            }
            0x78 => {
                let b = self.pop_i32()?;
                let a = self.pop_i32()?;
                self.push(Value::I32(a.rotate_right((b & 31) as u32)))
            }

            // i64 arithmetic
            0x79 => {
                let val = self.pop_i64()?;
                self.push(Value::I64(val.leading_zeros() as i64))
            }
            0x7A => {
                let val = self.pop_i64()?;
                self.push(Value::I64(val.trailing_zeros() as i64))
            }
            0x7B => {
                let val = self.pop_i64()?;
                self.push(Value::I64(val.count_ones() as i64))
            }
            0x7C => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                self.push(Value::I64(a.wrapping_add(b)))
            }
            0x7D => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                self.push(Value::I64(a.wrapping_sub(b)))
            }
            0x7E => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                self.push(Value::I64(a.wrapping_mul(b)))
            }
            0x7F => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                if b == 0 {
                    return Err("integer divide by zero");
                }
                self.push(Value::I64(a.wrapping_div(b)))
            }
            0x80 => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                if b == 0 {
                    return Err("integer divide by zero");
                }
                self.push(Value::I64((a as u64).wrapping_div(b as u64) as i64))
            }
            0x81 => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                if b == 0 {
                    return Err("integer divide by zero");
                }
                self.push(Value::I64(a.wrapping_rem(b)))
            }
            0x82 => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                if b == 0 {
                    return Err("integer divide by zero");
                }
                self.push(Value::I64((a as u64).wrapping_rem(b as u64) as i64))
            }
            0x83 => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                self.push(Value::I64(a & b))
            }
            0x84 => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                self.push(Value::I64(a | b))
            }
            0x85 => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                self.push(Value::I64(a ^ b))
            }
            0x86 => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                self.push(Value::I64(a.wrapping_shl((b & 63) as u32)))
            }
            0x87 => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                self.push(Value::I64(a.wrapping_shr((b & 63) as u32)))
            }
            0x88 => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                self.push(Value::I64((a as u64).wrapping_shr((b & 63) as u32) as i64))
            }
            0x89 => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                self.push(Value::I64(a.rotate_left((b & 63) as u32)))
            }
            0x8A => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                self.push(Value::I64(a.rotate_right((b & 63) as u32)))
            }

            _ => Err("unsupported opcode"),
        }
    }

    pub fn compute_state_hash(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();

        hasher.update(&[0x01]);
        hasher.update(&(self.stack.len() as u32).to_le_bytes());
        for val in &self.stack {
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

        hasher.update(&[0x02]);
        hasher.update(&(self.locals.len() as u32).to_le_bytes());
        for val in &self.locals {
            match val {
                Value::I32(v) => hasher.update(&v.to_le_bytes()),
                Value::I64(v) => hasher.update(&v.to_le_bytes()),
            }
        }

        hasher.update(&[0x03]);
        let mem_sample_size = self.memory.len().min(1024);
        hasher.update(&self.memory[..mem_sample_size]);

        hasher.update(&[0x04]);
        hasher.update(&self.pc.to_le_bytes());
        hasher.update(&self.fuel.to_le_bytes());

        let result = hasher.finalize();
        let mut output = [0u8; 32];
        output.copy_from_slice(&result);
        output
    }

    fn read_leb128_u32(&mut self, _bytecode: &[u8]) -> Result<u32, &'static str> {
        self.pc += 1;
        Ok(0)
    }

    fn read_leb128_i32(&mut self, _bytecode: &[u8]) -> Result<i32, &'static str> {
        self.pc += 1;
        Ok(0)
    }

    fn read_leb128_i64(&mut self, _bytecode: &[u8]) -> Result<i64, &'static str> {
        self.pc += 1;
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_i32_operations() {
        let mut interp = Interpreter::new(1024, 1000);

        interp.push(Value::I32(10)).unwrap();
        interp.push(Value::I32(20)).unwrap();
        interp.execute_opcode(0x6A, &[]).unwrap();
        assert_eq!(interp.pop_i32().unwrap(), 30);

        interp.push(Value::I32(7)).unwrap();
        interp.push(Value::I32(6)).unwrap();
        interp.execute_opcode(0x6C, &[]).unwrap();
        assert_eq!(interp.pop_i32().unwrap(), 42);

        interp.push(Value::I32(100)).unwrap();
        interp.push(Value::I32(42)).unwrap();
        interp.execute_opcode(0x6B, &[]).unwrap();
        assert_eq!(interp.pop_i32().unwrap(), 58);
    }

    #[test]
    fn test_i32_comparison() {
        let mut interp = Interpreter::new(1024, 1000);

        interp.push(Value::I32(42)).unwrap();
        interp.push(Value::I32(42)).unwrap();
        interp.execute_opcode(0x46, &[]).unwrap();
        assert_eq!(interp.pop_i32().unwrap(), 1);

        interp.push(Value::I32(10)).unwrap();
        interp.push(Value::I32(20)).unwrap();
        interp.execute_opcode(0x48, &[]).unwrap();
        assert_eq!(interp.pop_i32().unwrap(), 1);
    }

    #[test]
    fn test_i32_bitwise() {
        let mut interp = Interpreter::new(1024, 1000);

        interp.push(Value::I32(0b1010)).unwrap();
        interp.push(Value::I32(0b1100)).unwrap();
        interp.execute_opcode(0x71, &[]).unwrap();
        assert_eq!(interp.pop_i32().unwrap(), 0b1000);

        interp.push(Value::I32(0b1010)).unwrap();
        interp.push(Value::I32(0b1100)).unwrap();
        interp.execute_opcode(0x72, &[]).unwrap();
        assert_eq!(interp.pop_i32().unwrap(), 0b1110);
    }

    #[test]
    fn test_state_hash_deterministic() {
        let mut interp1 = Interpreter::new(1024, 1000);
        interp1.push(Value::I32(42)).unwrap();
        interp1.locals.push(Value::I32(100));
        let hash1 = interp1.compute_state_hash();

        let mut interp2 = Interpreter::new(1024, 1000);
        interp2.push(Value::I32(42)).unwrap();
        interp2.locals.push(Value::I32(100));
        let hash2 = interp2.compute_state_hash();

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_divide_by_zero() {
        let mut interp = Interpreter::new(1024, 1000);
        interp.push(Value::I32(10)).unwrap();
        interp.push(Value::I32(0)).unwrap();
        assert!(interp.execute_opcode(0x6D, &[]).is_err());
    }

    #[test]
    fn test_fuel_consumption() {
        let mut interp = Interpreter::new(1024, 5);
        interp.push(Value::I32(1)).unwrap();
        interp.push(Value::I32(2)).unwrap();
        interp.execute_opcode(0x6A, &[]).unwrap();
        interp.execute_opcode(0x6A, &[]).unwrap();
        interp.execute_opcode(0x6A, &[]).unwrap();
        interp.execute_opcode(0x6A, &[]).unwrap();
        assert!(interp.execute_opcode(0x6A, &[]).is_err());
    }
}

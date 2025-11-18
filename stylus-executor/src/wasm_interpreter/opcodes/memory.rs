// Memory operations: load, store, memory.size, memory.grow

use crate::wasm_interpreter::vm::Interpreter;
use crate::wasm_interpreter::types::Value;

pub fn execute(vm: &mut Interpreter, opcode: u8, bytecode: &[u8]) -> Result<(), &'static str> {
    match opcode {
        0x28 => load_i32(vm, bytecode),
        0x29 => load_i64(vm, bytecode),
        0x2A => load_i32_8s(vm, bytecode),
        0x2B => load_i32_8u(vm, bytecode),
        0x2C => load_i32_16s(vm, bytecode),
        0x2D => load_i32_16u(vm, bytecode),
        0x30 => load_i64_8s(vm, bytecode),
        0x31 => load_i64_8u(vm, bytecode),
        0x32 => load_i64_16s(vm, bytecode),
        0x33 => load_i64_16u(vm, bytecode),
        0x34 => load_i64_32s(vm, bytecode),
        0x35 => load_i64_32u(vm, bytecode),

        0x36 => store_i32(vm, bytecode),
        0x37 => store_i64(vm, bytecode),
        0x3A => store_i32_8(vm, bytecode),
        0x3B => store_i32_16(vm, bytecode),
        0x3C => store_i64_8(vm, bytecode),
        0x3D => store_i64_16(vm, bytecode),
        0x3E => store_i64_32(vm, bytecode),

        0x3F => memory_size(vm, bytecode),
        0x40 => memory_grow(vm, bytecode),

        _ => Err("invalid memory opcode"),
    }
}

fn compute_address(vm: &mut Interpreter, bytecode: &[u8]) -> Result<usize, &'static str> {
    let _align = vm.read_leb128_u32(bytecode)?;
    let offset = vm.read_leb128_u32(bytecode)? as usize;
    let base = vm.pop_i32()? as usize;
    base.checked_add(offset).ok_or("address overflow")
}

fn load_i32(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let addr = compute_address(vm, bytecode)?;
    let bytes = vm.load_memory(addr, 4)?;
    let val = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    vm.push(Value::I32(val))
}

fn load_i64(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let addr = compute_address(vm, bytecode)?;
    let bytes = vm.load_memory(addr, 8)?;
    let val = i64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5], bytes[6], bytes[7],
    ]);
    vm.push(Value::I64(val))
}

fn load_i32_8s(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let addr = compute_address(vm, bytecode)?;
    let byte = vm.load_memory(addr, 1)?[0];
    vm.push(Value::I32(byte as i8 as i32))
}

fn load_i32_8u(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let addr = compute_address(vm, bytecode)?;
    let byte = vm.load_memory(addr, 1)?[0];
    vm.push(Value::I32(byte as i32))
}

fn load_i32_16s(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let addr = compute_address(vm, bytecode)?;
    let bytes = vm.load_memory(addr, 2)?;
    let val = i16::from_le_bytes([bytes[0], bytes[1]]);
    vm.push(Value::I32(val as i32))
}

fn load_i32_16u(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let addr = compute_address(vm, bytecode)?;
    let bytes = vm.load_memory(addr, 2)?;
    let val = u16::from_le_bytes([bytes[0], bytes[1]]);
    vm.push(Value::I32(val as i32))
}

fn load_i64_8s(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let addr = compute_address(vm, bytecode)?;
    let byte = vm.load_memory(addr, 1)?[0];
    vm.push(Value::I64(byte as i8 as i64))
}

fn load_i64_8u(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let addr = compute_address(vm, bytecode)?;
    let byte = vm.load_memory(addr, 1)?[0];
    vm.push(Value::I64(byte as i64))
}

fn load_i64_16s(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let addr = compute_address(vm, bytecode)?;
    let bytes = vm.load_memory(addr, 2)?;
    let val = i16::from_le_bytes([bytes[0], bytes[1]]);
    vm.push(Value::I64(val as i64))
}

fn load_i64_16u(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let addr = compute_address(vm, bytecode)?;
    let bytes = vm.load_memory(addr, 2)?;
    let val = u16::from_le_bytes([bytes[0], bytes[1]]);
    vm.push(Value::I64(val as i64))
}

fn load_i64_32s(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let addr = compute_address(vm, bytecode)?;
    let bytes = vm.load_memory(addr, 4)?;
    let val = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    vm.push(Value::I64(val as i64))
}

fn load_i64_32u(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let addr = compute_address(vm, bytecode)?;
    let bytes = vm.load_memory(addr, 4)?;
    let val = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    vm.push(Value::I64(val as i64))
}

fn store_i32(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let val = vm.pop_i32()?;
    let addr = compute_address(vm, bytecode)?;
    vm.store_memory(addr, &val.to_le_bytes())
}

fn store_i64(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let val = vm.pop_i64()?;
    let addr = compute_address(vm, bytecode)?;
    vm.store_memory(addr, &val.to_le_bytes())
}

fn store_i32_8(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let val = vm.pop_i32()?;
    let addr = compute_address(vm, bytecode)?;
    vm.store_memory(addr, &[val as u8])
}

fn store_i32_16(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let val = vm.pop_i32()?;
    let addr = compute_address(vm, bytecode)?;
    vm.store_memory(addr, &(val as u16).to_le_bytes())
}

fn store_i64_8(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let val = vm.pop_i64()?;
    let addr = compute_address(vm, bytecode)?;
    vm.store_memory(addr, &[val as u8])
}

fn store_i64_16(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let val = vm.pop_i64()?;
    let addr = compute_address(vm, bytecode)?;
    vm.store_memory(addr, &(val as u16).to_le_bytes())
}

fn store_i64_32(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let val = vm.pop_i64()?;
    let addr = compute_address(vm, bytecode)?;
    vm.store_memory(addr, &(val as u32).to_le_bytes())
}

fn memory_size(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let _reserved = vm.read_leb128_u32(bytecode)?;
    vm.push(Value::I32(vm.memory_size()))
}

fn memory_grow(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let _reserved = vm.read_leb128_u32(bytecode)?;
    let pages = vm.pop_i32()?;
    let result = vm.grow_memory(pages as u32)?;
    vm.push(Value::I32(result))
}

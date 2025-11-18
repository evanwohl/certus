// Variable access: local.get, local.set, local.tee

use crate::wasm_interpreter::vm::Interpreter;

pub fn execute(vm: &mut Interpreter, opcode: u8, bytecode: &[u8]) -> Result<(), &'static str> {
    match opcode {
        0x20 => op_local_get(vm, bytecode),
        0x21 => op_local_set(vm, bytecode),
        0x22 => op_local_tee(vm, bytecode),
        _ => Err("invalid variable opcode"),
    }
}

fn op_local_get(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let idx = vm.read_leb128_u32(bytecode)? as usize;
    let val = vm.get_local(idx)?;
    vm.push(val)
}

fn op_local_set(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let idx = vm.read_leb128_u32(bytecode)? as usize;
    let val = vm.pop()?;
    vm.set_local(idx, val)
}

fn op_local_tee(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let idx = vm.read_leb128_u32(bytecode)? as usize;
    let val = vm.pop()?;
    vm.set_local(idx, val)?;
    vm.push(val)
}

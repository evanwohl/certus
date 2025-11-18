// Opcode dispatch and implementation

mod control;
mod numeric;
mod memory;
mod variable;

use super::vm::Interpreter;

pub fn dispatch(vm: &mut Interpreter, opcode: u8, bytecode: &[u8]) -> Result<(), &'static str> {
    match opcode {
        // Control flow
        0x00..=0x0F => control::execute(vm, opcode, bytecode),

        // Variable access
        0x20..=0x22 => variable::execute(vm, opcode, bytecode),

        // Memory operations
        0x28..=0x40 => memory::execute(vm, opcode, bytecode),

        // Constants
        0x41 => {
            let val = vm.read_leb128_i32(bytecode)?;
            vm.push(crate::wasm_interpreter::types::Value::I32(val))
        }
        0x42 => {
            let val = vm.read_leb128_i64(bytecode)?;
            vm.push(crate::wasm_interpreter::types::Value::I64(val))
        }

        // Numeric operations
        0x45..=0xC4 => numeric::execute(vm, opcode),

        _ => Err("unsupported opcode"),
    }
}

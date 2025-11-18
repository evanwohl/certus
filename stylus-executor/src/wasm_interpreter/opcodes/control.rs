// Control flow instructions: block, loop, if, br, return

use crate::wasm_interpreter::vm::Interpreter;
use crate::wasm_interpreter::types::BlockFrame;

pub fn execute(vm: &mut Interpreter, opcode: u8, bytecode: &[u8]) -> Result<(), &'static str> {
    match opcode {
        0x00 => Err("unreachable executed"),
        0x01 => Ok(()),

        0x02 => op_block(vm, bytecode),
        0x03 => op_loop(vm, bytecode),
        0x04 => op_if(vm, bytecode),
        0x05 => op_else(vm),
        0x0B => op_end(vm),
        0x0C => op_br(vm, bytecode),
        0x0D => op_br_if(vm, bytecode),
        0x0E => op_br_table(vm, bytecode),
        0x0F => op_return(vm),

        _ => Err("invalid control opcode"),
    }
}

fn op_block(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let _block_type = read_block_type(vm, bytecode)?;
    let start_pc = vm.get_pc() - 2;
    let end_pc = vm.find_matching_end(start_pc)?;

    vm.push_block(BlockFrame {
        start_pc,
        end_pc,
        stack_height: vm.stack_height(),
        is_loop: false,
        result_type: None,
    })
}

fn op_loop(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let _block_type = read_block_type(vm, bytecode)?;
    let start_pc = vm.get_pc() - 2;
    let end_pc = vm.find_matching_end(start_pc)?;

    vm.push_block(BlockFrame {
        start_pc,
        end_pc,
        stack_height: vm.stack_height(),
        is_loop: true,
        result_type: None,
    })
}

fn op_if(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let _block_type = read_block_type(vm, bytecode)?;
    let condition = vm.pop_i32()?;
    let start_pc = vm.get_pc() - 2;

    if condition == 0 {
        let target = vm.find_else_or_end(bytecode)?;
        vm.set_pc(target);
    } else {
        let end_pc = vm.find_matching_end(start_pc)?;
        vm.push_block(BlockFrame {
            start_pc,
            end_pc,
            stack_height: vm.stack_height(),
            is_loop: false,
            result_type: None,
        })?;
    }

    Ok(())
}

fn op_else(vm: &mut Interpreter) -> Result<(), &'static str> {
    let block = vm.pop_block()?;
    vm.set_pc(block.end_pc);
    Ok(())
}

fn op_end(vm: &mut Interpreter) -> Result<(), &'static str> {
    if vm.peek_block().is_some() {
        vm.pop_block()?;
        Ok(())
    } else {
        Ok(())
    }
}

fn op_br(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let depth = vm.read_leb128_u32(bytecode)? as usize;
    vm.branch(depth)
}

fn op_br_if(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let depth = vm.read_leb128_u32(bytecode)? as usize;
    let condition = vm.pop_i32()?;

    if condition != 0 {
        vm.branch(depth)?;
    }

    Ok(())
}

fn op_br_table(vm: &mut Interpreter, bytecode: &[u8]) -> Result<(), &'static str> {
    let count = vm.read_leb128_u32(bytecode)?;
    let mut targets = alloc::vec::Vec::with_capacity(count as usize);

    for _ in 0..count {
        targets.push(vm.read_leb128_u32(bytecode)? as usize);
    }

    let default_target = vm.read_leb128_u32(bytecode)? as usize;
    let index = vm.pop_i32()? as usize;

    let target = if index < targets.len() {
        targets[index]
    } else {
        default_target
    };

    vm.branch(target)
}

fn op_return(vm: &mut Interpreter) -> Result<(), &'static str> {
    let frame = vm.pop_call_frame()?;
    vm.set_pc(frame.return_pc);
    Ok(())
}

fn read_block_type(vm: &mut Interpreter, bytecode: &[u8]) -> Result<Option<u8>, &'static str> {
    let pc = vm.get_pc();
    if pc >= bytecode.len() {
        return Ok(None);
    }

    let byte = bytecode[pc];
    vm.set_pc(pc + 1);

    match byte {
        0x40 => Ok(None),
        0x7F | 0x7E | 0x7D | 0x7C => Ok(Some(byte)),
        _ => Err("invalid block type"),
    }
}

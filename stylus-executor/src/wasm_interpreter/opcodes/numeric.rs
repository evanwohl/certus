// Numeric operations: i32/i64 arithmetic, comparisons, conversions

use crate::wasm_interpreter::vm::Interpreter;
use crate::wasm_interpreter::types::Value;

pub fn execute(vm: &mut Interpreter, opcode: u8) -> Result<(), &'static str> {
    match opcode {
        // i32 comparisons
        0x45 => i32_eqz(vm),
        0x46 => i32_eq(vm),
        0x47 => i32_ne(vm),
        0x48 => i32_lt_s(vm),
        0x49 => i32_lt_u(vm),
        0x4A => i32_gt_s(vm),
        0x4B => i32_gt_u(vm),
        0x4C => i32_le_s(vm),
        0x4D => i32_le_u(vm),
        0x4E => i32_ge_s(vm),
        0x4F => i32_ge_u(vm),

        // i64 comparisons
        0x50 => i64_eqz(vm),
        0x51 => i64_eq(vm),
        0x52 => i64_ne(vm),
        0x53 => i64_lt_s(vm),
        0x54 => i64_lt_u(vm),
        0x55 => i64_gt_s(vm),
        0x56 => i64_gt_u(vm),
        0x57 => i64_le_s(vm),
        0x58 => i64_le_u(vm),
        0x59 => i64_ge_s(vm),
        0x5A => i64_ge_u(vm),

        // i32 arithmetic
        0x67 => i32_clz(vm),
        0x68 => i32_ctz(vm),
        0x69 => i32_popcnt(vm),
        0x6A => i32_add(vm),
        0x6B => i32_sub(vm),
        0x6C => i32_mul(vm),
        0x6D => i32_div_s(vm),
        0x6E => i32_div_u(vm),
        0x6F => i32_rem_s(vm),
        0x70 => i32_rem_u(vm),
        0x71 => i32_and(vm),
        0x72 => i32_or(vm),
        0x73 => i32_xor(vm),
        0x74 => i32_shl(vm),
        0x75 => i32_shr_s(vm),
        0x76 => i32_shr_u(vm),
        0x77 => i32_rotl(vm),
        0x78 => i32_rotr(vm),

        // i64 arithmetic
        0x79 => i64_clz(vm),
        0x7A => i64_ctz(vm),
        0x7B => i64_popcnt(vm),
        0x7C => i64_add(vm),
        0x7D => i64_sub(vm),
        0x7E => i64_mul(vm),
        0x7F => i64_div_s(vm),
        0x80 => i64_div_u(vm),
        0x81 => i64_rem_s(vm),
        0x82 => i64_rem_u(vm),
        0x83 => i64_and(vm),
        0x84 => i64_or(vm),
        0x85 => i64_xor(vm),
        0x86 => i64_shl(vm),
        0x87 => i64_shr_s(vm),
        0x88 => i64_shr_u(vm),
        0x89 => i64_rotl(vm),
        0x8A => i64_rotr(vm),

        // Conversions
        0xA7 => i32_wrap_i64(vm),
        0xAC => i64_extend_i32_s(vm),
        0xAD => i64_extend_i32_u(vm),

        _ => Err("unsupported numeric opcode"),
    }
}

// i32 comparisons

fn i32_eqz(vm: &mut Interpreter) -> Result<(), &'static str> {
    let a = vm.pop_i32()?;
    vm.push(Value::I32(if a == 0 { 1 } else { 0 }))
}

fn i32_eq(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i32()?;
    let a = vm.pop_i32()?;
    vm.push(Value::I32(if a == b { 1 } else { 0 }))
}

fn i32_ne(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i32()?;
    let a = vm.pop_i32()?;
    vm.push(Value::I32(if a != b { 1 } else { 0 }))
}

fn i32_lt_s(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i32()?;
    let a = vm.pop_i32()?;
    vm.push(Value::I32(if a < b { 1 } else { 0 }))
}

fn i32_lt_u(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i32()?;
    let a = vm.pop_i32()?;
    vm.push(Value::I32(if (a as u32) < (b as u32) { 1 } else { 0 }))
}

fn i32_gt_s(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i32()?;
    let a = vm.pop_i32()?;
    vm.push(Value::I32(if a > b { 1 } else { 0 }))
}

fn i32_gt_u(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i32()?;
    let a = vm.pop_i32()?;
    vm.push(Value::I32(if (a as u32) > (b as u32) { 1 } else { 0 }))
}

fn i32_le_s(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i32()?;
    let a = vm.pop_i32()?;
    vm.push(Value::I32(if a <= b { 1 } else { 0 }))
}

fn i32_le_u(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i32()?;
    let a = vm.pop_i32()?;
    vm.push(Value::I32(if (a as u32) <= (b as u32) { 1 } else { 0 }))
}

fn i32_ge_s(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i32()?;
    let a = vm.pop_i32()?;
    vm.push(Value::I32(if a >= b { 1 } else { 0 }))
}

fn i32_ge_u(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i32()?;
    let a = vm.pop_i32()?;
    vm.push(Value::I32(if (a as u32) >= (b as u32) { 1 } else { 0 }))
}

// i64 comparisons

fn i64_eqz(vm: &mut Interpreter) -> Result<(), &'static str> {
    let a = vm.pop_i64()?;
    vm.push(Value::I32(if a == 0 { 1 } else { 0 }))
}

fn i64_eq(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i64()?;
    let a = vm.pop_i64()?;
    vm.push(Value::I32(if a == b { 1 } else { 0 }))
}

fn i64_ne(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i64()?;
    let a = vm.pop_i64()?;
    vm.push(Value::I32(if a != b { 1 } else { 0 }))
}

fn i64_lt_s(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i64()?;
    let a = vm.pop_i64()?;
    vm.push(Value::I32(if a < b { 1 } else { 0 }))
}

fn i64_lt_u(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i64()?;
    let a = vm.pop_i64()?;
    vm.push(Value::I32(if (a as u64) < (b as u64) { 1 } else { 0 }))
}

fn i64_gt_s(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i64()?;
    let a = vm.pop_i64()?;
    vm.push(Value::I32(if a > b { 1 } else { 0 }))
}

fn i64_gt_u(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i64()?;
    let a = vm.pop_i64()?;
    vm.push(Value::I32(if (a as u64) > (b as u64) { 1 } else { 0 }))
}

fn i64_le_s(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i64()?;
    let a = vm.pop_i64()?;
    vm.push(Value::I32(if a <= b { 1 } else { 0 }))
}

fn i64_le_u(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i64()?;
    let a = vm.pop_i64()?;
    vm.push(Value::I32(if (a as u64) <= (b as u64) { 1 } else { 0 }))
}

fn i64_ge_s(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i64()?;
    let a = vm.pop_i64()?;
    vm.push(Value::I32(if a >= b { 1 } else { 0 }))
}

fn i64_ge_u(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i64()?;
    let a = vm.pop_i64()?;
    vm.push(Value::I32(if (a as u64) >= (b as u64) { 1 } else { 0 }))
}

// i32 arithmetic

fn i32_clz(vm: &mut Interpreter) -> Result<(), &'static str> {
    let a = vm.pop_i32()?;
    vm.push(Value::I32(a.leading_zeros() as i32))
}

fn i32_ctz(vm: &mut Interpreter) -> Result<(), &'static str> {
    let a = vm.pop_i32()?;
    vm.push(Value::I32(a.trailing_zeros() as i32))
}

fn i32_popcnt(vm: &mut Interpreter) -> Result<(), &'static str> {
    let a = vm.pop_i32()?;
    vm.push(Value::I32(a.count_ones() as i32))
}

fn i32_add(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i32()?;
    let a = vm.pop_i32()?;
    vm.push(Value::I32(a.wrapping_add(b)))
}

fn i32_sub(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i32()?;
    let a = vm.pop_i32()?;
    vm.push(Value::I32(a.wrapping_sub(b)))
}

fn i32_mul(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i32()?;
    let a = vm.pop_i32()?;
    vm.push(Value::I32(a.wrapping_mul(b)))
}

fn i32_div_s(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i32()?;
    let a = vm.pop_i32()?;
    if b == 0 {
        return Err("integer divide by zero");
    }
    vm.push(Value::I32(a.wrapping_div(b)))
}

fn i32_div_u(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i32()?;
    let a = vm.pop_i32()?;
    if b == 0 {
        return Err("integer divide by zero");
    }
    vm.push(Value::I32((a as u32).wrapping_div(b as u32) as i32))
}

fn i32_rem_s(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i32()?;
    let a = vm.pop_i32()?;
    if b == 0 {
        return Err("integer divide by zero");
    }
    vm.push(Value::I32(a.wrapping_rem(b)))
}

fn i32_rem_u(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i32()?;
    let a = vm.pop_i32()?;
    if b == 0 {
        return Err("integer divide by zero");
    }
    vm.push(Value::I32((a as u32).wrapping_rem(b as u32) as i32))
}

fn i32_and(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i32()?;
    let a = vm.pop_i32()?;
    vm.push(Value::I32(a & b))
}

fn i32_or(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i32()?;
    let a = vm.pop_i32()?;
    vm.push(Value::I32(a | b))
}

fn i32_xor(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i32()?;
    let a = vm.pop_i32()?;
    vm.push(Value::I32(a ^ b))
}

fn i32_shl(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i32()?;
    let a = vm.pop_i32()?;
    vm.push(Value::I32(a.wrapping_shl((b & 31) as u32)))
}

fn i32_shr_s(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i32()?;
    let a = vm.pop_i32()?;
    vm.push(Value::I32(a.wrapping_shr((b & 31) as u32)))
}

fn i32_shr_u(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i32()?;
    let a = vm.pop_i32()?;
    vm.push(Value::I32((a as u32).wrapping_shr((b & 31) as u32) as i32))
}

fn i32_rotl(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i32()?;
    let a = vm.pop_i32()?;
    vm.push(Value::I32(a.rotate_left((b & 31) as u32)))
}

fn i32_rotr(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i32()?;
    let a = vm.pop_i32()?;
    vm.push(Value::I32(a.rotate_right((b & 31) as u32)))
}

// i64 arithmetic

fn i64_clz(vm: &mut Interpreter) -> Result<(), &'static str> {
    let a = vm.pop_i64()?;
    vm.push(Value::I64(a.leading_zeros() as i64))
}

fn i64_ctz(vm: &mut Interpreter) -> Result<(), &'static str> {
    let a = vm.pop_i64()?;
    vm.push(Value::I64(a.trailing_zeros() as i64))
}

fn i64_popcnt(vm: &mut Interpreter) -> Result<(), &'static str> {
    let a = vm.pop_i64()?;
    vm.push(Value::I64(a.count_ones() as i64))
}

fn i64_add(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i64()?;
    let a = vm.pop_i64()?;
    vm.push(Value::I64(a.wrapping_add(b)))
}

fn i64_sub(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i64()?;
    let a = vm.pop_i64()?;
    vm.push(Value::I64(a.wrapping_sub(b)))
}

fn i64_mul(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i64()?;
    let a = vm.pop_i64()?;
    vm.push(Value::I64(a.wrapping_mul(b)))
}

fn i64_div_s(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i64()?;
    let a = vm.pop_i64()?;
    if b == 0 {
        return Err("integer divide by zero");
    }
    vm.push(Value::I64(a.wrapping_div(b)))
}

fn i64_div_u(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i64()?;
    let a = vm.pop_i64()?;
    if b == 0 {
        return Err("integer divide by zero");
    }
    vm.push(Value::I64((a as u64).wrapping_div(b as u64) as i64))
}

fn i64_rem_s(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i64()?;
    let a = vm.pop_i64()?;
    if b == 0 {
        return Err("integer divide by zero");
    }
    vm.push(Value::I64(a.wrapping_rem(b)))
}

fn i64_rem_u(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i64()?;
    let a = vm.pop_i64()?;
    if b == 0 {
        return Err("integer divide by zero");
    }
    vm.push(Value::I64((a as u64).wrapping_rem(b as u64) as i64))
}

fn i64_and(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i64()?;
    let a = vm.pop_i64()?;
    vm.push(Value::I64(a & b))
}

fn i64_or(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i64()?;
    let a = vm.pop_i64()?;
    vm.push(Value::I64(a | b))
}

fn i64_xor(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i64()?;
    let a = vm.pop_i64()?;
    vm.push(Value::I64(a ^ b))
}

fn i64_shl(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i64()?;
    let a = vm.pop_i64()?;
    vm.push(Value::I64(a.wrapping_shl((b & 63) as u32)))
}

fn i64_shr_s(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i64()?;
    let a = vm.pop_i64()?;
    vm.push(Value::I64(a.wrapping_shr((b & 63) as u32)))
}

fn i64_shr_u(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i64()?;
    let a = vm.pop_i64()?;
    vm.push(Value::I64((a as u64).wrapping_shr((b & 63) as u32) as i64))
}

fn i64_rotl(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i64()?;
    let a = vm.pop_i64()?;
    vm.push(Value::I64(a.rotate_left((b & 63) as u32)))
}

fn i64_rotr(vm: &mut Interpreter) -> Result<(), &'static str> {
    let b = vm.pop_i64()?;
    let a = vm.pop_i64()?;
    vm.push(Value::I64(a.rotate_right((b & 63) as u32)))
}

// Conversions

fn i32_wrap_i64(vm: &mut Interpreter) -> Result<(), &'static str> {
    let a = vm.pop_i64()?;
    vm.push(Value::I32(a as i32))
}

fn i64_extend_i32_s(vm: &mut Interpreter) -> Result<(), &'static str> {
    let a = vm.pop_i32()?;
    vm.push(Value::I64(a as i64))
}

fn i64_extend_i32_u(vm: &mut Interpreter) -> Result<(), &'static str> {
    let a = vm.pop_i32()?;
    vm.push(Value::I64((a as u32) as i64))
}

// Bytecode structure validation

use alloc::vec::Vec;

const MAX_FUNCTION_LOCALS: usize = 50000;
const MAX_FUNCTION_PARAMS: usize = 1000;

pub struct BytecodeValidator;

#[derive(Debug, Clone, Copy)]
struct BlockInfo {
    start: usize,
    end: usize,
    block_type: BlockType,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum BlockType {
    Block,
    Loop,
    If,
}

impl BytecodeValidator {
    /// Validate Wasm bytecode structure
    /// Returns Ok(jump_table) mapping block start -> end positions
    pub fn validate(bytecode: &[u8]) -> Result<JumpTable, &'static str> {
        Self::check_magic_version(bytecode)?;
        Self::validate_sections(bytecode)?;
        Self::build_jump_table(bytecode)
    }

    fn check_magic_version(bytecode: &[u8]) -> Result<(), &'static str> {
        if bytecode.len() < 8 {
            return Err("bytecode too short");
        }

        if &bytecode[0..4] != b"\0asm" {
            return Err("invalid magic number");
        }

        if &bytecode[4..8] != &[1, 0, 0, 0] {
            return Err("unsupported wasm version");
        }

        Ok(())
    }

    fn validate_sections(bytecode: &[u8]) -> Result<(), &'static str> {
        let mut pos = 8;

        while pos < bytecode.len() {
            if pos + 1 >= bytecode.len() {
                break;
            }

            let section_id = bytecode[pos];
            pos += 1;

            let (section_size, size_bytes) = Self::read_leb128_u32(bytecode, pos)?;
            pos += size_bytes;

            if pos + section_size as usize > bytecode.len() {
                return Err("section extends beyond bytecode");
            }

            match section_id {
                1 => Self::validate_type_section(&bytecode[pos..pos + section_size as usize])?,
                3 => Self::validate_function_section(&bytecode[pos..pos + section_size as usize])?,
                10 => Self::validate_code_section(&bytecode[pos..pos + section_size as usize])?,
                _ => {}
            }

            pos += section_size as usize;
        }

        Ok(())
    }

    fn validate_type_section(data: &[u8]) -> Result<(), &'static str> {
        let mut pos = 0;
        let (count, size) = Self::read_leb128_u32(data, pos)?;
        pos += size;

        for _ in 0..count {
            if pos >= data.len() {
                return Err("unexpected end in type section");
            }

            if data[pos] != 0x60 {
                return Err("invalid function type");
            }
            pos += 1;

            let (param_count, size) = Self::read_leb128_u32(data, pos)?;
            pos += size;

            if param_count > MAX_FUNCTION_PARAMS as u32 {
                return Err("too many function parameters");
            }

            pos += param_count as usize;

            if pos >= data.len() {
                return Err("unexpected end in type section");
            }

            let (result_count, size) = Self::read_leb128_u32(data, pos)?;
            pos += size;

            if result_count > 1 {
                return Err("multi-value returns not supported");
            }

            pos += result_count as usize;
        }

        Ok(())
    }

    fn validate_function_section(data: &[u8]) -> Result<(), &'static str> {
        let mut pos = 0;
        let (count, size) = Self::read_leb128_u32(data, pos)?;
        pos += size;

        for _ in 0..count {
            let (_type_idx, size) = Self::read_leb128_u32(data, pos)?;
            pos += size;
        }

        Ok(())
    }

    fn validate_code_section(data: &[u8]) -> Result<(), &'static str> {
        let mut pos = 0;
        let (count, size) = Self::read_leb128_u32(data, pos)?;
        pos += size;

        for _ in 0..count {
            let (body_size, size) = Self::read_leb128_u32(data, pos)?;
            pos += size;

            if pos + body_size as usize > data.len() {
                return Err("function body extends beyond section");
            }

            Self::validate_function_body(&data[pos..pos + body_size as usize])?;
            pos += body_size as usize;
        }

        Ok(())
    }

    fn validate_function_body(body: &[u8]) -> Result<(), &'static str> {
        let mut pos = 0;
        let (local_groups, size) = Self::read_leb128_u32(body, pos)?;
        pos += size;

        let mut total_locals = 0u32;

        for _ in 0..local_groups {
            let (count, size) = Self::read_leb128_u32(body, pos)?;
            pos += size;

            total_locals = total_locals.checked_add(count)
                .ok_or("too many locals")?;

            if total_locals > MAX_FUNCTION_LOCALS as u32 {
                return Err("locals exceed maximum");
            }

            if pos >= body.len() {
                return Err("unexpected end in function body");
            }

            pos += 1;
        }

        Ok(())
    }

    /// Build jump table mapping block start PC -> end PC
    /// Pre-computes all block boundaries for O(1) lookup
    pub fn build_jump_table(bytecode: &[u8]) -> Result<JumpTable, &'static str> {
        let mut table = JumpTable::new();
        let mut block_stack: Vec<BlockInfo> = Vec::new();

        let mut pc = 8;

        while pc < bytecode.len() {
            let opcode = bytecode[pc];

            match opcode {
                0x02 => {
                    block_stack.push(BlockInfo {
                        start: pc,
                        end: 0,
                        block_type: BlockType::Block,
                    });
                    pc += 1;
                    let (_type, size) = Self::read_block_type(bytecode, pc)?;
                    pc += size;
                }
                0x03 => {
                    block_stack.push(BlockInfo {
                        start: pc,
                        end: 0,
                        block_type: BlockType::Loop,
                    });
                    pc += 1;
                    let (_type, size) = Self::read_block_type(bytecode, pc)?;
                    pc += size;
                }
                0x04 => {
                    block_stack.push(BlockInfo {
                        start: pc,
                        end: 0,
                        block_type: BlockType::If,
                    });
                    pc += 1;
                    let (_type, size) = Self::read_block_type(bytecode, pc)?;
                    pc += size;
                }
                0x05 => {
                    if block_stack.is_empty() {
                        return Err("else without matching if");
                    }
                    let top = block_stack.last().unwrap();
                    if top.block_type != BlockType::If {
                        return Err("else without if");
                    }
                    pc += 1;
                }
                0x0B => {
                    if let Some(block) = block_stack.pop() {
                        table.insert(block.start, pc);
                    }
                    pc += 1;
                }
                _ => {
                    pc += Self::opcode_size(bytecode, pc)?;
                }
            }
        }

        if !block_stack.is_empty() {
            return Err("unmatched block");
        }

        Ok(table)
    }

    fn opcode_size(bytecode: &[u8], pc: usize) -> Result<usize, &'static str> {
        if pc >= bytecode.len() {
            return Err("pc out of bounds");
        }

        let opcode = bytecode[pc];
        let mut size = 1;

        match opcode {
            0x20 | 0x21 | 0x22 | 0x0C | 0x0D => {
                let (_, s) = Self::read_leb128_u32(bytecode, pc + 1)?;
                size += s;
            }
            0x41 => {
                let (_, s) = Self::read_leb128_i32(bytecode, pc + 1)?;
                size += s;
            }
            0x42 => {
                let (_, s) = Self::read_leb128_i64(bytecode, pc + 1)?;
                size += s;
            }
            0x28..=0x3E => {
                let (_, s1) = Self::read_leb128_u32(bytecode, pc + 1)?;
                let (_, s2) = Self::read_leb128_u32(bytecode, pc + 1 + s1)?;
                size += s1 + s2;
            }
            0x3F | 0x40 => {
                let (_, s) = Self::read_leb128_u32(bytecode, pc + 1)?;
                size += s;
            }
            _ => {}
        }

        Ok(size)
    }

    fn read_block_type(bytecode: &[u8], pc: usize) -> Result<(Option<u8>, usize), &'static str> {
        if pc >= bytecode.len() {
            return Ok((None, 0));
        }

        let byte = bytecode[pc];
        match byte {
            0x40 => Ok((None, 1)),
            0x7F | 0x7E | 0x7D | 0x7C => Ok((Some(byte), 1)),
            _ => Err("invalid block type"),
        }
    }

    fn read_leb128_u32(data: &[u8], pos: usize) -> Result<(u32, usize), &'static str> {
        let mut result = 0u32;
        let mut shift = 0;

        for i in 0..5 {
            if pos + i >= data.len() {
                return Err("unexpected end of data");
            }

            let byte = data[pos + i];
            result |= ((byte & 0x7F) as u32) << shift;

            if byte & 0x80 == 0 {
                return Ok((result, i + 1));
            }

            shift += 7;
        }

        Err("LEB128 too long")
    }

    fn read_leb128_i32(data: &[u8], pos: usize) -> Result<(i32, usize), &'static str> {
        let mut result = 0i32;
        let mut shift = 0;
        let mut byte;

        for i in 0..5 {
            if pos + i >= data.len() {
                return Err("unexpected end of data");
            }

            byte = data[pos + i];
            result |= ((byte & 0x7F) as i32) << shift;
            shift += 7;

            if byte & 0x80 == 0 {
                if shift < 32 && (byte & 0x40) != 0 {
                    result |= !0 << shift;
                }
                return Ok((result, i + 1));
            }
        }

        Err("LEB128 too long")
    }

    fn read_leb128_i64(data: &[u8], pos: usize) -> Result<(i64, usize), &'static str> {
        let mut result = 0i64;
        let mut shift = 0;
        let mut byte;

        for i in 0..10 {
            if pos + i >= data.len() {
                return Err("unexpected end of data");
            }

            byte = data[pos + i];
            result |= ((byte & 0x7F) as i64) << shift;
            shift += 7;

            if byte & 0x80 == 0 {
                if shift < 64 && (byte & 0x40) != 0 {
                    result |= !0 << shift;
                }
                return Ok((result, i + 1));
            }
        }

        Err("LEB128 too long")
    }
}

pub struct JumpTable {
    entries: Vec<(usize, usize)>,
}

impl JumpTable {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn insert(&mut self, start: usize, end: usize) {
        self.entries.push((start, end));
    }

    pub fn get(&self, start: usize) -> Option<usize> {
        self.entries.iter()
            .find(|(s, _)| *s == start)
            .map(|(_, e)| *e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_magic() {
        let bytecode = b"\0asm\x01\0\0\0";
        assert!(BytecodeValidator::check_magic_version(bytecode).is_ok());
    }

    #[test]
    fn test_invalid_magic() {
        let bytecode = b"WASM\x01\0\0\0";
        assert!(BytecodeValidator::check_magic_version(bytecode).is_err());
    }

    #[test]
    fn test_jump_table_matching_blocks() {
        let mut bytecode = Vec::new();
        bytecode.extend_from_slice(b"\0asm\x01\0\0\0");
        bytecode.push(0x02);
        bytecode.push(0x40);
        bytecode.push(0x01);
        bytecode.push(0x0B);

        let table = BytecodeValidator::build_jump_table(&bytecode).unwrap();
        assert_eq!(table.get(8), Some(11));
    }

    #[test]
    fn test_unmatched_block() {
        let mut bytecode = Vec::new();
        bytecode.extend_from_slice(b"\0asm\x01\0\0\0");
        bytecode.push(0x02);
        bytecode.push(0x40);

        assert!(BytecodeValidator::build_jump_table(&bytecode).is_err());
    }
}

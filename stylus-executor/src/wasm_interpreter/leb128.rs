// LEB128 variable-length integer decoding

pub struct Leb128Reader<'a> {
    bytecode: &'a [u8],
    pc: usize,
}

impl<'a> Leb128Reader<'a> {
    pub fn new(bytecode: &'a [u8], pc: usize) -> Self {
        Self { bytecode, pc }
    }

    pub fn read_u32(&mut self) -> Result<(u32, usize), &'static str> {
        let mut result = 0u32;
        let mut shift = 0;
        let start_pc = self.pc;

        for i in 0..5 {
            if self.pc + i >= self.bytecode.len() {
                return Err("unexpected end of bytecode");
            }

            let byte = self.bytecode[self.pc + i];
            result |= ((byte & 0x7F) as u32) << shift;

            if byte & 0x80 == 0 {
                let bytes_read = i + 1;
                self.pc += bytes_read;
                return Ok((result, bytes_read));
            }

            shift += 7;
        }

        Err("LEB128 too long")
    }

    pub fn read_i32(&mut self) -> Result<(i32, usize), &'static str> {
        let mut result = 0i32;
        let mut shift = 0;
        let mut byte = 0u8;
        let start_pc = self.pc;

        for i in 0..5 {
            if self.pc + i >= self.bytecode.len() {
                return Err("unexpected end of bytecode");
            }

            byte = self.bytecode[self.pc + i];
            result |= ((byte & 0x7F) as i32) << shift;
            shift += 7;

            if byte & 0x80 == 0 {
                let bytes_read = i + 1;
                self.pc += bytes_read;

                // Sign extend
                if shift < 32 && (byte & 0x40) != 0 {
                    result |= !0 << shift;
                }

                return Ok((result, bytes_read));
            }
        }

        Err("LEB128 too long")
    }

    pub fn read_i64(&mut self) -> Result<(i64, usize), &'static str> {
        let mut result = 0i64;
        let mut shift = 0;
        let mut byte = 0u8;
        let start_pc = self.pc;

        for i in 0..10 {
            if self.pc + i >= self.bytecode.len() {
                return Err("unexpected end of bytecode");
            }

            byte = self.bytecode[self.pc + i];
            result |= ((byte & 0x7F) as i64) << shift;
            shift += 7;

            if byte & 0x80 == 0 {
                let bytes_read = i + 1;
                self.pc += bytes_read;

                // Sign extend
                if shift < 64 && (byte & 0x40) != 0 {
                    result |= !0 << shift;
                }

                return Ok((result, bytes_read));
            }
        }

        Err("LEB128 too long")
    }

    pub fn current_pc(&self) -> usize {
        self.pc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_u32_single_byte() {
        let bytes = [0x05];
        let mut reader = Leb128Reader::new(&bytes, 0);
        assert_eq!(reader.read_u32().unwrap(), (5, 1));
    }

    #[test]
    fn test_read_u32_multi_byte() {
        let bytes = [0xE5, 0x8E, 0x26];
        let mut reader = Leb128Reader::new(&bytes, 0);
        assert_eq!(reader.read_u32().unwrap(), (624485, 3));
    }

    #[test]
    fn test_read_i32_negative() {
        let bytes = [0x7F];
        let mut reader = Leb128Reader::new(&bytes, 0);
        assert_eq!(reader.read_i32().unwrap(), (-1, 1));
    }

    #[test]
    fn test_read_i64() {
        let bytes = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00];
        let mut reader = Leb128Reader::new(&bytes, 0);
        assert_eq!(reader.read_i64().unwrap(), (i64::MAX, 10));
    }
}

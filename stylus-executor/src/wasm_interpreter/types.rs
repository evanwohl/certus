// Core type definitions

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    pub fn value_type(&self) -> ValueType {
        match self {
            Value::I32(_) => ValueType::I32,
            Value::I64(_) => ValueType::I64,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    I32,
    I64,
}

#[derive(Debug, Clone)]
pub struct CallFrame {
    pub return_pc: usize,
    pub locals_start: usize,
    pub locals_count: usize,
}

#[derive(Debug, Clone)]
pub struct BlockFrame {
    pub start_pc: usize,
    pub end_pc: usize,
    pub stack_height: usize,
    pub is_loop: bool,
    pub result_type: Option<ValueType>,
}

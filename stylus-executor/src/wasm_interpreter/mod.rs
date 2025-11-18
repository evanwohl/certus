// Modular Wasm interpreter for on-chain fraud proof verification
// Architecture: VM core + opcode dispatch + state management

mod vm;
mod opcodes;
mod leb128;
mod types;
mod state;
mod validator;

pub use vm::Interpreter;
pub use types::{Value, ValueType, BlockFrame, CallFrame};
pub use state::StateHasher;
pub use validator::{BytecodeValidator, JumpTable};

pub const MAX_STACK_DEPTH: usize = 1024;
pub const MAX_CALL_DEPTH: usize = 256;
pub const MAX_BLOCK_DEPTH: usize = 256;
pub const MAX_MEMORY_PAGES: usize = 160;
pub const PAGE_SIZE: usize = 65536;

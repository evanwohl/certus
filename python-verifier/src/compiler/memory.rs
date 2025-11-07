// Deterministic heap memory management for Lists and Dicts
// All allocations use WASM linear memory with explicit layout

use wasm_encoder::*;

// Memory constants
pub const HEAP_PTR_GLOBAL: u32 = 1;  // Global index for heap pointer
pub const HEAP_LIMIT_GLOBAL: u32 = 2; // Global index for heap limit

// FNV-1a hash constants (deterministic, no seed)
const FNV_OFFSET_BASIS: i32 = 2166136261u32 as i32;
const FNV_PRIME: i32 = 16777619;

// List memory layout helpers
pub struct ListLayout;

impl ListLayout {
    /// Allocate list in heap: [length:i32][capacity:i32][elem0:i32][elem1:i32]...
    /// Returns: list_ptr on stack
    pub fn alloc(func: &mut Function, length: u32) {
        let size = 8 + (length * 4);

        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::I32Const(size as i32));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::GlobalGet(HEAP_LIMIT_GLOBAL));
        func.instruction(&Instruction::I32GtU);
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::Unreachable);
        func.instruction(&Instruction::End);

        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::I32Const(length as i32));
        func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));

        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::I32Const(length as i32));
        func.instruction(&Instruction::I32Store(MemArg { offset: 4, align: 2, memory_index: 0 }));

        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::I32Const(size as i32));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::GlobalSet(HEAP_PTR_GLOBAL));

        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::I32Const(size as i32));
        func.instruction(&Instruction::I32Sub);
    }

    /// Store element at index: list_ptr, index, value -> ()
    pub fn store_element(func: &mut Function, scratch0: u32) {
        // Stack: list_ptr, index, value
        func.instruction(&Instruction::LocalSet(scratch0)); // save value

        // Calculate offset: 8 + (index * 4)
        func.instruction(&Instruction::I32Const(4));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);

        // Add to list_ptr
        func.instruction(&Instruction::I32Add);

        // Store value
        func.instruction(&Instruction::LocalGet(scratch0));
        func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));
    }

    /// Load element at index: list_ptr, index -> value
    pub fn load_element(func: &mut Function, scratch0: u32, scratch1: u32) {
        // Stack: list_ptr, index
        func.instruction(&Instruction::LocalSet(scratch1));
        func.instruction(&Instruction::LocalSet(scratch0));

        func.instruction(&Instruction::LocalGet(scratch1));
        func.instruction(&Instruction::LocalGet(scratch0));
        func.instruction(&Instruction::I32Load(MemArg { offset: 0, align: 2, memory_index: 0 }));
        func.instruction(&Instruction::I32GeU);
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::Unreachable);
        func.instruction(&Instruction::End);

        func.instruction(&Instruction::LocalGet(scratch0));
        func.instruction(&Instruction::LocalGet(scratch1));
        func.instruction(&Instruction::I32Const(4));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Load(MemArg { offset: 0, align: 2, memory_index: 0 }));
    }
}

// Dict memory layout helpers
pub struct DictLayout;

impl DictLayout {
    /// Allocate dict with capacity: [capacity:i32][size:i32][tombstones:i32][reserved:i32]
    /// Returns: dict_ptr on stack
    pub fn alloc(func: &mut Function, capacity: u32, dict_ptr: u32, counter: u32) {
        let size = 16 + (capacity * 12);

        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::I32Const(size as i32));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::GlobalGet(HEAP_LIMIT_GLOBAL));
        func.instruction(&Instruction::I32GtU);
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::Unreachable);
        func.instruction(&Instruction::End);

        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::I32Const(capacity as i32));
        func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));

        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::I32Store(MemArg { offset: 4, align: 2, memory_index: 0 }));

        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::I32Store(MemArg { offset: 8, align: 2, memory_index: 0 }));

        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::LocalSet(dict_ptr));
        Self::init_slots(func, capacity, dict_ptr, counter);

        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::I32Const(size as i32));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::GlobalSet(HEAP_PTR_GLOBAL));

        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::I32Const(size as i32));
        func.instruction(&Instruction::I32Sub);
    }

    /// Initialize all hash slots to 0 (empty)
    fn init_slots(func: &mut Function, capacity: u32, dict_ptr: u32, counter: u32) {
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(counter));

        func.instruction(&Instruction::Block(BlockType::Empty));
        func.instruction(&Instruction::Loop(BlockType::Empty));

        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::I32Const(capacity as i32));
        func.instruction(&Instruction::I32GeU);
        func.instruction(&Instruction::BrIf(1));

        func.instruction(&Instruction::LocalGet(dict_ptr));
        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::I32Const(12));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::I32Const(16));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Add);

        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));

        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(counter));

        func.instruction(&Instruction::Br(0));
        func.instruction(&Instruction::End);
        func.instruction(&Instruction::End);
    }

    /// FNV-1a hash function (deterministic)
    pub fn fnv_hash(func: &mut Function, scratch: u32) {
        func.instruction(&Instruction::LocalSet(scratch));

        func.instruction(&Instruction::I32Const(FNV_OFFSET_BASIS));
        func.instruction(&Instruction::LocalGet(scratch));
        func.instruction(&Instruction::I32Xor);
        func.instruction(&Instruction::I32Const(FNV_PRIME));
        func.instruction(&Instruction::I32Mul);

        func.instruction(&Instruction::LocalTee(scratch));
        func.instruction(&Instruction::I32Eqz);
        func.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::Else);
        func.instruction(&Instruction::LocalGet(scratch));
        func.instruction(&Instruction::End);
    }

    /// Insert key-value pair using linear probing
    pub fn insert(func: &mut Function, dict_ptr: u32, key: u32, value: u32, hash: u32, capacity: u32, index: u32, slot_ptr: u32, slot_hash: u32) {
        func.instruction(&Instruction::LocalSet(value));
        func.instruction(&Instruction::LocalSet(key));
        func.instruction(&Instruction::LocalSet(dict_ptr));

        func.instruction(&Instruction::LocalGet(key));
        Self::fnv_hash(func, hash);
        func.instruction(&Instruction::LocalSet(hash));

        func.instruction(&Instruction::LocalGet(dict_ptr));
        func.instruction(&Instruction::I32Load(MemArg { offset: 0, align: 2, memory_index: 0 }));
        func.instruction(&Instruction::LocalSet(capacity));

        func.instruction(&Instruction::LocalGet(hash));
        func.instruction(&Instruction::LocalGet(capacity));
        func.instruction(&Instruction::I32RemU);
        func.instruction(&Instruction::LocalSet(index));

        Self::insert_loop(func, dict_ptr, key, value, hash, capacity, index, slot_ptr, slot_hash);
    }

    /// Linear probing loop for dict insertion
    fn insert_loop(func: &mut Function, dict_ptr: u32, key: u32, value: u32, hash: u32, capacity: u32, index: u32, slot_ptr: u32, slot_hash: u32) {
        func.instruction(&Instruction::Block(BlockType::Empty));
        func.instruction(&Instruction::Loop(BlockType::Empty));

        func.instruction(&Instruction::LocalGet(dict_ptr));
        func.instruction(&Instruction::LocalGet(index));
        func.instruction(&Instruction::I32Const(12));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::I32Const(16));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(slot_ptr));

        func.instruction(&Instruction::LocalGet(slot_ptr));
        func.instruction(&Instruction::I32Load(MemArg { offset: 0, align: 2, memory_index: 0 }));
        func.instruction(&Instruction::LocalTee(slot_hash));

        func.instruction(&Instruction::I32Eqz);
        func.instruction(&Instruction::If(BlockType::Empty));

        func.instruction(&Instruction::LocalGet(slot_ptr));
        func.instruction(&Instruction::LocalGet(hash));
        func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));

        func.instruction(&Instruction::LocalGet(slot_ptr));
        func.instruction(&Instruction::LocalGet(key));
        func.instruction(&Instruction::I32Store(MemArg { offset: 4, align: 2, memory_index: 0 }));

        func.instruction(&Instruction::LocalGet(slot_ptr));
        func.instruction(&Instruction::LocalGet(value));
        func.instruction(&Instruction::I32Store(MemArg { offset: 8, align: 2, memory_index: 0 }));

        func.instruction(&Instruction::LocalGet(dict_ptr));
        func.instruction(&Instruction::LocalGet(dict_ptr));
        func.instruction(&Instruction::I32Load(MemArg { offset: 4, align: 2, memory_index: 0 }));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Store(MemArg { offset: 4, align: 2, memory_index: 0 }));

        func.instruction(&Instruction::Br(2));
        func.instruction(&Instruction::End);

        func.instruction(&Instruction::LocalGet(slot_ptr));
        func.instruction(&Instruction::I32Load(MemArg { offset: 4, align: 2, memory_index: 0 }));
        func.instruction(&Instruction::LocalGet(key));
        func.instruction(&Instruction::I32Eq);
        func.instruction(&Instruction::If(BlockType::Empty));

        func.instruction(&Instruction::LocalGet(slot_ptr));
        func.instruction(&Instruction::LocalGet(value));
        func.instruction(&Instruction::I32Store(MemArg { offset: 8, align: 2, memory_index: 0 }));

        func.instruction(&Instruction::Br(2));
        func.instruction(&Instruction::End);

        func.instruction(&Instruction::LocalGet(index));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(capacity));
        func.instruction(&Instruction::I32RemU);
        func.instruction(&Instruction::LocalSet(index));

        func.instruction(&Instruction::Br(0));
        func.instruction(&Instruction::End);
        func.instruction(&Instruction::End);
    }

    /// Lookup key in dict: dict_ptr, key -> value (or 0 if not found)
    #[allow(dead_code)]
    pub fn lookup(func: &mut Function, dict_ptr: u32, key: u32, hash: u32, capacity: u32, index: u32, slot_ptr: u32) {
        func.instruction(&Instruction::LocalSet(key));
        func.instruction(&Instruction::LocalSet(dict_ptr));

        func.instruction(&Instruction::LocalGet(key));
        Self::fnv_hash(func, hash);
        func.instruction(&Instruction::LocalSet(hash));

        func.instruction(&Instruction::LocalGet(dict_ptr));
        func.instruction(&Instruction::I32Load(MemArg { offset: 0, align: 2, memory_index: 0 }));
        func.instruction(&Instruction::LocalSet(capacity));

        func.instruction(&Instruction::LocalGet(hash));
        func.instruction(&Instruction::LocalGet(capacity));
        func.instruction(&Instruction::I32RemU);
        func.instruction(&Instruction::LocalSet(index));

        func.instruction(&Instruction::Block(BlockType::Result(ValType::I32)));
        func.instruction(&Instruction::Loop(BlockType::Empty));

        func.instruction(&Instruction::LocalGet(dict_ptr));
        func.instruction(&Instruction::LocalGet(index));
        func.instruction(&Instruction::I32Const(12));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::I32Const(16));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(slot_ptr));

        func.instruction(&Instruction::LocalGet(slot_ptr));
        func.instruction(&Instruction::I32Load(MemArg { offset: 0, align: 2, memory_index: 0 }));

        func.instruction(&Instruction::I32Eqz);
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::Br(2));
        func.instruction(&Instruction::End);

        func.instruction(&Instruction::LocalGet(slot_ptr));
        func.instruction(&Instruction::I32Load(MemArg { offset: 4, align: 2, memory_index: 0 }));
        func.instruction(&Instruction::LocalGet(key));
        func.instruction(&Instruction::I32Eq);
        func.instruction(&Instruction::If(BlockType::Empty));

        func.instruction(&Instruction::LocalGet(slot_ptr));
        func.instruction(&Instruction::I32Load(MemArg { offset: 8, align: 2, memory_index: 0 }));
        func.instruction(&Instruction::Br(2));
        func.instruction(&Instruction::End);

        func.instruction(&Instruction::LocalGet(index));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(capacity));
        func.instruction(&Instruction::I32RemU);
        func.instruction(&Instruction::LocalSet(index));

        func.instruction(&Instruction::Br(0));
        func.instruction(&Instruction::End);
        func.instruction(&Instruction::End);
    }
}

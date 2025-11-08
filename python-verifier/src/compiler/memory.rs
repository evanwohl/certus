// Deterministic heap memory management for Lists and Dicts
// All allocations use WASM linear memory with explicit layout

use wasm_encoder::*;

// Memory constants
pub const HEAP_PTR_GLOBAL: u32 = 1;  // Global index for heap pointer
pub const HEAP_LIMIT_GLOBAL: u32 = 2; // Global index for heap limit

// Type tags for runtime discrimination
const TYPE_LIST: i32 = 1;
const TYPE_DICT: i32 = 2;
const TYPE_STRING: i32 = 3;
const TYPE_BYTES: i32 = 4;

// FNV-1a hash constants (deterministic, no seed)
const FNV_OFFSET_BASIS: i32 = 2166136261u32 as i32;
const FNV_PRIME: i32 = 16777619;

// List memory layout helpers
pub struct ListLayout;

impl ListLayout {
    /// Allocate list in heap: [type:i32][length:i32][capacity:i32][elem0:i32][elem1:i32]...
    /// Returns: list_ptr on stack
    pub fn alloc(func: &mut Function, length: u32) {
        let size = 12 + (length * 4);

        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::I32Const(size as i32));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::GlobalGet(HEAP_LIMIT_GLOBAL));
        func.instruction(&Instruction::I32GtU);
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::Unreachable);
        func.instruction(&Instruction::End);

        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::I32Const(TYPE_LIST));
        func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));

        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::I32Const(length as i32));
        func.instruction(&Instruction::I32Store(MemArg { offset: 4, align: 2, memory_index: 0 }));

        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::I32Const(length as i32));
        func.instruction(&Instruction::I32Store(MemArg { offset: 8, align: 2, memory_index: 0 }));

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
        func.instruction(&Instruction::LocalSet(scratch0));

        // Calculate offset: 12 + (index * 4)
        func.instruction(&Instruction::I32Const(4));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::I32Const(12));
        func.instruction(&Instruction::I32Add);

        func.instruction(&Instruction::I32Add);

        func.instruction(&Instruction::LocalGet(scratch0));
        func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));
    }

    /// Load element at index: list_ptr, index -> value
    pub fn load_element(func: &mut Function, scratch0: u32, scratch1: u32) {
        // Stack: list_ptr, index
        func.instruction(&Instruction::LocalSet(scratch1));
        func.instruction(&Instruction::LocalSet(scratch0));

        // bounds check: index < length (at offset 4)
        func.instruction(&Instruction::LocalGet(scratch1));
        func.instruction(&Instruction::LocalGet(scratch0));
        func.instruction(&Instruction::I32Load(MemArg { offset: 4, align: 2, memory_index: 0 }));
        func.instruction(&Instruction::I32GeU);
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::Unreachable);
        func.instruction(&Instruction::End);

        // compute address: list_ptr + 12 + (index * 4)
        func.instruction(&Instruction::LocalGet(scratch0));
        func.instruction(&Instruction::LocalGet(scratch1));
        func.instruction(&Instruction::I32Const(4));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::I32Const(12));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Load(MemArg { offset: 0, align: 2, memory_index: 0 }));
    }

    /// Update element at index with bounds check: list_ptr, index, value -> ()
    pub fn update_element(func: &mut Function, list_ptr: u32, index: u32, value: u32) {
        // Stack: list_ptr, index, value
        func.instruction(&Instruction::LocalSet(value));
        func.instruction(&Instruction::LocalSet(index));
        func.instruction(&Instruction::LocalSet(list_ptr));

        // bounds check: index < length (at offset 4)
        func.instruction(&Instruction::LocalGet(index));
        func.instruction(&Instruction::LocalGet(list_ptr));
        func.instruction(&Instruction::I32Load(MemArg { offset: 4, align: 2, memory_index: 0 }));
        func.instruction(&Instruction::I32GeU);
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::Unreachable);
        func.instruction(&Instruction::End);

        // compute address: list_ptr + 12 + (index * 4)
        func.instruction(&Instruction::LocalGet(list_ptr));
        func.instruction(&Instruction::LocalGet(index));
        func.instruction(&Instruction::I32Const(4));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::I32Const(12));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Add);

        func.instruction(&Instruction::LocalGet(value));
        func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));
    }
}

// Dict memory layout helpers
pub struct DictLayout;

impl DictLayout {
    /// Allocate dict: [type:i32][capacity:i32][size:i32][tombstones:i32][slots...]
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
        func.instruction(&Instruction::I32Const(TYPE_DICT));
        func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));

        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::I32Const(capacity as i32));
        func.instruction(&Instruction::I32Store(MemArg { offset: 4, align: 2, memory_index: 0 }));

        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::I32Store(MemArg { offset: 8, align: 2, memory_index: 0 }));

        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::I32Store(MemArg { offset: 12, align: 2, memory_index: 0 }));

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
        func.instruction(&Instruction::I32Load(MemArg { offset: 4, align: 2, memory_index: 0 }));
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
        func.instruction(&Instruction::I32Load(MemArg { offset: 8, align: 2, memory_index: 0 }));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Store(MemArg { offset: 8, align: 2, memory_index: 0 }));

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
        func.instruction(&Instruction::I32Load(MemArg { offset: 4, align: 2, memory_index: 0 }));
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

// String memory layout helpers
pub struct StringLayout;

impl StringLayout {
    /// Allocate string in heap: [type:i32][length:i32][bytes...]
    /// Returns: str_ptr on stack
    pub fn alloc(func: &mut Function, bytes: &[u8]) {
        let length = bytes.len() as i32;
        let size = 8 + bytes.len();
        let aligned_size = (size + 3) & !3;

        // Check heap overflow
        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::I32Const(aligned_size as i32));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::GlobalGet(HEAP_LIMIT_GLOBAL));
        func.instruction(&Instruction::I32GtU);
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::Unreachable);
        func.instruction(&Instruction::End);

        // Store type tag
        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::I32Const(TYPE_STRING));
        func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));

        // Store length
        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::I32Const(length));
        func.instruction(&Instruction::I32Store(MemArg { offset: 4, align: 2, memory_index: 0 }));

        // Return string pointer before updating heap ptr
        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));

        // Update heap pointer
        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::I32Const(aligned_size as i32));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::GlobalSet(HEAP_PTR_GLOBAL));
    }

    /// Load string length: str_ptr -> length
    pub fn load_length(func: &mut Function) {
        func.instruction(&Instruction::I32Load(MemArg { offset: 4, align: 2, memory_index: 0 }));
    }

    /// Create substring: str_ptr, start, end -> new_str_ptr
    pub fn slice(func: &mut Function, src: u32, start: u32, end: u32, len: u32, new_ptr: u32, counter: u32) {
        func.instruction(&Instruction::LocalSet(end));
        func.instruction(&Instruction::LocalSet(start));
        func.instruction(&Instruction::LocalSet(src));

        // Get string length
        func.instruction(&Instruction::LocalGet(src));
        Self::load_length(func);
        func.instruction(&Instruction::LocalSet(len));

        // Clamp start to [0, len]
        func.instruction(&Instruction::LocalGet(start));
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::I32LtS);
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(start));
        func.instruction(&Instruction::End);

        func.instruction(&Instruction::LocalGet(start));
        func.instruction(&Instruction::LocalGet(len));
        func.instruction(&Instruction::I32GtS);
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::LocalGet(len));
        func.instruction(&Instruction::LocalSet(start));
        func.instruction(&Instruction::End);

        // Clamp end to [start, len]
        func.instruction(&Instruction::LocalGet(end));
        func.instruction(&Instruction::LocalGet(start));
        func.instruction(&Instruction::I32LtS);
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::LocalGet(start));
        func.instruction(&Instruction::LocalSet(end));
        func.instruction(&Instruction::End);

        func.instruction(&Instruction::LocalGet(end));
        func.instruction(&Instruction::LocalGet(len));
        func.instruction(&Instruction::I32GtS);
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::LocalGet(len));
        func.instruction(&Instruction::LocalSet(end));
        func.instruction(&Instruction::End);

        // Calculate slice length
        func.instruction(&Instruction::LocalGet(end));
        func.instruction(&Instruction::LocalGet(start));
        func.instruction(&Instruction::I32Sub);
        func.instruction(&Instruction::LocalSet(len));

        // Allocate new string
        let aligned_size_expr = |f: &mut Function| {
            f.instruction(&Instruction::LocalGet(len));
            f.instruction(&Instruction::I32Const(11));
            f.instruction(&Instruction::I32Add);
            f.instruction(&Instruction::I32Const(-4));
            f.instruction(&Instruction::I32And);
        };

        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        aligned_size_expr(func);
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::GlobalGet(HEAP_LIMIT_GLOBAL));
        func.instruction(&Instruction::I32GtU);
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::Unreachable);
        func.instruction(&Instruction::End);

        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::LocalTee(new_ptr));
        func.instruction(&Instruction::I32Const(TYPE_STRING));
        func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));

        func.instruction(&Instruction::LocalGet(new_ptr));
        func.instruction(&Instruction::LocalGet(len));
        func.instruction(&Instruction::I32Store(MemArg { offset: 4, align: 2, memory_index: 0 }));

        // Copy bytes
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(counter));

        func.instruction(&Instruction::Block(BlockType::Empty));
        func.instruction(&Instruction::Loop(BlockType::Empty));

        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::LocalGet(len));
        func.instruction(&Instruction::I32GeU);
        func.instruction(&Instruction::BrIf(1));

        func.instruction(&Instruction::LocalGet(new_ptr));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::I32Add);

        func.instruction(&Instruction::LocalGet(src));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(start));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Load8U(MemArg { offset: 0, align: 0, memory_index: 0 }));

        func.instruction(&Instruction::I32Store8(MemArg { offset: 0, align: 0, memory_index: 0 }));

        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(counter));

        func.instruction(&Instruction::Br(0));
        func.instruction(&Instruction::End);
        func.instruction(&Instruction::End);

        // Update heap pointer
        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        aligned_size_expr(func);
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::GlobalSet(HEAP_PTR_GLOBAL));

        // Return new string pointer
        func.instruction(&Instruction::LocalGet(new_ptr));
    }

    /// Concatenate two strings: str1_ptr, str2_ptr -> new_str_ptr
    /// Pops [str2, str1] from stack, pushes new_str_ptr
    pub fn concat(func: &mut Function, str1: u32, str2: u32, len1: u32, len2: u32, new_ptr: u32, slice_len: u32, counter: u32) {
        func.instruction(&Instruction::LocalSet(str2));
        func.instruction(&Instruction::LocalSet(str1));

        // Load str1 length
        func.instruction(&Instruction::LocalGet(str1));
        Self::load_length(func);
        func.instruction(&Instruction::LocalSet(len1));

        // Load str2 length
        func.instruction(&Instruction::LocalGet(str2));
        Self::load_length(func);
        func.instruction(&Instruction::LocalSet(len2));

        // Calculate total length
        func.instruction(&Instruction::LocalGet(len1));
        func.instruction(&Instruction::LocalGet(len2));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(slice_len));

        // Heap overflow check
        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::LocalGet(slice_len));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Const(3));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Const(-4));
        func.instruction(&Instruction::I32And);
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::GlobalGet(HEAP_LIMIT_GLOBAL));
        func.instruction(&Instruction::I32GtU);
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::Unreachable);
        func.instruction(&Instruction::End);

        // Allocate new string header
        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::LocalTee(new_ptr));
        func.instruction(&Instruction::I32Const(TYPE_STRING));
        func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));

        func.instruction(&Instruction::LocalGet(new_ptr));
        func.instruction(&Instruction::LocalGet(slice_len));
        func.instruction(&Instruction::I32Store(MemArg { offset: 4, align: 2, memory_index: 0 }));

        // Copy str1 bytes: loop counter = 0 to len1
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(counter));

        func.instruction(&Instruction::Block(BlockType::Empty));
        func.instruction(&Instruction::Loop(BlockType::Empty));
        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::LocalGet(len1));
        func.instruction(&Instruction::I32GeU);
        func.instruction(&Instruction::BrIf(1));

        func.instruction(&Instruction::LocalGet(new_ptr));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(str1));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Load8U(MemArg { offset: 0, align: 0, memory_index: 0 }));
        func.instruction(&Instruction::I32Store8(MemArg { offset: 0, align: 0, memory_index: 0 }));

        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(counter));
        func.instruction(&Instruction::Br(0));
        func.instruction(&Instruction::End);
        func.instruction(&Instruction::End);

        // Copy str2 bytes: loop counter = 0 to len2
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(counter));

        func.instruction(&Instruction::Block(BlockType::Empty));
        func.instruction(&Instruction::Loop(BlockType::Empty));
        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::LocalGet(len2));
        func.instruction(&Instruction::I32GeU);
        func.instruction(&Instruction::BrIf(1));

        func.instruction(&Instruction::LocalGet(new_ptr));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(len1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(str2));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Load8U(MemArg { offset: 0, align: 0, memory_index: 0 }));
        func.instruction(&Instruction::I32Store8(MemArg { offset: 0, align: 0, memory_index: 0 }));

        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(counter));
        func.instruction(&Instruction::Br(0));
        func.instruction(&Instruction::End);
        func.instruction(&Instruction::End);

        // Update heap pointer with aligned size
        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::LocalGet(slice_len));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Const(3));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Const(-4));
        func.instruction(&Instruction::I32And);
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::GlobalSet(HEAP_PTR_GLOBAL));

        // Return new string pointer
        func.instruction(&Instruction::LocalGet(new_ptr));
    }

    /// String equality: str1_ptr, str2_ptr -> bool (1 or 0)
    /// Pops [str2, str1] from stack, pushes 1 if equal, 0 otherwise
    pub fn equals(func: &mut Function, str1: u32, str2: u32, len1: u32, len2: u32, counter: u32) {
        func.instruction(&Instruction::LocalSet(str2));
        func.instruction(&Instruction::LocalSet(str1));

        // Load lengths
        func.instruction(&Instruction::LocalGet(str1));
        Self::load_length(func);
        func.instruction(&Instruction::LocalSet(len1));

        func.instruction(&Instruction::LocalGet(str2));
        Self::load_length(func);
        func.instruction(&Instruction::LocalSet(len2));

        // Fast path: if lengths differ, return 0
        func.instruction(&Instruction::LocalGet(len1));
        func.instruction(&Instruction::LocalGet(len2));
        func.instruction(&Instruction::I32Ne);
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::Return);
        func.instruction(&Instruction::End);

        // Compare bytes: loop counter = 0 to len1
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(counter));

        func.instruction(&Instruction::Block(BlockType::Empty));
        func.instruction(&Instruction::Loop(BlockType::Empty));
        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::LocalGet(len1));
        func.instruction(&Instruction::I32GeU);
        func.instruction(&Instruction::BrIf(1));

        // Load byte from str1
        func.instruction(&Instruction::LocalGet(str1));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Load8U(MemArg { offset: 0, align: 0, memory_index: 0 }));

        // Load byte from str2
        func.instruction(&Instruction::LocalGet(str2));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Load8U(MemArg { offset: 0, align: 0, memory_index: 0 }));

        // If bytes differ, return 0
        func.instruction(&Instruction::I32Ne);
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::Return);
        func.instruction(&Instruction::End);

        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(counter));
        func.instruction(&Instruction::Br(0));
        func.instruction(&Instruction::End);
        func.instruction(&Instruction::End);

        // All bytes equal, return 1
        func.instruction(&Instruction::I32Const(1));
    }

    /// String indexing: str_ptr, index -> byte value (as i32)
    /// Pops [index, str_ptr] from stack, pushes byte value
    /// Traps if index out of bounds
    pub fn index(func: &mut Function, str_ptr: u32, index: u32) {
        func.instruction(&Instruction::LocalSet(index));
        func.instruction(&Instruction::LocalSet(str_ptr));

        // Bounds check: index < length
        func.instruction(&Instruction::LocalGet(index));
        func.instruction(&Instruction::LocalGet(str_ptr));
        Self::load_length(func);
        func.instruction(&Instruction::I32GeU);
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::Unreachable);
        func.instruction(&Instruction::End);

        // Load byte at str_ptr + 8 + index
        func.instruction(&Instruction::LocalGet(str_ptr));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(index));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Load8U(MemArg { offset: 0, align: 0, memory_index: 0 }));
    }

    /// Check if string starts with prefix
    /// Pops [str_ptr, prefix_ptr], pushes 1 if true, 0 if false
    /// Uses byte_local as a temporary result holder (we use counter for the result, byte_local for bytes)
    pub fn startswith(func: &mut Function, str_ptr: u32, prefix_ptr: u32, str_len: u32, prefix_len: u32, counter: u32, result: u32) {
        func.instruction(&Instruction::LocalSet(prefix_ptr));
        func.instruction(&Instruction::LocalSet(str_ptr));

        // Load string length
        func.instruction(&Instruction::LocalGet(str_ptr));
        Self::load_length(func);
        func.instruction(&Instruction::LocalSet(str_len));

        // Load prefix length
        func.instruction(&Instruction::LocalGet(prefix_ptr));
        Self::load_length(func);
        func.instruction(&Instruction::LocalSet(prefix_len));

        // If prefix longer than string, result = 0, otherwise check bytes
        func.instruction(&Instruction::LocalGet(prefix_len));
        func.instruction(&Instruction::LocalGet(str_len));
        func.instruction(&Instruction::I32GtU);
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(result));
        func.instruction(&Instruction::Else);

        // Assume match initially (handles empty prefix case)
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::LocalSet(result));

        // Compare prefix_len bytes
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(counter));

        // Loop to check all bytes
        func.instruction(&Instruction::Block(BlockType::Empty));
        func.instruction(&Instruction::Loop(BlockType::Empty));

        // If counter == prefix_len, all bytes matched, break
        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::LocalGet(prefix_len));
        func.instruction(&Instruction::I32GeU);
        func.instruction(&Instruction::BrIf(1)); // Break out of loop

        // Load and compare bytes
        func.instruction(&Instruction::LocalGet(str_ptr));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Load8U(MemArg { offset: 0, align: 0, memory_index: 0 }));

        func.instruction(&Instruction::LocalGet(prefix_ptr));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Load8U(MemArg { offset: 0, align: 0, memory_index: 0 }));

        // Compare bytes - if not equal, set result = 0 and break
        func.instruction(&Instruction::I32Ne);
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(result));
        func.instruction(&Instruction::Br(2)); // Break out of loop and block
        func.instruction(&Instruction::End);

        // counter++
        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(counter));

        func.instruction(&Instruction::Br(0)); // Continue loop
        func.instruction(&Instruction::End); // End of loop
        func.instruction(&Instruction::End); // End of block

        func.instruction(&Instruction::End); // End of if/else

        // Push result onto stack
        func.instruction(&Instruction::LocalGet(result));
    }

    /// Convert integer to string
    /// Pops [int_value], pushes string_ptr
    pub fn from_int(func: &mut Function, int_val: u32, is_neg: u32, abs_val: u32, digit_count: u32, len: u32, new_ptr: u32, write_pos: u32, digit: u32) {
        func.instruction(&Instruction::LocalSet(int_val));

        // Check if negative
        func.instruction(&Instruction::LocalGet(int_val));
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::I32LtS);
        func.instruction(&Instruction::LocalSet(is_neg));

        // Get absolute value
        func.instruction(&Instruction::LocalGet(int_val));
        func.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        func.instruction(&Instruction::LocalGet(is_neg));
        func.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        func.instruction(&Instruction::LocalGet(int_val));
        func.instruction(&Instruction::I32Const(-1));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::Else);
        func.instruction(&Instruction::LocalGet(int_val));
        func.instruction(&Instruction::End);
        func.instruction(&Instruction::Else);
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::End);
        func.instruction(&Instruction::LocalSet(abs_val));

        // Count digits
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(digit_count));

        func.instruction(&Instruction::LocalGet(abs_val));
        func.instruction(&Instruction::LocalSet(digit));

        // Loop to count digits (do-while: runs at least once)
        func.instruction(&Instruction::Block(BlockType::Empty));
        func.instruction(&Instruction::Loop(BlockType::Empty));

        // digit_count++
        func.instruction(&Instruction::LocalGet(digit_count));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(digit_count));

        // digit /= 10
        func.instruction(&Instruction::LocalGet(digit));
        func.instruction(&Instruction::I32Const(10));
        func.instruction(&Instruction::I32DivU);
        func.instruction(&Instruction::LocalSet(digit));

        // if digit > 0, continue loop (br 0 = loop again)
        func.instruction(&Instruction::LocalGet(digit));
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::I32GtU);
        func.instruction(&Instruction::BrIf(0));

        // Exit loop (br 1 = break out of block)
        func.instruction(&Instruction::Br(1));
        func.instruction(&Instruction::End);
        func.instruction(&Instruction::End);

        // Calculate total length: digit_count + (is_neg ? 1 : 0)
        func.instruction(&Instruction::LocalGet(digit_count));
        func.instruction(&Instruction::LocalGet(is_neg));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(len));

        // Allocate string
        let size_expr = 8; // header
        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::LocalSet(new_ptr));

        // Heap overflow check
        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::LocalGet(len));
        func.instruction(&Instruction::I32Const(size_expr));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Const(3));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Const(-4));
        func.instruction(&Instruction::I32And);
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::GlobalGet(HEAP_LIMIT_GLOBAL));
        func.instruction(&Instruction::I32GtU);
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::Unreachable);
        func.instruction(&Instruction::End);

        // Store type tag
        func.instruction(&Instruction::LocalGet(new_ptr));
        func.instruction(&Instruction::I32Const(TYPE_STRING));
        func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));

        // Store length
        func.instruction(&Instruction::LocalGet(new_ptr));
        func.instruction(&Instruction::LocalGet(len));
        func.instruction(&Instruction::I32Store(MemArg { offset: 4, align: 2, memory_index: 0 }));

        // Write position starts at end of string
        func.instruction(&Instruction::LocalGet(len));
        func.instruction(&Instruction::LocalSet(write_pos));

        // Reset abs_val for digit extraction
        func.instruction(&Instruction::LocalGet(int_val));
        func.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        func.instruction(&Instruction::LocalGet(is_neg));
        func.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        func.instruction(&Instruction::LocalGet(int_val));
        func.instruction(&Instruction::I32Const(-1));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::Else);
        func.instruction(&Instruction::LocalGet(int_val));
        func.instruction(&Instruction::End);
        func.instruction(&Instruction::Else);
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::End);
        func.instruction(&Instruction::LocalSet(abs_val));

        // Write digits right-to-left (do-while: runs at least once)
        func.instruction(&Instruction::Block(BlockType::Empty));
        func.instruction(&Instruction::Loop(BlockType::Empty));

        // write_pos--
        func.instruction(&Instruction::LocalGet(write_pos));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Sub);
        func.instruction(&Instruction::LocalSet(write_pos));

        // digit = abs_val % 10
        func.instruction(&Instruction::LocalGet(abs_val));
        func.instruction(&Instruction::I32Const(10));
        func.instruction(&Instruction::I32RemU);
        func.instruction(&Instruction::LocalSet(digit));

        // Store ASCII digit '0' + digit
        func.instruction(&Instruction::LocalGet(new_ptr));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(write_pos));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(digit));
        func.instruction(&Instruction::I32Const(48)); // ASCII '0'
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Store8(MemArg { offset: 0, align: 0, memory_index: 0 }));

        // abs_val /= 10
        func.instruction(&Instruction::LocalGet(abs_val));
        func.instruction(&Instruction::I32Const(10));
        func.instruction(&Instruction::I32DivU);
        func.instruction(&Instruction::LocalSet(abs_val));

        // if abs_val > 0, continue loop (br 0 = loop again)
        func.instruction(&Instruction::LocalGet(abs_val));
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::I32GtU);
        func.instruction(&Instruction::BrIf(0));

        // Exit loop (br 1 = break out of block)
        func.instruction(&Instruction::Br(1));
        func.instruction(&Instruction::End);
        func.instruction(&Instruction::End);

        // If negative, write '-' at position 0
        func.instruction(&Instruction::LocalGet(is_neg));
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::LocalGet(new_ptr));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Const(45)); // ASCII '-'
        func.instruction(&Instruction::I32Store8(MemArg { offset: 0, align: 0, memory_index: 0 }));
        func.instruction(&Instruction::End);

        // Update heap pointer
        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::LocalGet(len));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Const(3));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Const(-4));
        func.instruction(&Instruction::I32And);
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::GlobalSet(HEAP_PTR_GLOBAL));

        // Return string pointer
        func.instruction(&Instruction::LocalGet(new_ptr));
    }
}

// Bytes layout: identical to strings but with TYPE_BYTES tag
// Memory layout: [type:i32=4][length:i32][bytes...]
pub struct BytesLayout;

impl BytesLayout {
    /// Convert string to bytes (UTF-8 encoding)
    /// Pops [str_ptr], pushes bytes_ptr
    pub fn from_string(func: &mut Function, str_local: u32, len_local: u32, counter: u32, new_ptr: u32) {
        func.instruction(&Instruction::LocalSet(str_local));

        // Load string length
        func.instruction(&Instruction::LocalGet(str_local));
        func.instruction(&Instruction::I32Load(MemArg { offset: 4, align: 2, memory_index: 0 }));
        func.instruction(&Instruction::LocalSet(len_local));

        // Calculate aligned size
        func.instruction(&Instruction::LocalGet(len_local));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Const(3));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Const(-4));
        func.instruction(&Instruction::I32And);
        func.instruction(&Instruction::LocalTee(counter));

        // Heap overflow check
        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::GlobalGet(HEAP_LIMIT_GLOBAL));
        func.instruction(&Instruction::I32GtU);
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::Unreachable);
        func.instruction(&Instruction::End);

        // Allocate bytes header
        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::LocalTee(new_ptr));
        func.instruction(&Instruction::I32Const(TYPE_BYTES));
        func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));

        func.instruction(&Instruction::LocalGet(new_ptr));
        func.instruction(&Instruction::LocalGet(len_local));
        func.instruction(&Instruction::I32Store(MemArg { offset: 4, align: 2, memory_index: 0 }));

        // Copy bytes from string
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(counter));

        func.instruction(&Instruction::Block(BlockType::Empty));
        func.instruction(&Instruction::Loop(BlockType::Empty));
        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::LocalGet(len_local));
        func.instruction(&Instruction::I32GeU);
        func.instruction(&Instruction::BrIf(1));

        func.instruction(&Instruction::LocalGet(new_ptr));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(str_local));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Load8U(MemArg { offset: 0, align: 0, memory_index: 0 }));
        func.instruction(&Instruction::I32Store8(MemArg { offset: 0, align: 0, memory_index: 0 }));

        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(counter));
        func.instruction(&Instruction::Br(0));
        func.instruction(&Instruction::End);
        func.instruction(&Instruction::End);

        // Update heap pointer
        func.instruction(&Instruction::LocalGet(len_local));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Const(3));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Const(-4));
        func.instruction(&Instruction::I32And);
        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::GlobalSet(HEAP_PTR_GLOBAL));

        // Return bytes pointer
        func.instruction(&Instruction::LocalGet(new_ptr));
    }

    /// Convert bytes to hexadecimal string
    /// Pops [bytes_ptr], pushes string_ptr
    pub fn hexdigest(func: &mut Function, bytes_ptr: u32, len: u32, new_ptr: u32, counter: u32) {
        let byte_val = counter + 1;
        let scratch = counter + 2;
        func.instruction(&Instruction::LocalSet(bytes_ptr));

        // Load bytes length
        func.instruction(&Instruction::LocalGet(bytes_ptr));
        func.instruction(&Instruction::I32Load(MemArg { offset: 4, align: 2, memory_index: 0 }));
        func.instruction(&Instruction::LocalSet(len));

        // Allocate string with length = 2 * bytes_length (each byte becomes 2 hex chars)
        func.instruction(&Instruction::LocalGet(len));
        func.instruction(&Instruction::I32Const(2));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::LocalSet(len));

        // Heap overflow check
        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::LocalSet(new_ptr));

        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::LocalGet(len));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Const(3));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Const(-4));
        func.instruction(&Instruction::I32And);
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::GlobalGet(HEAP_LIMIT_GLOBAL));
        func.instruction(&Instruction::I32GtU);
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::Unreachable);
        func.instruction(&Instruction::End);

        // Store type tag (TYPE_STRING)
        func.instruction(&Instruction::LocalGet(new_ptr));
        func.instruction(&Instruction::I32Const(TYPE_STRING));
        func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));

        // Store length
        func.instruction(&Instruction::LocalGet(new_ptr));
        func.instruction(&Instruction::LocalGet(len));
        func.instruction(&Instruction::I32Store(MemArg { offset: 4, align: 2, memory_index: 0 }));

        // Convert each byte to 2 hex chars
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalSet(counter));

        func.instruction(&Instruction::Block(BlockType::Empty));
        func.instruction(&Instruction::Loop(BlockType::Empty));

        // If counter == len/2, done
        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::LocalGet(len));
        func.instruction(&Instruction::I32Const(2));
        func.instruction(&Instruction::I32DivU);
        func.instruction(&Instruction::I32GeU);
        func.instruction(&Instruction::BrIf(1));

        // Load byte from bytes[counter]
        func.instruction(&Instruction::LocalGet(bytes_ptr));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Load8U(MemArg { offset: 0, align: 0, memory_index: 0 }));
        func.instruction(&Instruction::LocalSet(byte_val));

        // High nibble: (byte >> 4) & 0xF
        func.instruction(&Instruction::LocalGet(byte_val));
        func.instruction(&Instruction::I32Const(4));
        func.instruction(&Instruction::I32ShrU);
        func.instruction(&Instruction::I32Const(0xF));
        func.instruction(&Instruction::I32And);
        func.instruction(&Instruction::LocalSet(scratch));

        // Convert to hex char: if < 10, '0' + n, else 'a' + (n - 10)
        func.instruction(&Instruction::LocalGet(scratch));
        func.instruction(&Instruction::I32Const(10));
        func.instruction(&Instruction::I32LtU);
        func.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        func.instruction(&Instruction::LocalGet(scratch));
        func.instruction(&Instruction::I32Const(48)); // '0'
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::Else);
        func.instruction(&Instruction::LocalGet(scratch));
        func.instruction(&Instruction::I32Const(10));
        func.instruction(&Instruction::I32Sub);
        func.instruction(&Instruction::I32Const(97)); // 'a'
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::End);

        // Store high nibble char
        func.instruction(&Instruction::LocalSet(scratch));
        func.instruction(&Instruction::LocalGet(new_ptr));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::I32Const(2));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(scratch));
        func.instruction(&Instruction::I32Store8(MemArg { offset: 0, align: 0, memory_index: 0 }));

        // Low nibble: byte & 0xF
        func.instruction(&Instruction::LocalGet(byte_val));
        func.instruction(&Instruction::I32Const(0xF));
        func.instruction(&Instruction::I32And);
        func.instruction(&Instruction::LocalSet(scratch));

        // Convert to hex char
        func.instruction(&Instruction::LocalGet(scratch));
        func.instruction(&Instruction::I32Const(10));
        func.instruction(&Instruction::I32LtU);
        func.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        func.instruction(&Instruction::LocalGet(scratch));
        func.instruction(&Instruction::I32Const(48)); // '0'
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::Else);
        func.instruction(&Instruction::LocalGet(scratch));
        func.instruction(&Instruction::I32Const(10));
        func.instruction(&Instruction::I32Sub);
        func.instruction(&Instruction::I32Const(97)); // 'a'
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::End);

        // Store low nibble char
        func.instruction(&Instruction::LocalSet(scratch));
        func.instruction(&Instruction::LocalGet(new_ptr));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::I32Const(2));
        func.instruction(&Instruction::I32Mul);
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(scratch));
        func.instruction(&Instruction::I32Store8(MemArg { offset: 0, align: 0, memory_index: 0 }));

        // counter++
        func.instruction(&Instruction::LocalGet(counter));
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(counter));

        func.instruction(&Instruction::Br(0));
        func.instruction(&Instruction::End);
        func.instruction(&Instruction::End);

        // Update heap pointer
        func.instruction(&Instruction::LocalGet(len));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Const(3));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Const(-4));
        func.instruction(&Instruction::I32And);
        func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::GlobalSet(HEAP_PTR_GLOBAL));

        // Return string pointer
        func.instruction(&Instruction::LocalGet(new_ptr));
    }
}

/// SHA256 hash function - full FIPS 180-4 implementation
/// Pops [bytes_ptr], pushes bytes_ptr (32-byte hash)
/// Implements complete SHA-256 with padding, message schedule, and compression
pub fn sha256(func: &mut Function, base: u32) {
    // SHA-256 constants (first 32 bits of fractional parts of cube roots of first 64 primes)
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];

    // Initial hash values (first 32 bits of fractional parts of square roots of first 8 primes)
    const H0_INIT: u32 = 0x6a09e667;
    const H1_INIT: u32 = 0xbb67ae85;
    const H2_INIT: u32 = 0x3c6ef372;
    const H3_INIT: u32 = 0xa54ff53a;
    const H4_INIT: u32 = 0x510e527f;
    const H5_INIT: u32 = 0x9b05688c;
    const H6_INIT: u32 = 0x1f83d9ab;
    const H7_INIT: u32 = 0x5be0cd19;

    // Local variable allocation
    let bytes_ptr = base;
    let data_len = base + 1;
    let bit_len_lo = base + 2;
    let bit_len_hi = base + 3;
    let padded_len = base + 4;
    let padded_ptr = base + 5;
    let block_offset = base + 6;
    let round_idx = base + 7;

    // Hash state (h0-h7)
    let h = [base + 8, base + 9, base + 10, base + 11, base + 12, base + 13, base + 14, base + 15];

    // Working variables (a-h)
    let work = [base + 16, base + 17, base + 18, base + 19, base + 20, base + 21, base + 22, base + 23];

    // Message schedule (w[0..64])
    let w_start = base + 24;

    // Temp variables for SHA-256 operations
    let temp1 = base + 88;
    let temp2 = base + 89;
    let s0 = base + 90;
    let s1 = base + 91;
    let ch = base + 92;
    let maj = base + 93;
    let k_val = base + 94;
    let w_val = base + 95;
    let new_ptr = base + 96;

    // Store input pointer
    func.instruction(&Instruction::LocalSet(bytes_ptr));

    // Load data length
    func.instruction(&Instruction::LocalGet(bytes_ptr));
    func.instruction(&Instruction::I32Load(MemArg { offset: 4, align: 2, memory_index: 0 }));
    func.instruction(&Instruction::LocalSet(data_len));

    // Calculate bit length (data_len * 8) as 64-bit value
    func.instruction(&Instruction::LocalGet(data_len));
    func.instruction(&Instruction::I32Const(8));
    func.instruction(&Instruction::I32Mul);
    func.instruction(&Instruction::LocalSet(bit_len_lo));
    func.instruction(&Instruction::I32Const(0));
    func.instruction(&Instruction::LocalSet(bit_len_hi));

    // Calculate padded length: ((data_len + 1 + 8 + 63) / 64) * 64
    func.instruction(&Instruction::LocalGet(data_len));
    func.instruction(&Instruction::I32Const(1 + 8 + 63));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::I32Const(64));
    func.instruction(&Instruction::I32DivU);
    func.instruction(&Instruction::I32Const(64));
    func.instruction(&Instruction::I32Mul);
    func.instruction(&Instruction::LocalSet(padded_len));

    // Allocate padded buffer on heap
    func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
    func.instruction(&Instruction::LocalSet(padded_ptr));

    // Check heap overflow
    func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
    func.instruction(&Instruction::LocalGet(padded_len));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::GlobalGet(HEAP_LIMIT_GLOBAL));
    func.instruction(&Instruction::I32GtU);
    func.instruction(&Instruction::If(BlockType::Empty));
    func.instruction(&Instruction::Unreachable);
    func.instruction(&Instruction::End);

    // Copy original data to padded buffer using memory.copy
    func.instruction(&Instruction::LocalGet(padded_ptr));
    func.instruction(&Instruction::LocalGet(bytes_ptr));
    func.instruction(&Instruction::I32Const(8));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::LocalGet(data_len));
    func.instruction(&Instruction::MemoryCopy { src_mem: 0, dst_mem: 0 });

    // Append 0x80 byte after data
    func.instruction(&Instruction::LocalGet(padded_ptr));
    func.instruction(&Instruction::LocalGet(data_len));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::I32Const(0x80));
    func.instruction(&Instruction::I32Store8(MemArg { offset: 0, align: 0, memory_index: 0 }));

    // Zero out padding bytes
    // Calculate zero_start = padded_ptr + data_len + 1
    // Calculate zero_len = padded_len - data_len - 1 - 8
    func.instruction(&Instruction::LocalGet(padded_ptr));
    func.instruction(&Instruction::LocalGet(data_len));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::I32Const(1));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::I32Const(0));
    func.instruction(&Instruction::LocalGet(padded_len));
    func.instruction(&Instruction::LocalGet(data_len));
    func.instruction(&Instruction::I32Sub);
    func.instruction(&Instruction::I32Const(1 + 8));
    func.instruction(&Instruction::I32Sub);
    func.instruction(&Instruction::MemoryFill(0));

    // Append 64-bit bit length in big-endian at end
    // SHA-256 requires: [bit_len_hi (4 bytes BE)][bit_len_lo (4 bytes BE)] at end of padding

    // Store bit_len_hi (high 32 bits) in big-endian at offset -8
    let len_pos = base + 97; // Temporary for length position
    func.instruction(&Instruction::LocalGet(padded_ptr));
    func.instruction(&Instruction::LocalGet(padded_len));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::I32Const(8));
    func.instruction(&Instruction::I32Sub);
    func.instruction(&Instruction::LocalSet(len_pos));

    // Byte 0 of bit_len_hi (most significant)
    func.instruction(&Instruction::LocalGet(len_pos));
    func.instruction(&Instruction::LocalGet(bit_len_hi));
    func.instruction(&Instruction::I32Const(24));
    func.instruction(&Instruction::I32ShrU);
    func.instruction(&Instruction::I32Store8(MemArg { offset: 0, align: 0, memory_index: 0 }));

    // Byte 1
    func.instruction(&Instruction::LocalGet(len_pos));
    func.instruction(&Instruction::LocalGet(bit_len_hi));
    func.instruction(&Instruction::I32Const(16));
    func.instruction(&Instruction::I32ShrU);
    func.instruction(&Instruction::I32Const(0xFF));
    func.instruction(&Instruction::I32And);
    func.instruction(&Instruction::I32Store8(MemArg { offset: 1, align: 0, memory_index: 0 }));

    // Byte 2
    func.instruction(&Instruction::LocalGet(len_pos));
    func.instruction(&Instruction::LocalGet(bit_len_hi));
    func.instruction(&Instruction::I32Const(8));
    func.instruction(&Instruction::I32ShrU);
    func.instruction(&Instruction::I32Const(0xFF));
    func.instruction(&Instruction::I32And);
    func.instruction(&Instruction::I32Store8(MemArg { offset: 2, align: 0, memory_index: 0 }));

    // Byte 3 (least significant of bit_len_hi)
    func.instruction(&Instruction::LocalGet(len_pos));
    func.instruction(&Instruction::LocalGet(bit_len_hi));
    func.instruction(&Instruction::I32Const(0xFF));
    func.instruction(&Instruction::I32And);
    func.instruction(&Instruction::I32Store8(MemArg { offset: 3, align: 0, memory_index: 0 }));

    // Store bit_len_lo (low 32 bits) in big-endian at offset -4
    func.instruction(&Instruction::LocalGet(padded_ptr));
    func.instruction(&Instruction::LocalGet(padded_len));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::I32Const(4));
    func.instruction(&Instruction::I32Sub);
    func.instruction(&Instruction::LocalSet(len_pos));

    // Byte 0 of bit_len_lo (most significant of low word)
    func.instruction(&Instruction::LocalGet(len_pos));
    func.instruction(&Instruction::LocalGet(bit_len_lo));
    func.instruction(&Instruction::I32Const(24));
    func.instruction(&Instruction::I32ShrU);
    func.instruction(&Instruction::I32Store8(MemArg { offset: 0, align: 0, memory_index: 0 }));

    // Byte 1
    func.instruction(&Instruction::LocalGet(len_pos));
    func.instruction(&Instruction::LocalGet(bit_len_lo));
    func.instruction(&Instruction::I32Const(16));
    func.instruction(&Instruction::I32ShrU);
    func.instruction(&Instruction::I32Const(0xFF));
    func.instruction(&Instruction::I32And);
    func.instruction(&Instruction::I32Store8(MemArg { offset: 1, align: 0, memory_index: 0 }));

    // Byte 2
    func.instruction(&Instruction::LocalGet(len_pos));
    func.instruction(&Instruction::LocalGet(bit_len_lo));
    func.instruction(&Instruction::I32Const(8));
    func.instruction(&Instruction::I32ShrU);
    func.instruction(&Instruction::I32Const(0xFF));
    func.instruction(&Instruction::I32And);
    func.instruction(&Instruction::I32Store8(MemArg { offset: 2, align: 0, memory_index: 0 }));

    // Byte 3 (least significant)
    func.instruction(&Instruction::LocalGet(len_pos));
    func.instruction(&Instruction::LocalGet(bit_len_lo));
    func.instruction(&Instruction::I32Const(0xFF));
    func.instruction(&Instruction::I32And);
    func.instruction(&Instruction::I32Store8(MemArg { offset: 3, align: 0, memory_index: 0 }));

    // Initialize hash values
    for (idx, val) in [H0_INIT, H1_INIT, H2_INIT, H3_INIT, H4_INIT, H5_INIT, H6_INIT, H7_INIT].iter().enumerate() {
        func.instruction(&Instruction::I32Const(*val as i32));
        func.instruction(&Instruction::LocalSet(h[idx]));
    }

    // Process each 512-bit block
    func.instruction(&Instruction::I32Const(0));
    func.instruction(&Instruction::LocalSet(block_offset));

    // Block loop: while block_offset < padded_len
    func.instruction(&Instruction::Block(BlockType::Empty));
    func.instruction(&Instruction::Loop(BlockType::Empty));

    // Check loop condition
    func.instruction(&Instruction::LocalGet(block_offset));
    func.instruction(&Instruction::LocalGet(padded_len));
    func.instruction(&Instruction::I32GeU);
    func.instruction(&Instruction::BrIf(1));

    // Load 16 words (64 bytes) from current block into w[0..16]
    // SHA-256 requires big-endian, but I32Load is little-endian, so we must byte-swap
    for i in 0..16 {
        func.instruction(&Instruction::LocalGet(padded_ptr));
        func.instruction(&Instruction::LocalGet(block_offset));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::I32Load(MemArg { offset: (i * 4) as u64, align: 2, memory_index: 0 }));

        // Byte-swap from little-endian to big-endian: ABCD -> DCBA
        // result = ((word << 24) & 0xFF000000) | ((word << 8) & 0x00FF0000) |
        //          ((word >> 8) & 0x0000FF00) | ((word >> 24) & 0x000000FF)

        // Duplicate word for multiple operations
        func.instruction(&Instruction::LocalTee(w_start + i));

        // Part 1: (word << 24) - rotate byte 0 to position 3
        func.instruction(&Instruction::I32Const(24));
        func.instruction(&Instruction::I32Shl);

        // Part 2: (word << 8) & 0x00FF0000 - rotate byte 1 to position 2
        func.instruction(&Instruction::LocalGet(w_start + i));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32Shl);
        func.instruction(&Instruction::I32Const(0x00FF0000u32 as i32));
        func.instruction(&Instruction::I32And);
        func.instruction(&Instruction::I32Or);

        // Part 3: (word >> 8) & 0x0000FF00 - rotate byte 2 to position 1
        func.instruction(&Instruction::LocalGet(w_start + i));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32ShrU);
        func.instruction(&Instruction::I32Const(0x0000FF00));
        func.instruction(&Instruction::I32And);
        func.instruction(&Instruction::I32Or);

        // Part 4: (word >> 24) - rotate byte 3 to position 0
        func.instruction(&Instruction::LocalGet(w_start + i));
        func.instruction(&Instruction::I32Const(24));
        func.instruction(&Instruction::I32ShrU);
        func.instruction(&Instruction::I32Or);

        // Store the byte-swapped value
        func.instruction(&Instruction::LocalSet(w_start + i));
    }

    // Extend into w[16..64] using message schedule
    for i in 16..64 {
        let w_i = w_start + i;
        let w_i_2 = w_start + (i - 2);
        let w_i_7 = w_start + (i - 7);
        let w_i_15 = w_start + (i - 15);
        let w_i_16 = w_start + (i - 16);

        // s0 = ROTR(w[i-15], 7) ^ ROTR(w[i-15], 18) ^ (w[i-15] >> 3)
        func.instruction(&Instruction::LocalGet(w_i_15));
        func.instruction(&Instruction::I32Const(7));
        func.instruction(&Instruction::I32Rotr);

        func.instruction(&Instruction::LocalGet(w_i_15));
        func.instruction(&Instruction::I32Const(18));
        func.instruction(&Instruction::I32Rotr);
        func.instruction(&Instruction::I32Xor);

        func.instruction(&Instruction::LocalGet(w_i_15));
        func.instruction(&Instruction::I32Const(3));
        func.instruction(&Instruction::I32ShrU);
        func.instruction(&Instruction::I32Xor);
        func.instruction(&Instruction::LocalSet(s0));

        // s1 = ROTR(w[i-2], 17) ^ ROTR(w[i-2], 19) ^ (w[i-2] >> 10)
        func.instruction(&Instruction::LocalGet(w_i_2));
        func.instruction(&Instruction::I32Const(17));
        func.instruction(&Instruction::I32Rotr);

        func.instruction(&Instruction::LocalGet(w_i_2));
        func.instruction(&Instruction::I32Const(19));
        func.instruction(&Instruction::I32Rotr);
        func.instruction(&Instruction::I32Xor);

        func.instruction(&Instruction::LocalGet(w_i_2));
        func.instruction(&Instruction::I32Const(10));
        func.instruction(&Instruction::I32ShrU);
        func.instruction(&Instruction::I32Xor);
        func.instruction(&Instruction::LocalSet(s1));

        // w[i] = w[i-16] + s0 + w[i-7] + s1
        func.instruction(&Instruction::LocalGet(w_i_16));
        func.instruction(&Instruction::LocalGet(s0));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(w_i_7));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalGet(s1));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(w_i));
    }

    // Initialize working variables from hash values
    for i in 0..8 {
        func.instruction(&Instruction::LocalGet(h[i]));
        func.instruction(&Instruction::LocalSet(work[i]));
    }

    // 64 rounds of compression
    func.instruction(&Instruction::I32Const(0));
    func.instruction(&Instruction::LocalSet(round_idx));

    func.instruction(&Instruction::Block(BlockType::Empty));
    func.instruction(&Instruction::Loop(BlockType::Empty));

    // Check round loop condition
    func.instruction(&Instruction::LocalGet(round_idx));
    func.instruction(&Instruction::I32Const(64));
    func.instruction(&Instruction::I32GeU);
    func.instruction(&Instruction::BrIf(1));

    // S1 = ROTR(e, 6) ^ ROTR(e, 11) ^ ROTR(e, 25)
    func.instruction(&Instruction::LocalGet(work[4]));
    func.instruction(&Instruction::I32Const(6));
    func.instruction(&Instruction::I32Rotr);

    func.instruction(&Instruction::LocalGet(work[4]));
    func.instruction(&Instruction::I32Const(11));
    func.instruction(&Instruction::I32Rotr);
    func.instruction(&Instruction::I32Xor);

    func.instruction(&Instruction::LocalGet(work[4]));
    func.instruction(&Instruction::I32Const(25));
    func.instruction(&Instruction::I32Rotr);
    func.instruction(&Instruction::I32Xor);
    func.instruction(&Instruction::LocalSet(s1));

    // ch = (e & f) ^ ((~e) & g)
    func.instruction(&Instruction::LocalGet(work[4]));
    func.instruction(&Instruction::LocalGet(work[5]));
    func.instruction(&Instruction::I32And);

    func.instruction(&Instruction::LocalGet(work[4]));
    func.instruction(&Instruction::I32Const(-1));
    func.instruction(&Instruction::I32Xor);
    func.instruction(&Instruction::LocalGet(work[6]));
    func.instruction(&Instruction::I32And);
    func.instruction(&Instruction::I32Xor);
    func.instruction(&Instruction::LocalSet(ch));

    // Load K[round_idx] into k_val using if-else ladder (deterministic constant selection)
    func.instruction(&Instruction::I32Const(0));
    func.instruction(&Instruction::LocalSet(k_val));
    for (k_idx, k_const) in K.iter().enumerate() {
        func.instruction(&Instruction::LocalGet(round_idx));
        func.instruction(&Instruction::I32Const(k_idx as i32));
        func.instruction(&Instruction::I32Eq);
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::I32Const(*k_const as i32));
        func.instruction(&Instruction::LocalSet(k_val));
        func.instruction(&Instruction::End);
    }

    // Load w[round_idx] into w_val using if-else ladder
    func.instruction(&Instruction::I32Const(0));
    func.instruction(&Instruction::LocalSet(w_val));
    for w_idx in 0..64 {
        func.instruction(&Instruction::LocalGet(round_idx));
        func.instruction(&Instruction::I32Const(w_idx as i32));
        func.instruction(&Instruction::I32Eq);
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::LocalGet(w_start + w_idx));
        func.instruction(&Instruction::LocalSet(w_val));
        func.instruction(&Instruction::End);
    }

    // temp1 = h + S1 + ch + K[round_idx] + w[round_idx]
    func.instruction(&Instruction::LocalGet(work[7]));
    func.instruction(&Instruction::LocalGet(s1));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::LocalGet(ch));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::LocalGet(k_val));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::LocalGet(w_val));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::LocalSet(temp1));

    // S0 = ROTR(a, 2) ^ ROTR(a, 13) ^ ROTR(a, 22)
    func.instruction(&Instruction::LocalGet(work[0]));
    func.instruction(&Instruction::I32Const(2));
    func.instruction(&Instruction::I32Rotr);

    func.instruction(&Instruction::LocalGet(work[0]));
    func.instruction(&Instruction::I32Const(13));
    func.instruction(&Instruction::I32Rotr);
    func.instruction(&Instruction::I32Xor);

    func.instruction(&Instruction::LocalGet(work[0]));
    func.instruction(&Instruction::I32Const(22));
    func.instruction(&Instruction::I32Rotr);
    func.instruction(&Instruction::I32Xor);
    func.instruction(&Instruction::LocalSet(s0));

    // maj = (a & b) ^ (a & c) ^ (b & c)
    func.instruction(&Instruction::LocalGet(work[0]));
    func.instruction(&Instruction::LocalGet(work[1]));
    func.instruction(&Instruction::I32And);

    func.instruction(&Instruction::LocalGet(work[0]));
    func.instruction(&Instruction::LocalGet(work[2]));
    func.instruction(&Instruction::I32And);
    func.instruction(&Instruction::I32Xor);

    func.instruction(&Instruction::LocalGet(work[1]));
    func.instruction(&Instruction::LocalGet(work[2]));
    func.instruction(&Instruction::I32And);
    func.instruction(&Instruction::I32Xor);
    func.instruction(&Instruction::LocalSet(maj));

    // temp2 = S0 + maj
    func.instruction(&Instruction::LocalGet(s0));
    func.instruction(&Instruction::LocalGet(maj));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::LocalSet(temp2));

    // Update working variables (rotate h=g, g=f, f=e, e=d+temp1, d=c, c=b, b=a, a=temp1+temp2)
    func.instruction(&Instruction::LocalGet(work[6]));
    func.instruction(&Instruction::LocalSet(work[7]));

    func.instruction(&Instruction::LocalGet(work[5]));
    func.instruction(&Instruction::LocalSet(work[6]));

    func.instruction(&Instruction::LocalGet(work[4]));
    func.instruction(&Instruction::LocalSet(work[5]));

    func.instruction(&Instruction::LocalGet(work[3]));
    func.instruction(&Instruction::LocalGet(temp1));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::LocalSet(work[4]));

    func.instruction(&Instruction::LocalGet(work[2]));
    func.instruction(&Instruction::LocalSet(work[3]));

    func.instruction(&Instruction::LocalGet(work[1]));
    func.instruction(&Instruction::LocalSet(work[2]));

    func.instruction(&Instruction::LocalGet(work[0]));
    func.instruction(&Instruction::LocalSet(work[1]));

    func.instruction(&Instruction::LocalGet(temp1));
    func.instruction(&Instruction::LocalGet(temp2));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::LocalSet(work[0]));

    // Increment round index
    func.instruction(&Instruction::LocalGet(round_idx));
    func.instruction(&Instruction::I32Const(1));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::LocalSet(round_idx));

    // Continue round loop
    func.instruction(&Instruction::Br(0));
    func.instruction(&Instruction::End);
    func.instruction(&Instruction::End);

    // Add working variables to hash values
    for i in 0..8 {
        func.instruction(&Instruction::LocalGet(h[i]));
        func.instruction(&Instruction::LocalGet(work[i]));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(h[i]));
    }

    // Increment block offset by 64
    func.instruction(&Instruction::LocalGet(block_offset));
    func.instruction(&Instruction::I32Const(64));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::LocalSet(block_offset));

    // Continue block loop
    func.instruction(&Instruction::Br(0));
    func.instruction(&Instruction::End);
    func.instruction(&Instruction::End);

    // Allocate result buffer
    func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
    func.instruction(&Instruction::LocalSet(new_ptr));

    func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
    func.instruction(&Instruction::I32Const(40));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::GlobalGet(HEAP_LIMIT_GLOBAL));
    func.instruction(&Instruction::I32GtU);
    func.instruction(&Instruction::If(BlockType::Empty));
    func.instruction(&Instruction::Unreachable);
    func.instruction(&Instruction::End);

    // Store type tag
    func.instruction(&Instruction::LocalGet(new_ptr));
    func.instruction(&Instruction::I32Const(TYPE_BYTES));
    func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));

    // Store length (32 bytes)
    func.instruction(&Instruction::LocalGet(new_ptr));
    func.instruction(&Instruction::I32Const(32));
    func.instruction(&Instruction::I32Store(MemArg { offset: 4, align: 2, memory_index: 0 }));

    // Store hash values as big-endian bytes
    for (idx, h_local) in h.iter().enumerate() {
        let offset = (idx * 4) as u64;

        func.instruction(&Instruction::LocalGet(new_ptr));
        func.instruction(&Instruction::LocalGet(*h_local));
        func.instruction(&Instruction::I32Const(24));
        func.instruction(&Instruction::I32ShrU);
        func.instruction(&Instruction::I32Store8(MemArg { offset: 8 + offset, align: 0, memory_index: 0 }));

        func.instruction(&Instruction::LocalGet(new_ptr));
        func.instruction(&Instruction::LocalGet(*h_local));
        func.instruction(&Instruction::I32Const(16));
        func.instruction(&Instruction::I32ShrU);
        func.instruction(&Instruction::I32Const(0xFF));
        func.instruction(&Instruction::I32And);
        func.instruction(&Instruction::I32Store8(MemArg { offset: 8 + offset + 1, align: 0, memory_index: 0 }));

        func.instruction(&Instruction::LocalGet(new_ptr));
        func.instruction(&Instruction::LocalGet(*h_local));
        func.instruction(&Instruction::I32Const(8));
        func.instruction(&Instruction::I32ShrU);
        func.instruction(&Instruction::I32Const(0xFF));
        func.instruction(&Instruction::I32And);
        func.instruction(&Instruction::I32Store8(MemArg { offset: 8 + offset + 2, align: 0, memory_index: 0 }));

        func.instruction(&Instruction::LocalGet(new_ptr));
        func.instruction(&Instruction::LocalGet(*h_local));
        func.instruction(&Instruction::I32Const(0xFF));
        func.instruction(&Instruction::I32And);
        func.instruction(&Instruction::I32Store8(MemArg { offset: 8 + offset + 3, align: 0, memory_index: 0 }));
    }

    // Update heap pointer
    func.instruction(&Instruction::GlobalGet(HEAP_PTR_GLOBAL));
    func.instruction(&Instruction::I32Const(40));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::GlobalSet(HEAP_PTR_GLOBAL));

    // Return new pointer
    func.instruction(&Instruction::LocalGet(new_ptr));

    let _ = (K, temp1, temp2);
}

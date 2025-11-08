use anyhow::Result;
use wasm_encoder::*;
use std::collections::BTreeMap;

use super::ir::*;
use super::memory;

const GAS_LIMIT: i32 = 100_000_000;
const HEAP_START: i32 = 0x10000;
const HEAP_LIMIT: i32 = 0x400000;

pub(crate) struct WasmCodegen {
    function_indices: BTreeMap<String, u32>,
    gas_global: u32,
}

impl WasmCodegen {
    pub fn new() -> Self {
        Self {
            function_indices: BTreeMap::new(),
            gas_global: 0,
        }
    }

    pub fn generate(&mut self, ir: &IR) -> Result<Vec<u8>> {
        let IR::Module { functions, .. } = ir;

        for (idx, func) in functions.iter().enumerate() {
            self.function_indices.insert(func.name.clone(), idx as u32);
        }

        let mut module = Module::new();

        // create types for each function based on parameter count
        let mut types = TypeSection::new();
        for func in functions.iter() {
            let param_count = func._params.len();
            let params = vec![ValType::I32; param_count];
            types.function(params, vec![ValType::I32]);
        }
        module.section(&types);

        // Import section
        let mut imports = ImportSection::new();
        imports.import(
            "env",
            "memory",
            MemoryType {
                minimum: 16,
                maximum: Some(256),
                memory64: false,
                shared: false,
            },
        );
        module.section(&imports);

        // Function section
        let mut funcs = FunctionSection::new();
        for idx in 0..functions.len() {
            funcs.function(idx as u32);
        }
        module.section(&funcs);

        // Global section: gas counter, heap pointer, heap limit
        let mut globals = GlobalSection::new();
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: true,
            },
            &ConstExpr::i32_const(0),
        );
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: true,
            },
            &ConstExpr::i32_const(HEAP_START),
        );
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: false,
            },
            &ConstExpr::i32_const(HEAP_LIMIT),
        );
        module.section(&globals);

        // Export section
        let mut exports = ExportSection::new();
        exports.export("main", ExportKind::Func, 0);
        exports.export("memory", ExportKind::Memory, 0);
        module.section(&exports);

        // Code section
        let mut code = CodeSection::new();
        for func in functions {
            let wasm_func = self.generate_function(func)?;
            code.function(&wasm_func);
        }
        module.section(&code);

        Ok(module.finish())
    }

    fn generate_function(&mut self, func: &IRFunction) -> Result<Function> {
        // In WASM, parameters are the first N locals
        // We only declare additional locals beyond parameters
        let param_count = func._params.len() as u32;
        let additional_locals = func.locals.len() as u32 - param_count + func.temp_locals;

        let mut wasm_func = if additional_locals > 0 {
            Function::new(vec![(additional_locals, ValType::I32)])
        } else {
            Function::new(vec![])
        };

        // Gas temp local is the last scratch local
        let gas_temp_local = func.locals.len() as u32 + func.temp_locals - 1;

        self.meter_gas(&mut wasm_func, 10, gas_temp_local);

        let base_scratch = func.locals.len() as u32;

        for stmt in &func.body {
            let mut next_scratch = base_scratch;
            self.generate_stmt_with_scratch(&mut wasm_func, stmt, func, gas_temp_local, &mut next_scratch)?;
        }

        // For main function, return OUTPUT variable if it exists, otherwise 0
        if func.name == "main" {
            if let Some(&output_idx) = func.local_map.get("OUTPUT") {
                wasm_func.instruction(&Instruction::LocalGet(output_idx));
            } else {
                wasm_func.instruction(&Instruction::I32Const(0));
            }
        } else {
            // For non-main functions, return 0 if no explicit return
            wasm_func.instruction(&Instruction::I32Const(0));
        }
        wasm_func.instruction(&Instruction::End);

        Ok(wasm_func)
    }

    fn meter_gas(&self, func: &mut Function, cost: i32, gas_temp_local: u32) {
        func.instruction(&Instruction::GlobalGet(self.gas_global));
        func.instruction(&Instruction::I32Const(cost));
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalTee(gas_temp_local));
        func.instruction(&Instruction::I32Const(GAS_LIMIT));
        func.instruction(&Instruction::I32GtS);
        func.instruction(&Instruction::If(BlockType::Empty));
        func.instruction(&Instruction::Unreachable);
        func.instruction(&Instruction::End);
        func.instruction(&Instruction::LocalGet(gas_temp_local));
        func.instruction(&Instruction::GlobalSet(self.gas_global));
    }

    fn generate_stmt_with_scratch(&mut self, func: &mut Function, stmt: &IRStmt, ir_func: &IRFunction, gas_temp_local: u32, next_scratch: &mut u32) -> Result<()> {
        match stmt {
            IRStmt::Assign { var, value } => {
                self.generate_expr(func, value, ir_func, gas_temp_local, next_scratch)?;
                let local_idx = ir_func.local_map.get(var)
                    .ok_or_else(|| anyhow::anyhow!("Variable '{}' not in local_map", var))?;
                func.instruction(&Instruction::LocalSet(*local_idx));
            }
            IRStmt::SubscriptAssign { target, index, value } => {
                // generate target, index, value on stack
                self.generate_expr(func, target, ir_func, gas_temp_local, next_scratch)?;
                self.generate_expr(func, index, ir_func, gas_temp_local, next_scratch)?;
                self.generate_expr(func, value, ir_func, gas_temp_local, next_scratch)?;

                let base = *next_scratch;
                *next_scratch = base + 11;

                // save to locals: target, index, value
                func.instruction(&Instruction::LocalSet(base + 2));
                func.instruction(&Instruction::LocalSet(base + 1));
                func.instruction(&Instruction::LocalSet(base));

                // load type tag from target[0]
                func.instruction(&Instruction::LocalGet(base));
                func.instruction(&Instruction::I32Load(MemArg { offset: 0, align: 2, memory_index: 0 }));

                // type == 1 (list)?
                func.instruction(&Instruction::I32Const(1));
                func.instruction(&Instruction::I32Eq);
                func.instruction(&Instruction::If(BlockType::Empty));

                // list branch
                func.instruction(&Instruction::LocalGet(base));
                func.instruction(&Instruction::LocalGet(base + 1));
                func.instruction(&Instruction::LocalGet(base + 2));
                memory::ListLayout::update_element(func, base + 3, base + 4, base + 5);

                func.instruction(&Instruction::Else);

                // dict branch
                func.instruction(&Instruction::LocalGet(base));
                func.instruction(&Instruction::LocalGet(base + 1));
                func.instruction(&Instruction::LocalGet(base + 2));
                memory::DictLayout::insert(func, base + 3, base + 4, base + 5, base + 6, base + 7, base + 8, base + 9, base + 10);

                func.instruction(&Instruction::End);
            }
            IRStmt::Return(expr) => {
                self.generate_expr(func, expr, ir_func, gas_temp_local, next_scratch)?;
                func.instruction(&Instruction::Return);
            }
            IRStmt::If { cond, then_block, else_block } => {
                self.generate_expr(func, cond, ir_func, gas_temp_local, next_scratch)?;
                func.instruction(&Instruction::If(BlockType::Empty));
                for s in then_block {
                    self.generate_stmt_with_scratch(func, s, ir_func, gas_temp_local, next_scratch)?;
                }
                if !else_block.is_empty() {
                    func.instruction(&Instruction::Else);
                    for s in else_block {
                        self.generate_stmt_with_scratch(func, s, ir_func, gas_temp_local, next_scratch)?;
                    }
                }
                func.instruction(&Instruction::End);
            }
            IRStmt::While { cond, body } => {
                func.instruction(&Instruction::Block(BlockType::Empty));
                func.instruction(&Instruction::Loop(BlockType::Empty));
                self.meter_gas(func, 1, gas_temp_local);
                self.generate_expr(func, cond, ir_func, gas_temp_local, next_scratch)?;
                func.instruction(&Instruction::I32Eqz);
                func.instruction(&Instruction::BrIf(1));
                for s in body {
                    self.generate_stmt_with_scratch(func, s, ir_func, gas_temp_local, next_scratch)?;
                }
                func.instruction(&Instruction::Br(0));
                func.instruction(&Instruction::End);
                func.instruction(&Instruction::End);
            }
            IRStmt::For { var, iter, body } => {
                let loop_var = ir_func.local_map.get(var)
                    .ok_or_else(|| anyhow::anyhow!("Loop variable '{}' not in local_map", var))?;

                let counter = *next_scratch;
                let body_scratch_base = counter + 1;

                func.instruction(&Instruction::I32Const(0));
                func.instruction(&Instruction::LocalSet(counter));

                func.instruction(&Instruction::Block(BlockType::Empty));
                func.instruction(&Instruction::Loop(BlockType::Empty));
                self.meter_gas(func, 1, gas_temp_local);

                func.instruction(&Instruction::LocalGet(counter));
                let mut iter_scratch = body_scratch_base;
                self.generate_expr(func, iter, ir_func, gas_temp_local, &mut iter_scratch)?;
                func.instruction(&Instruction::I32GeS);
                func.instruction(&Instruction::BrIf(1));

                func.instruction(&Instruction::LocalGet(counter));
                func.instruction(&Instruction::LocalSet(*loop_var));
                for s in body {
                    let mut body_scratch = body_scratch_base;
                    self.generate_stmt_with_scratch(func, s, ir_func, gas_temp_local, &mut body_scratch)?;
                }

                func.instruction(&Instruction::LocalGet(counter));
                func.instruction(&Instruction::I32Const(1));
                func.instruction(&Instruction::I32Add);
                func.instruction(&Instruction::LocalSet(counter));

                func.instruction(&Instruction::Br(0));
                func.instruction(&Instruction::End);
                func.instruction(&Instruction::End);
            }
            IRStmt::Expr(expr) => {
                self.generate_expr(func, expr, ir_func, gas_temp_local, next_scratch)?;
                func.instruction(&Instruction::Drop);
            }
            IRStmt::Block(stmts) => {
                for s in stmts {
                    self.generate_stmt_with_scratch(func, s, ir_func, gas_temp_local, next_scratch)?;
                }
            }
        }
        Ok(())
    }

    fn generate_expr(&mut self, func: &mut Function, expr: &IRExpr, ir_func: &IRFunction, gas_temp_local: u32, next_scratch: &mut u32) -> Result<()> {
        match expr {
            IRExpr::Const(val) => {
                func.instruction(&Instruction::I32Const(*val));
            }
            IRExpr::LoadLocal(var) => {
                let idx = ir_func.local_map.get(var)
                    .ok_or_else(|| anyhow::anyhow!("Variable '{}' not in local_map", var))?;
                func.instruction(&Instruction::LocalGet(*idx));
            }
            IRExpr::UnaryOp { op, operand } => {
                match op {
                    UnaryOp::Neg => {
                        func.instruction(&Instruction::I32Const(0));
                        self.generate_expr(func, operand, ir_func, gas_temp_local, next_scratch)?;
                        func.instruction(&Instruction::I32Sub);
                    }
                    UnaryOp::Not => {
                        self.generate_expr(func, operand, ir_func, gas_temp_local, next_scratch)?;
                        func.instruction(&Instruction::I32Eqz);
                    }
                }
            }
            IRExpr::BinOp { op, left, right } => {
                // Runtime type dispatch for string operations
                match op {
                    BinOp::Add => {
                        // Type-aware addition: string concatenation or integer addition
                        let saved_scratch = *next_scratch;
                        let left_local = *next_scratch;
                        let right_local = left_local + 1;
                        *next_scratch = right_local + 1;

                        self.generate_expr(func, left, ir_func, gas_temp_local, next_scratch)?;
                        func.instruction(&Instruction::LocalSet(left_local));

                        self.generate_expr(func, right, ir_func, gas_temp_local, next_scratch)?;
                        func.instruction(&Instruction::LocalSet(right_local));

                        // Check if left is heap pointer (>= 1024) AND is string (type tag == 3)
                        func.instruction(&Instruction::LocalGet(left_local));
                        func.instruction(&Instruction::I32Const(1024));
                        func.instruction(&Instruction::I32GeU);

                        func.instruction(&Instruction::LocalGet(left_local));
                        func.instruction(&Instruction::I32Load(MemArg { offset: 0, align: 2, memory_index: 0 }));
                        func.instruction(&Instruction::I32Const(3)); // TYPE_STRING
                        func.instruction(&Instruction::I32Eq);

                        func.instruction(&Instruction::I32And);

                        func.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
                        // String concatenation path
                        func.instruction(&Instruction::LocalGet(left_local));
                        func.instruction(&Instruction::LocalGet(right_local));
                        let base = *next_scratch;
                        memory::StringLayout::concat(func, base, base + 1, base + 2, base + 3, base + 4, base + 5, base + 6);
                        func.instruction(&Instruction::Else);
                        // Integer addition path
                        func.instruction(&Instruction::LocalGet(left_local));
                        func.instruction(&Instruction::LocalGet(right_local));
                        func.instruction(&Instruction::I32Add);
                        func.instruction(&Instruction::End);

                        *next_scratch = saved_scratch;
                    }
                    BinOp::Eq => {
                        // Type-aware equality: string equals or integer equals
                        let saved_scratch = *next_scratch;
                        let left_local = *next_scratch;
                        let right_local = left_local + 1;
                        *next_scratch = right_local + 1;

                        self.generate_expr(func, left, ir_func, gas_temp_local, next_scratch)?;
                        func.instruction(&Instruction::LocalSet(left_local));

                        self.generate_expr(func, right, ir_func, gas_temp_local, next_scratch)?;
                        func.instruction(&Instruction::LocalSet(right_local));

                        // Check if left is heap pointer (>= 1024) AND is string (type tag == 3)
                        func.instruction(&Instruction::LocalGet(left_local));
                        func.instruction(&Instruction::I32Const(1024));
                        func.instruction(&Instruction::I32GeU);

                        func.instruction(&Instruction::LocalGet(left_local));
                        func.instruction(&Instruction::I32Load(MemArg { offset: 0, align: 2, memory_index: 0 }));
                        func.instruction(&Instruction::I32Const(3)); // TYPE_STRING
                        func.instruction(&Instruction::I32Eq);

                        func.instruction(&Instruction::I32And);

                        func.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
                        // String equality path
                        func.instruction(&Instruction::LocalGet(left_local));
                        func.instruction(&Instruction::LocalGet(right_local));
                        let base = *next_scratch;
                        memory::StringLayout::equals(func, base, base + 1, base + 2, base + 3, base + 4);
                        func.instruction(&Instruction::Else);
                        // Integer equality path
                        func.instruction(&Instruction::LocalGet(left_local));
                        func.instruction(&Instruction::LocalGet(right_local));
                        func.instruction(&Instruction::I32Eq);
                        func.instruction(&Instruction::End);

                        *next_scratch = saved_scratch;
                    }
                    _ => {
                        self.generate_expr(func, left, ir_func, gas_temp_local, next_scratch)?;
                        self.generate_expr(func, right, ir_func, gas_temp_local, next_scratch)?;

                        match op {
                            BinOp::FloorDiv => {
                        let scratch0 = ir_func.locals.len() as u32;
                        let scratch1 = scratch0 + 1;
                        let scratch2 = scratch0 + 2;

                        func.instruction(&Instruction::LocalSet(scratch1));
                        func.instruction(&Instruction::LocalSet(scratch0));

                        func.instruction(&Instruction::LocalGet(scratch1));
                        func.instruction(&Instruction::I32Eqz);
                        func.instruction(&Instruction::If(BlockType::Empty));
                        func.instruction(&Instruction::Unreachable);
                        func.instruction(&Instruction::End);

                        func.instruction(&Instruction::LocalGet(scratch0));
                        func.instruction(&Instruction::LocalGet(scratch1));
                        func.instruction(&Instruction::I32DivS);
                        func.instruction(&Instruction::LocalSet(scratch2));

                        func.instruction(&Instruction::LocalGet(scratch0));
                        func.instruction(&Instruction::LocalGet(scratch1));
                        func.instruction(&Instruction::I32RemS);
                        func.instruction(&Instruction::LocalTee(scratch0));
                        func.instruction(&Instruction::I32Const(0));
                        func.instruction(&Instruction::I32Ne);

                        func.instruction(&Instruction::LocalGet(scratch0));
                        func.instruction(&Instruction::LocalGet(scratch1));
                        func.instruction(&Instruction::I32Xor);
                        func.instruction(&Instruction::I32Const(0));
                        func.instruction(&Instruction::I32LtS);

                        func.instruction(&Instruction::I32And);
                        func.instruction(&Instruction::If(BlockType::Empty));
                        func.instruction(&Instruction::LocalGet(scratch2));
                        func.instruction(&Instruction::I32Const(1));
                        func.instruction(&Instruction::I32Sub);
                        func.instruction(&Instruction::LocalSet(scratch2));
                        func.instruction(&Instruction::End);

                        func.instruction(&Instruction::LocalGet(scratch2));
                    }
                    BinOp::Mod => {
                        // Python modulo: result has same sign as divisor
                        // if C_rem and divisor have different signs: result = C_rem + divisor
                        // else: result = C_rem
                        let scratch0 = ir_func.locals.len() as u32;
                        let scratch1 = scratch0 + 1;
                        let scratch2 = scratch0 + 2;

                        // Store operands
                        func.instruction(&Instruction::LocalSet(scratch1));  // b
                        func.instruction(&Instruction::LocalSet(scratch0));  // a

                        // Check for division by zero
                        func.instruction(&Instruction::LocalGet(scratch1));
                        func.instruction(&Instruction::I32Eqz);
                        func.instruction(&Instruction::If(BlockType::Empty));
                        func.instruction(&Instruction::Unreachable);
                        func.instruction(&Instruction::End);

                        // Compute C-style remainder: r = a % b
                        func.instruction(&Instruction::LocalGet(scratch0));
                        func.instruction(&Instruction::LocalGet(scratch1));
                        func.instruction(&Instruction::I32RemS);
                        func.instruction(&Instruction::LocalTee(scratch2));  // r

                        // Check if r == 0, if so just return 0
                        func.instruction(&Instruction::I32Eqz);
                        func.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
                        func.instruction(&Instruction::I32Const(0));
                        func.instruction(&Instruction::Else);

                        // Check if signs differ: (r ^ b) < 0
                        func.instruction(&Instruction::LocalGet(scratch2));
                        func.instruction(&Instruction::LocalGet(scratch1));
                        func.instruction(&Instruction::I32Xor);
                        func.instruction(&Instruction::I32Const(0));
                        func.instruction(&Instruction::I32LtS);

                        // If signs differ: r + b, else: r
                        func.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
                        func.instruction(&Instruction::LocalGet(scratch2));
                        func.instruction(&Instruction::LocalGet(scratch1));
                        func.instruction(&Instruction::I32Add);
                        func.instruction(&Instruction::Else);
                        func.instruction(&Instruction::LocalGet(scratch2));
                        func.instruction(&Instruction::End);

                        func.instruction(&Instruction::End);
                    }
                    BinOp::Div => {
                        // Integer division only (no floats for determinism)
                        func.instruction(&Instruction::I32DivS);
                    }
                            _ => {
                                let instr = match op {
                                    BinOp::Sub => Instruction::I32Sub,
                                    BinOp::Mul => Instruction::I32Mul,
                                    BinOp::Ne => Instruction::I32Ne,
                                    BinOp::Lt => Instruction::I32LtS,
                                    BinOp::Le => Instruction::I32LeS,
                                    BinOp::Gt => Instruction::I32GtS,
                                    BinOp::Ge => Instruction::I32GeS,
                                    _ => unreachable!(),
                                };
                                func.instruction(&instr);
                            }
                        }
                    }
                }
            }
            IRExpr::Call { func: fname, args } => {
                for arg in args {
                    self.generate_expr(func, arg, ir_func, gas_temp_local, next_scratch)?;
                }
                let func_idx = self.function_indices.get(fname)
                    .ok_or_else(|| anyhow::anyhow!("Function '{}' not found", fname))?;
                func.instruction(&Instruction::Call(*func_idx));
            }
            IRExpr::List(elements) => {
                let length = elements.len() as u32;
                let scratch0 = *next_scratch;
                let scratch1 = scratch0 + 1;
                *next_scratch = scratch1 + 1;

                memory::ListLayout::alloc(func, length);
                func.instruction(&Instruction::LocalSet(scratch0));

                for (i, elem) in elements.iter().enumerate() {
                    func.instruction(&Instruction::LocalGet(scratch0));
                    func.instruction(&Instruction::I32Const(i as i32));
                    self.generate_expr(func, elem, ir_func, gas_temp_local, next_scratch)?;
                    memory::ListLayout::store_element(func, scratch1);
                }

                func.instruction(&Instruction::LocalGet(scratch0));
            }
            IRExpr::Dict(pairs) => {
                let capacity = (pairs.len() * 2).max(8) as u32;
                let base = *next_scratch;
                *next_scratch = base + 8;

                memory::DictLayout::alloc(func, capacity, base, base + 1);
                func.instruction(&Instruction::LocalSet(base));

                for (key_expr, val_expr) in pairs {
                    func.instruction(&Instruction::LocalGet(base));
                    self.generate_expr(func, key_expr, ir_func, gas_temp_local, next_scratch)?;
                    self.generate_expr(func, val_expr, ir_func, gas_temp_local, next_scratch)?;
                    memory::DictLayout::insert(func, base, base + 1, base + 2, base + 3, base + 4, base + 5, base + 6, base + 7);
                }

                func.instruction(&Instruction::LocalGet(base));
            }
            IRExpr::Subscript { value, index } => {
                // Type-aware subscript: string indexing or list/dict access
                let saved_scratch = *next_scratch;
                let value_local = *next_scratch;
                let index_local = value_local + 1;
                *next_scratch = index_local + 1;

                self.generate_expr(func, value, ir_func, gas_temp_local, next_scratch)?;
                func.instruction(&Instruction::LocalSet(value_local));

                self.generate_expr(func, index, ir_func, gas_temp_local, next_scratch)?;
                func.instruction(&Instruction::LocalSet(index_local));

                // Check if value is heap pointer (>= 1024) AND is string (type tag == 3)
                func.instruction(&Instruction::LocalGet(value_local));
                func.instruction(&Instruction::I32Const(1024));
                func.instruction(&Instruction::I32GeU);

                func.instruction(&Instruction::LocalGet(value_local));
                func.instruction(&Instruction::I32Load(MemArg { offset: 0, align: 2, memory_index: 0 }));
                func.instruction(&Instruction::I32Const(3)); // TYPE_STRING
                func.instruction(&Instruction::I32Eq);

                func.instruction(&Instruction::I32And);

                func.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
                // String indexing path
                func.instruction(&Instruction::LocalGet(value_local));
                func.instruction(&Instruction::LocalGet(index_local));
                let str_base = *next_scratch;
                memory::StringLayout::index(func, str_base, str_base + 1);
                func.instruction(&Instruction::Else);
                // List/dict access path
                func.instruction(&Instruction::LocalGet(value_local));
                func.instruction(&Instruction::LocalGet(index_local));
                let list_base = *next_scratch;
                memory::ListLayout::load_element(func, list_base, list_base + 1);
                func.instruction(&Instruction::End);

                *next_scratch = saved_scratch;
            }
            IRExpr::Str(s) => {
                let bytes = s.as_bytes();
                let base = *next_scratch;
                *next_scratch = base + 1;

                memory::StringLayout::alloc(func, bytes);
                func.instruction(&Instruction::LocalSet(base));

                // Copy bytes into allocated string
                for (i, &byte) in bytes.iter().enumerate() {
                    func.instruction(&Instruction::LocalGet(base));
                    func.instruction(&Instruction::I32Const(8 + i as i32));
                    func.instruction(&Instruction::I32Add);
                    func.instruction(&Instruction::I32Const(byte as i32));
                    func.instruction(&Instruction::I32Store8(MemArg { offset: 0, align: 0, memory_index: 0 }));
                }

                func.instruction(&Instruction::LocalGet(base));
            }
            IRExpr::Slice { value, start, end } => {
                // Stack needs to be: [str_ptr, start, end] before slice call
                let str_local = *next_scratch;
                *next_scratch = str_local + 1;

                // Generate and save string pointer
                self.generate_expr(func, value, ir_func, gas_temp_local, next_scratch)?;
                func.instruction(&Instruction::LocalSet(str_local));

                // Generate start (default 0)
                if let Some(start_expr) = start {
                    self.generate_expr(func, start_expr, ir_func, gas_temp_local, next_scratch)?;
                } else {
                    func.instruction(&Instruction::I32Const(0));
                }

                // Generate end (default string length)
                if let Some(end_expr) = end {
                    self.generate_expr(func, end_expr, ir_func, gas_temp_local, next_scratch)?;
                } else {
                    // Load string length as default end
                    func.instruction(&Instruction::LocalGet(str_local));
                    memory::StringLayout::load_length(func);
                }

                // Now stack is [start, end], need to get str_ptr under them
                let start_local = *next_scratch;
                let end_local = start_local + 1;
                *next_scratch = end_local + 1;

                func.instruction(&Instruction::LocalSet(end_local));
                func.instruction(&Instruction::LocalSet(start_local));
                func.instruction(&Instruction::LocalGet(str_local));
                func.instruction(&Instruction::LocalGet(start_local));
                func.instruction(&Instruction::LocalGet(end_local));

                // Allocate scratch locals for slice operation
                let base = *next_scratch;
                *next_scratch = base + 6;

                memory::StringLayout::slice(func, base, base + 1, base + 2, base + 3, base + 4, base + 5);
            }
            IRExpr::IfExpr { cond, then_val, else_val } => {
                // Conditional expression: if(cond) then_val else else_val
                self.generate_expr(func, cond, ir_func, gas_temp_local, next_scratch)?;

                func.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
                self.generate_expr(func, then_val, ir_func, gas_temp_local, next_scratch)?;
                func.instruction(&Instruction::Else);
                self.generate_expr(func, else_val, ir_func, gas_temp_local, next_scratch)?;
                func.instruction(&Instruction::End);
            }
        }
        Ok(())
    }
}

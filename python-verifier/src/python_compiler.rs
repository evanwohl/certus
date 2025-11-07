use anyhow::{Result, bail};
use wasm_encoder::*;
use std::collections::{HashMap, BTreeMap};
use std::sync::Arc;
use sha2::{Sha256, Digest};
use rustpython_parser::{self as parser, ast};

const MAX_LOCALS: usize = 256;
const MAX_PYTHON_SIZE: usize = 100 * 1024;
const SCRATCH_LOCALS: u32 = 4;
const GAS_LIMIT: i32 = 100_000_000;
const HEAP_START: i32 = 0x10000;
const HEAP_LIMIT: i32 = 0x400000;

pub struct PythonCompiler {
    cache: HashMap<String, Arc<Vec<u8>>>,
}

impl PythonCompiler {
    pub fn new() -> Self {
        Self {
            cache: HashMap::with_capacity(64),
        }
    }

    pub fn compile(&mut self, python_code: &str) -> Result<Vec<u8>> {
        if python_code.len() > MAX_PYTHON_SIZE {
            bail!("Python code exceeds 100KB limit");
        }

        let mut hasher = Sha256::new();
        hasher.update(python_code.as_bytes());
        let code_hash = hex::encode(hasher.finalize());

        if let Some(cached) = self.cache.get(&code_hash) {
            return Ok((**cached).clone());
        }

        let py_ast = self.parse_python(python_code)?;
        let ir = self.lower_to_ir(&py_ast)?;
        let wasm = self.codegen_wasm(&ir)?;

        self.cache.insert(code_hash, Arc::new(wasm.clone()));
        Ok(wasm)
    }

    fn parse_python(&self, code: &str) -> Result<ast::Mod> {
        parser::parse(code, parser::Mode::Module, "<input>")
            .map_err(|e| anyhow::anyhow!("Python parse error: {}", e))
    }

    fn lower_to_ir(&self, py_ast: &ast::Mod) -> Result<IR> {
        let mut lowering = IRLowering::new();
        lowering.lower_module(py_ast)
    }

    fn codegen_wasm(&self, ir: &IR) -> Result<Vec<u8>> {
        let mut codegen = WasmCodegen::new();
        codegen.generate(ir)
    }
}

// Intermediate representation for deterministic compilation
#[derive(Debug, Clone)]
pub enum IR {
    Module {
        functions: Vec<IRFunction>,
        _globals: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub struct IRFunction {
    pub name: String,
    pub _params: Vec<String>,
    pub locals: Vec<String>,
    pub local_map: HashMap<String, u32>,
    pub temp_locals: u32,
    pub body: Vec<IRStmt>,
}

#[derive(Debug, Clone)]
pub enum IRStmt {
    Assign { var: String, value: IRExpr },
    Return(IRExpr),
    If { cond: IRExpr, then_block: Vec<IRStmt>, else_block: Vec<IRStmt> },
    While { cond: IRExpr, body: Vec<IRStmt> },
    For { var: String, iter: IRExpr, body: Vec<IRStmt> },
    Expr(IRExpr),
    Block(Vec<IRStmt>),
}

#[derive(Debug, Clone)]
pub enum IRExpr {
    Const(i32),
    LoadLocal(String),
    BinOp { op: BinOp, left: Box<IRExpr>, right: Box<IRExpr> },
    UnaryOp { op: UnaryOp, operand: Box<IRExpr> },
    Call { func: String, args: Vec<IRExpr> },
    Attribute { obj: Box<IRExpr>, attr: String },
    Subscript { obj: Box<IRExpr>, index: Box<IRExpr> },
    List(Vec<IRExpr>),
    Dict(Vec<(IRExpr, IRExpr)>),
    Str(String),
}

#[derive(Debug, Clone)]
pub enum BinOp {
    Add, Sub, Mul, Div, FloorDiv, Mod,
    Eq, Ne, Lt, Le, Gt, Ge,
}

#[derive(Debug, Clone)]
pub enum UnaryOp {
    Neg,    // -x
    Not,    // not x
}

struct IRLowering {
    current_locals: BTreeMap<String, usize>,
    defined_functions: BTreeMap<String, bool>,
}

impl IRLowering {
    fn new() -> Self {
        Self {
            current_locals: BTreeMap::new(),
            defined_functions: BTreeMap::new(),
        }
    }

    fn lower_module(&mut self, module: &ast::Mod) -> Result<IR> {
        let ast::Mod::Module(ast::ModModule { body, .. }) = module else {
            bail!("Only module-level Python supported");
        };

        self.validate_determinism(body)?;

        for stmt in body {
            if let ast::Stmt::FunctionDef(func_def) = stmt {
                self.defined_functions.insert(func_def.name.to_string(), true);
            }
        }

        let mut functions = Vec::new();
        let mut main_body = Vec::new();

        for stmt in body {
            match stmt {
                ast::Stmt::FunctionDef(func_def) => {
                    functions.push(self.lower_function(func_def)?);
                }
                _ => {
                    main_body.push(self.lower_stmt(stmt)?);
                }
            }
        }

        let main_locals: Vec<String> = self.current_locals.keys().cloned().collect();
        let local_map: HashMap<String, u32> = main_locals.iter()
            .enumerate()
            .map(|(i, name)| (name.clone(), i as u32))
            .collect();

        let temp_count = SCRATCH_LOCALS as usize;

        if main_locals.len() + temp_count > MAX_LOCALS {
            bail!("Too many locals: {} + {} scratch exceeds limit", main_locals.len(), temp_count);
        }

        functions.insert(0, IRFunction {
            name: "main".to_string(),
            _params: vec![],
            locals: main_locals,
            local_map,
            temp_locals: temp_count as u32,
            body: main_body,
        });

        Ok(IR::Module {
            functions,
            _globals: vec![],
        })
    }

    fn validate_determinism(&self, body: &[ast::Stmt]) -> Result<()> {
        for stmt in body {
            self.check_stmt_determinism(stmt)?;
        }
        Ok(())
    }

    fn check_stmt_determinism(&self, stmt: &ast::Stmt) -> Result<()> {
        match stmt {
            ast::Stmt::Import(_) | ast::Stmt::ImportFrom(_) => {
                // imports are validated separately in lib.rs (only json/hashlib allowed)
            }
            ast::Stmt::FunctionDef(f) => {
                for s in &f.body {
                    self.check_stmt_determinism(s)?;
                }
            }
            ast::Stmt::If(if_stmt) => {
                for s in &if_stmt.body {
                    self.check_stmt_determinism(s)?;
                }
                for s in &if_stmt.orelse {
                    self.check_stmt_determinism(s)?;
                }
            }
            ast::Stmt::While(w) => {
                for s in &w.body {
                    self.check_stmt_determinism(s)?;
                }
            }
            ast::Stmt::For(f) => {
                for s in &f.body {
                    self.check_stmt_determinism(s)?;
                }
            }
            _ => {}
        }
        Ok(())
    }


    fn lower_function(&mut self, func: &ast::StmtFunctionDef) -> Result<IRFunction> {
        let mut params = Vec::new();
        for arg in &func.args.args {
            params.push(arg.def.arg.to_string());
        }

        let mut body = Vec::new();
        let saved_locals = self.current_locals.clone();
        self.current_locals.clear();

        // Add parameters as locals first (they'll be at indices 0, 1, 2, ...)
        for (idx, param_name) in params.iter().enumerate() {
            self.current_locals.insert(param_name.clone(), idx);
        }

        for stmt in &func.body {
            body.push(self.lower_stmt(stmt)?);
        }

        // Build locals vec: params first, then other locals (sorted for determinism)
        let mut locals = params.clone();
        let mut other_locals: Vec<String> = self.current_locals.keys()
            .filter(|k| !params.contains(k))
            .cloned()
            .collect();
        other_locals.sort(); // Sort for determinism
        locals.extend(other_locals);

        // Build local map
        let local_map: HashMap<String, u32> = locals.iter()
            .enumerate()
            .map(|(i, name)| (name.clone(), i as u32))
            .collect();

        let temp_count = SCRATCH_LOCALS as usize;

        if locals.len() + temp_count > MAX_LOCALS {
            bail!("Function '{}' has too many locals: {} + {} scratch", func.name, locals.len(), temp_count);
        }

        self.current_locals = saved_locals;

        Ok(IRFunction {
            name: func.name.to_string(),
            _params: params,
            locals,
            local_map,
            temp_locals: temp_count as u32,
            body,
        })
    }

    fn lower_stmt(&mut self, stmt: &ast::Stmt) -> Result<IRStmt> {
        match stmt {
            ast::Stmt::Assign(assign) => {
                if assign.targets.len() != 1 {
                    bail!("Multiple assignment not supported");
                }

                // Handle tuple unpacking: a, b = expr1, expr2
                if let ast::Expr::Tuple(tuple) = &assign.targets[0] {
                    let ast::Expr::Tuple(values) = &*assign.value else {
                        bail!("Tuple unpacking requires tuple value");
                    };

                    if tuple.elts.len() != values.elts.len() {
                        bail!("Tuple unpacking size mismatch");
                    }

                    let mut stmts = Vec::new();
                    for (target, value) in tuple.elts.iter().zip(values.elts.iter()) {
                        let ast::Expr::Name(name) = target else {
                            bail!("Tuple unpacking target must be variable");
                        };
                        let var_name = name.id.to_string();
                        let len = self.current_locals.len();
                        self.current_locals.entry(var_name.clone()).or_insert(len);

                        let value_expr = self.lower_expr(value)?;
                        stmts.push(IRStmt::Assign { var: var_name, value: value_expr });
                    }

                    return Ok(IRStmt::Block(stmts));
                }

                let ast::Expr::Name(name) = &assign.targets[0] else {
                    bail!("Only simple variable assignment supported");
                };
                let var_name = name.id.to_string();
                let len = self.current_locals.len();
                self.current_locals.entry(var_name.clone()).or_insert(len);

                let value = self.lower_expr(&assign.value)?;
                Ok(IRStmt::Assign { var: var_name, value })
            }
            ast::Stmt::Return(ret) => {
                let value = if let Some(v) = &ret.value {
                    self.lower_expr(v)?
                } else {
                    IRExpr::Const(0)
                };
                Ok(IRStmt::Return(value))
            }
            ast::Stmt::If(if_stmt) => {
                let cond = self.lower_expr(&if_stmt.test)?;
                let then_block = if_stmt.body.iter()
                    .map(|s| self.lower_stmt(s))
                    .collect::<Result<Vec<_>>>()?;
                let else_block = if_stmt.orelse.iter()
                    .map(|s| self.lower_stmt(s))
                    .collect::<Result<Vec<_>>>()?;
                Ok(IRStmt::If { cond, then_block, else_block })
            }
            ast::Stmt::While(while_stmt) => {
                let cond = self.lower_expr(&while_stmt.test)?;
                let body = while_stmt.body.iter()
                    .map(|s| self.lower_stmt(s))
                    .collect::<Result<Vec<_>>>()?;
                Ok(IRStmt::While { cond, body })
            }
            ast::Stmt::For(for_stmt) => {
                let ast::Expr::Name(var) = &*for_stmt.target else {
                    bail!("For loop target must be simple variable");
                };
                let var_name = var.id.to_string();
                let len = self.current_locals.len();
                self.current_locals.entry(var_name.clone()).or_insert(len);

                let iter = match &*for_stmt.iter {
                    ast::Expr::Call(call) => {
                        let ast::Expr::Name(fname) = &*call.func else {
                            bail!("For loop iter must be range()");
                        };
                        if fname.id.as_str() != "range" {
                            bail!("For loop iter must be range(), got {}", fname.id);
                        }

                        if call.args.len() == 1 {
                            self.lower_expr(&call.args[0])?
                        } else {
                            bail!("Only range(n) supported, not range(start, stop)");
                        }
                    }
                    _ => bail!("For loop iter must be range(n)"),
                };

                let body = for_stmt.body.iter()
                    .map(|s| self.lower_stmt(s))
                    .collect::<Result<Vec<_>>>()?;
                Ok(IRStmt::For { var: var_name, iter, body })
            }
            ast::Stmt::Expr(expr) => {
                Ok(IRStmt::Expr(self.lower_expr(&expr.value)?))
            }
            ast::Stmt::Import(_) | ast::Stmt::ImportFrom(_) => {
                // Allow imports, actual functionality handled at runtime
                Ok(IRStmt::Block(vec![]))
            }
            _ => bail!("Unsupported statement type"),
        }
    }

    fn lower_expr(&mut self, expr: &ast::Expr) -> Result<IRExpr> {
        match expr {
            ast::Expr::Constant(c) => {
                match &c.value {
                    ast::Constant::Int(i) => {
                        let val = i.to_string().parse::<i32>()
                            .map_err(|_| anyhow::anyhow!("Integer too large"))?;
                        Ok(IRExpr::Const(val))
                    }
                    ast::Constant::Float(_) => bail!("Float literals not allowed (non-deterministic)"),
                    ast::Constant::Bool(b) => Ok(IRExpr::Const(if *b { 1 } else { 0 })),
                    ast::Constant::None => Ok(IRExpr::Const(0)),
                    ast::Constant::Str(s) => Ok(IRExpr::Str(s.to_string())),
                    _ => bail!("Unsupported constant type"),
                }
            }
            ast::Expr::Name(name) => {
                let var_name = name.id.to_string();
                let len = self.current_locals.len();
                self.current_locals.entry(var_name.clone()).or_insert(len);
                Ok(IRExpr::LoadLocal(var_name))
            }
            ast::Expr::BinOp(binop) => {
                let left = Box::new(self.lower_expr(&binop.left)?);
                let right = Box::new(self.lower_expr(&binop.right)?);
                let op = match binop.op {
                    ast::Operator::Add => BinOp::Add,
                    ast::Operator::Sub => BinOp::Sub,
                    ast::Operator::Mult => BinOp::Mul,
                    ast::Operator::Div => BinOp::Div,
                    ast::Operator::FloorDiv => BinOp::FloorDiv,
                    ast::Operator::Mod => BinOp::Mod,
                    _ => bail!("Unsupported binary operator"),
                };
                Ok(IRExpr::BinOp { op, left, right })
            }
            ast::Expr::Compare(cmp) => {
                if cmp.ops.len() != 1 || cmp.comparators.len() != 1 {
                    bail!("Chained comparisons not supported");
                }
                let left = Box::new(self.lower_expr(&cmp.left)?);
                let right = Box::new(self.lower_expr(&cmp.comparators[0])?);
                let op = match &cmp.ops[0] {
                    ast::CmpOp::Eq => BinOp::Eq,
                    ast::CmpOp::NotEq => BinOp::Ne,
                    ast::CmpOp::Lt => BinOp::Lt,
                    ast::CmpOp::LtE => BinOp::Le,
                    ast::CmpOp::Gt => BinOp::Gt,
                    ast::CmpOp::GtE => BinOp::Ge,
                    _ => bail!("Unsupported comparison operator"),
                };
                Ok(IRExpr::BinOp { op, left, right })
            }
            ast::Expr::UnaryOp(unary) => {
                let operand = Box::new(self.lower_expr(&unary.operand)?);
                let op = match unary.op {
                    ast::UnaryOp::USub => UnaryOp::Neg,
                    ast::UnaryOp::Not => UnaryOp::Not,
                    _ => bail!("Unsupported unary operator"),
                };
                Ok(IRExpr::UnaryOp { op, operand })
            }
            ast::Expr::Call(call) => {
                let ast::Expr::Name(func_name) = &*call.func else {
                    bail!("Only simple function calls supported");
                };
                let fname = func_name.id.to_string();

                if fname == "range" {
                    bail!("range() must be used only in for loops");
                }

                if !self.defined_functions.contains_key(&fname) {
                    bail!("Function '{}' not defined", fname);
                }

                let args = call.args.iter()
                    .map(|a| self.lower_expr(a))
                    .collect::<Result<Vec<_>>>()?;
                Ok(IRExpr::Call {
                    func: fname,
                    args,
                })
            }
            ast::Expr::List(list) => {
                for e in &list.elts {
                    self.validate_int_expr(e)?;
                }
                let items = list.elts.iter()
                    .map(|e| self.lower_expr(e))
                    .collect::<Result<Vec<_>>>()?;
                Ok(IRExpr::List(items))
            }
            ast::Expr::Dict(dict) => {
                for (k_opt, v) in dict.keys.iter().zip(&dict.values) {
                    if let Some(k) = k_opt {
                        self.validate_int_expr(k)?;
                        self.validate_int_expr(v)?;
                    }
                }
                let mut pairs = Vec::new();
                for (k_opt, v) in dict.keys.iter().zip(&dict.values) {
                    if let Some(k) = k_opt {
                        pairs.push((self.lower_expr(k)?, self.lower_expr(v)?));
                    }
                }
                Ok(IRExpr::Dict(pairs))
            }
            ast::Expr::Attribute(attr) => {
                let obj = Box::new(self.lower_expr(&attr.value)?);
                Ok(IRExpr::Attribute {
                    obj,
                    attr: attr.attr.to_string(),
                })
            }
            _ => bail!("Unsupported expression type"),
        }
    }

    fn validate_int_expr(&self, expr: &ast::Expr) -> Result<()> {
        match expr {
            ast::Expr::Constant(c) => {
                match &c.value {
                    ast::Constant::Int(_) | ast::Constant::Bool(_) | ast::Constant::None => Ok(()),
                    _ => bail!("Only int/bool/None allowed in containers"),
                }
            }
            ast::Expr::Name(_) | ast::Expr::BinOp(_) | ast::Expr::Compare(_) => Ok(()),
            _ => bail!("Complex expressions not allowed in containers"),
        }
    }
}

struct WasmCodegen {
    function_indices: BTreeMap<String, u32>,
    gas_global: u32,
    heap_global: u32,
    heap_limit_global: u32,
}

impl WasmCodegen {
    fn new() -> Self {
        Self {
            function_indices: BTreeMap::new(),
            gas_global: 0,
            heap_global: 1,
            heap_limit_global: 2,
        }
    }

    fn generate(&mut self, ir: &IR) -> Result<Vec<u8>> {
        let IR::Module { functions, .. } = ir;

        for (idx, func) in functions.iter().enumerate() {
            self.function_indices.insert(func.name.clone(), idx as u32);
        }

        let mut module = Module::new();

        // Type section - create types for each function based on parameter count
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

        // Track next available scratch local for loops (starts after user locals)
        let mut next_scratch = func.locals.len() as u32;

        for stmt in &func.body {
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
                self.generate_expr(func, value, ir_func, gas_temp_local)?;
                let local_idx = ir_func.local_map.get(var)
                    .ok_or_else(|| anyhow::anyhow!("Variable '{}' not in local_map", var))?;
                func.instruction(&Instruction::LocalSet(*local_idx));
            }
            IRStmt::Return(expr) => {
                self.generate_expr(func, expr, ir_func, gas_temp_local)?;
                func.instruction(&Instruction::Return);
            }
            IRStmt::If { cond, then_block, else_block } => {
                self.generate_expr(func, cond, ir_func, gas_temp_local)?;
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
                self.generate_expr(func, cond, ir_func, gas_temp_local)?;
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

                // Allocate a unique counter for this loop
                let counter = *next_scratch;
                *next_scratch += 1;

                func.instruction(&Instruction::I32Const(0));
                func.instruction(&Instruction::LocalSet(counter));

                func.instruction(&Instruction::Block(BlockType::Empty));
                func.instruction(&Instruction::Loop(BlockType::Empty));
                self.meter_gas(func, 1, gas_temp_local);

                func.instruction(&Instruction::LocalGet(counter));
                self.generate_expr(func, iter, ir_func, gas_temp_local)?;
                func.instruction(&Instruction::I32GeS);
                func.instruction(&Instruction::BrIf(1));

                func.instruction(&Instruction::LocalGet(counter));
                func.instruction(&Instruction::LocalSet(*loop_var));
                for s in body {
                    self.generate_stmt_with_scratch(func, s, ir_func, gas_temp_local, next_scratch)?;
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
                self.generate_expr(func, expr, ir_func, gas_temp_local)?;
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

    fn generate_expr(&mut self, func: &mut Function, expr: &IRExpr, ir_func: &IRFunction, gas_temp_local: u32) -> Result<()> {
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
                        // 0 - operand
                        func.instruction(&Instruction::I32Const(0));
                        self.generate_expr(func, operand, ir_func, gas_temp_local)?;
                        func.instruction(&Instruction::I32Sub);
                    }
                    UnaryOp::Not => {
                        self.generate_expr(func, operand, ir_func, gas_temp_local)?;
                        func.instruction(&Instruction::I32Eqz);
                    }
                }
            }
            IRExpr::BinOp { op, left, right } => {
                self.generate_expr(func, left, ir_func, gas_temp_local)?;
                self.generate_expr(func, right, ir_func, gas_temp_local)?;

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
                            BinOp::Add => Instruction::I32Add,
                            BinOp::Sub => Instruction::I32Sub,
                            BinOp::Mul => Instruction::I32Mul,
                            BinOp::Eq => Instruction::I32Eq,
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
            IRExpr::Call { func: fname, args } => {
                for arg in args {
                    self.generate_expr(func, arg, ir_func, gas_temp_local)?;
                }
                let func_idx = self.function_indices.get(fname)
                    .ok_or_else(|| anyhow::anyhow!("Function '{}' not found", fname))?;
                func.instruction(&Instruction::Call(*func_idx));
            }
            IRExpr::List(items) => {
                let size = (1 + items.len()) * 4;
                let scratch = ir_func.locals.len() as u32;

                self.meter_gas(func, items.len() as i32 * 2, gas_temp_local);

                func.instruction(&Instruction::GlobalGet(self.heap_global));
                func.instruction(&Instruction::I32Const(size as i32));
                func.instruction(&Instruction::I32Add);
                func.instruction(&Instruction::GlobalGet(self.heap_limit_global));
                func.instruction(&Instruction::I32GtS);
                func.instruction(&Instruction::If(BlockType::Empty));
                func.instruction(&Instruction::Unreachable);
                func.instruction(&Instruction::End);

                func.instruction(&Instruction::GlobalGet(self.heap_global));
                func.instruction(&Instruction::LocalTee(scratch));

                func.instruction(&Instruction::I32Const(items.len() as i32));
                func.instruction(&Instruction::I32Store(MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }));

                for (i, item) in items.iter().enumerate() {
                    func.instruction(&Instruction::LocalGet(scratch));
                    self.generate_expr(func, item, ir_func, gas_temp_local)?;
                    func.instruction(&Instruction::I32Store(MemArg {
                        offset: ((i + 1) * 4) as u64,
                        align: 2,
                        memory_index: 0,
                    }));
                }

                func.instruction(&Instruction::GlobalGet(self.heap_global));
                func.instruction(&Instruction::I32Const(size as i32));
                func.instruction(&Instruction::I32Add);
                func.instruction(&Instruction::GlobalSet(self.heap_global));

                func.instruction(&Instruction::LocalGet(scratch));
            }
            IRExpr::Dict(pairs) => {
                let size = (1 + pairs.len() * 2) * 4;
                let scratch = ir_func.locals.len() as u32;

                self.meter_gas(func, pairs.len() as i32 * 4, gas_temp_local);

                func.instruction(&Instruction::GlobalGet(self.heap_global));
                func.instruction(&Instruction::I32Const(size as i32));
                func.instruction(&Instruction::I32Add);
                func.instruction(&Instruction::GlobalGet(self.heap_limit_global));
                func.instruction(&Instruction::I32GtS);
                func.instruction(&Instruction::If(BlockType::Empty));
                func.instruction(&Instruction::Unreachable);
                func.instruction(&Instruction::End);

                func.instruction(&Instruction::GlobalGet(self.heap_global));
                func.instruction(&Instruction::LocalTee(scratch));

                func.instruction(&Instruction::I32Const(pairs.len() as i32));
                func.instruction(&Instruction::I32Store(MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }));

                for (i, (key, val)) in pairs.iter().enumerate() {
                    func.instruction(&Instruction::LocalGet(scratch));
                    self.generate_expr(func, key, ir_func, gas_temp_local)?;
                    func.instruction(&Instruction::I32Store(MemArg {
                        offset: ((i * 2 + 1) * 4) as u64,
                        align: 2,
                        memory_index: 0,
                    }));

                    func.instruction(&Instruction::LocalGet(scratch));
                    self.generate_expr(func, val, ir_func, gas_temp_local)?;
                    func.instruction(&Instruction::I32Store(MemArg {
                        offset: ((i * 2 + 2) * 4) as u64,
                        align: 2,
                        memory_index: 0,
                    }));
                }

                func.instruction(&Instruction::GlobalGet(self.heap_global));
                func.instruction(&Instruction::I32Const(size as i32));
                func.instruction(&Instruction::I32Add);
                func.instruction(&Instruction::GlobalSet(self.heap_global));

                func.instruction(&Instruction::LocalGet(scratch));
            }
            IRExpr::Subscript { .. } => {
                bail!("Subscript operations require runtime support (not yet implemented)")
            }
            IRExpr::Attribute { .. } => {
                bail!("Attribute access requires runtime support (not yet implemented)")
            }
            IRExpr::Str(_) => {
                bail!("String literals require runtime support (not yet implemented)")
            }
        }
        Ok(())
    }
}

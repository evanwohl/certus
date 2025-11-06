use anyhow::{Result, bail};
use wasm_encoder::*;
use std::collections::{HashMap, BTreeMap};
use std::sync::Arc;
use sha2::{Sha256, Digest};
use rustpython_parser::{self as parser, ast};

const MAX_LOCALS: usize = 256;
const MAX_PYTHON_SIZE: usize = 100 * 1024;

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
}

#[derive(Debug, Clone)]
pub enum IRExpr {
    Const(i32),
    LoadLocal(String),
    BinOp { op: BinOp, left: Box<IRExpr>, right: Box<IRExpr> },
    Call { func: String, args: Vec<IRExpr> },
    List(Vec<IRExpr>),
    Dict(Vec<(IRExpr, IRExpr)>),
}

#[derive(Debug, Clone)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod,
    Eq, Ne, Lt, Le, Gt, Ge,
}

// Lowers Python AST to intermediate representation
struct IRLowering {
    current_locals: BTreeMap<String, usize>,
}

impl IRLowering {
    fn new() -> Self {
        Self {
            current_locals: BTreeMap::new(),
        }
    }

    fn lower_module(&mut self, module: &ast::Mod) -> Result<IR> {
        let ast::Mod::Module(ast::ModModule { body, .. }) = module else {
            bail!("Only module-level Python supported");
        };

        self.validate_determinism(body)?;

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

        let temp_count = self.count_temps(&main_body);

        if main_locals.len() + temp_count > MAX_LOCALS {
            bail!("Too many locals: {} + {} temps exceeds limit", main_locals.len(), temp_count);
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
                bail!("Imports not allowed (use builtins only)");
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

    fn count_temps(&self, stmts: &[IRStmt]) -> usize {
        let mut max_temps = 0;
        for stmt in stmts {
            max_temps = max_temps.max(self.count_stmt_temps(stmt));
        }
        max_temps
    }

    fn count_stmt_temps(&self, stmt: &IRStmt) -> usize {
        match stmt {
            IRStmt::For { body, .. } => 1 + self.count_temps(body),
            IRStmt::If { then_block, else_block, .. } => {
                self.count_temps(then_block).max(self.count_temps(else_block))
            }
            IRStmt::While { body, .. } => self.count_temps(body),
            _ => 0,
        }
    }

    fn lower_function(&mut self, func: &ast::StmtFunctionDef) -> Result<IRFunction> {
        let mut params = Vec::new();
        for arg in &func.args.args {
            params.push(arg.def.arg.to_string());
        }

        let mut body = Vec::new();
        let saved_locals = self.current_locals.clone();
        self.current_locals.clear();

        for stmt in &func.body {
            body.push(self.lower_stmt(stmt)?);
        }

        let locals: Vec<String> = self.current_locals.keys().cloned().collect();
        let local_map: HashMap<String, u32> = locals.iter()
            .enumerate()
            .map(|(i, name)| (name.clone(), i as u32))
            .collect();

        let temp_count = self.count_temps(&body);

        if locals.len() + temp_count > MAX_LOCALS {
            bail!("Function '{}' has too many locals: {} + {} temps", func.name, locals.len(), temp_count);
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

                let iter = self.lower_expr(&for_stmt.iter)?;
                let body = for_stmt.body.iter()
                    .map(|s| self.lower_stmt(s))
                    .collect::<Result<Vec<_>>>()?;
                Ok(IRStmt::For { var: var_name, iter, body })
            }
            ast::Stmt::Expr(expr) => {
                Ok(IRStmt::Expr(self.lower_expr(&expr.value)?))
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
                    ast::Constant::Bool(b) => Ok(IRExpr::Const(if *b { 1 } else { 0 })),
                    ast::Constant::None => Ok(IRExpr::Const(0)),
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
                    ast::Operator::Div | ast::Operator::FloorDiv => BinOp::Div,
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
            ast::Expr::Call(call) => {
                let ast::Expr::Name(func_name) = &*call.func else {
                    bail!("Only simple function calls supported");
                };
                let args = call.args.iter()
                    .map(|a| self.lower_expr(a))
                    .collect::<Result<Vec<_>>>()?;
                Ok(IRExpr::Call {
                    func: func_name.id.to_string(),
                    args,
                })
            }
            ast::Expr::List(list) => {
                let items = list.elts.iter()
                    .map(|e| self.lower_expr(e))
                    .collect::<Result<Vec<_>>>()?;
                Ok(IRExpr::List(items))
            }
            ast::Expr::Dict(dict) => {
                let mut pairs = Vec::new();
                for (k_opt, v) in dict.keys.iter().zip(&dict.values) {
                    if let Some(k) = k_opt {
                        pairs.push((self.lower_expr(k)?, self.lower_expr(v)?));
                    }
                }
                Ok(IRExpr::Dict(pairs))
            }
            _ => bail!("Unsupported expression type"),
        }
    }
}

// Generates WebAssembly bytecode from IR
struct WasmCodegen {
    function_indices: BTreeMap<String, u32>,
    heap_offset_global: u32,
}

impl WasmCodegen {
    fn new() -> Self {
        Self {
            function_indices: BTreeMap::new(),
            heap_offset_global: 1,
        }
    }

    fn generate(&mut self, ir: &IR) -> Result<Vec<u8>> {
        let IR::Module { functions, .. } = ir;

        for (idx, func) in functions.iter().enumerate() {
            self.function_indices.insert(func.name.clone(), idx as u32);
        }

        let mut module = Module::new();

        // Type section
        let mut types = TypeSection::new();
        types.function(vec![ValType::I32; 10], vec![ValType::I32]);
        module.section(&types);

        // Function section
        let mut funcs = FunctionSection::new();
        for _ in functions {
            funcs.function(0);
        }
        module.section(&funcs);

        // Memory section
        let mut memory = MemorySection::new();
        memory.memory(MemoryType {
            minimum: 16,
            maximum: Some(256),
            memory64: false,
            shared: false,
        });
        module.section(&memory);

        // Global section: gas counter and heap pointer
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
            &ConstExpr::i32_const(0x10000),
        );
        module.section(&globals);

        // Export section
        let mut exports = ExportSection::new();
        exports.export("memory", ExportKind::Memory, 0);
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
        let total_locals = func.locals.len() as u32 + func.temp_locals;
        let mut wasm_func = Function::new(vec![(total_locals, ValType::I32)]);

        wasm_func.instruction(&Instruction::GlobalGet(0));
        wasm_func.instruction(&Instruction::I32Const(100));
        wasm_func.instruction(&Instruction::I32Add);
        wasm_func.instruction(&Instruction::GlobalSet(0));

        for stmt in &func.body {
            self.generate_stmt(&mut wasm_func, stmt, func)?;
        }

        wasm_func.instruction(&Instruction::I32Const(0));
        wasm_func.instruction(&Instruction::End);

        Ok(wasm_func)
    }

    fn generate_stmt(&mut self, func: &mut Function, stmt: &IRStmt, ir_func: &IRFunction) -> Result<()> {
        match stmt {
            IRStmt::Assign { var, value } => {
                self.generate_expr(func, value, ir_func)?;
                let local_idx = ir_func.local_map.get(var)
                    .ok_or_else(|| anyhow::anyhow!("Variable '{}' not in local_map", var))?;
                func.instruction(&Instruction::LocalSet(*local_idx));
            }
            IRStmt::Return(expr) => {
                self.generate_expr(func, expr, ir_func)?;
                func.instruction(&Instruction::Return);
            }
            IRStmt::If { cond, then_block, else_block } => {
                self.generate_expr(func, cond, ir_func)?;
                func.instruction(&Instruction::If(BlockType::Empty));
                for s in then_block {
                    self.generate_stmt(func, s, ir_func)?;
                }
                if !else_block.is_empty() {
                    func.instruction(&Instruction::Else);
                    for s in else_block {
                        self.generate_stmt(func, s, ir_func)?;
                    }
                }
                func.instruction(&Instruction::End);
            }
            IRStmt::While { cond, body } => {
                func.instruction(&Instruction::Block(BlockType::Empty));
                func.instruction(&Instruction::Loop(BlockType::Empty));
                self.generate_expr(func, cond, ir_func)?;
                func.instruction(&Instruction::I32Eqz);
                func.instruction(&Instruction::BrIf(1));
                for s in body {
                    self.generate_stmt(func, s, ir_func)?;
                }
                func.instruction(&Instruction::Br(0));
                func.instruction(&Instruction::End);
                func.instruction(&Instruction::End);
            }
            IRStmt::For { var, iter, body } => {
                let loop_var = ir_func.local_map.get(var)
                    .ok_or_else(|| anyhow::anyhow!("Loop variable '{}' not in local_map", var))?;
                let counter = ir_func.locals.len() as u32;

                func.instruction(&Instruction::I32Const(0));
                func.instruction(&Instruction::LocalSet(counter));

                func.instruction(&Instruction::Block(BlockType::Empty));
                func.instruction(&Instruction::Loop(BlockType::Empty));

                func.instruction(&Instruction::LocalGet(counter));
                self.generate_expr(func, iter, ir_func)?;
                func.instruction(&Instruction::I32GeU);
                func.instruction(&Instruction::BrIf(1));

                func.instruction(&Instruction::LocalGet(counter));
                func.instruction(&Instruction::LocalSet(*loop_var));
                for s in body {
                    self.generate_stmt(func, s, ir_func)?;
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
                self.generate_expr(func, expr, ir_func)?;
                func.instruction(&Instruction::Drop);
            }
        }
        Ok(())
    }

    fn generate_expr(&mut self, func: &mut Function, expr: &IRExpr, ir_func: &IRFunction) -> Result<()> {
        match expr {
            IRExpr::Const(val) => {
                func.instruction(&Instruction::I32Const(*val));
            }
            IRExpr::LoadLocal(var) => {
                let idx = ir_func.local_map.get(var)
                    .ok_or_else(|| anyhow::anyhow!("Variable '{}' not in local_map", var))?;
                func.instruction(&Instruction::LocalGet(*idx));
            }
            IRExpr::BinOp { op, left, right } => {
                self.generate_expr(func, left, ir_func)?;
                self.generate_expr(func, right, ir_func)?;

                match op {
                    BinOp::Div | BinOp::Mod => {
                        func.instruction(&Instruction::LocalTee(ir_func.locals.len() as u32));
                        func.instruction(&Instruction::I32Eqz);
                        func.instruction(&Instruction::If(BlockType::Empty));
                        func.instruction(&Instruction::Unreachable);
                        func.instruction(&Instruction::End);
                        func.instruction(&Instruction::LocalGet(ir_func.locals.len() as u32));

                        let instr = match op {
                            BinOp::Div => Instruction::I32DivS,
                            BinOp::Mod => Instruction::I32RemS,
                            _ => unreachable!(),
                        };
                        func.instruction(&instr);
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
                    self.generate_expr(func, arg, ir_func)?;
                }
                let func_idx = self.function_indices.get(fname)
                    .copied()
                    .unwrap_or(0);
                func.instruction(&Instruction::Call(func_idx));
            }
            IRExpr::List(items) => {
                let size = (1 + items.len()) * 4;

                func.instruction(&Instruction::GlobalGet(self.heap_offset_global));
                let list_ptr_local = ir_func.locals.len() as u32 + 1;
                func.instruction(&Instruction::LocalTee(list_ptr_local));

                func.instruction(&Instruction::I32Const(items.len() as i32));
                func.instruction(&Instruction::I32Store(MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }));

                for (i, item) in items.iter().enumerate() {
                    func.instruction(&Instruction::LocalGet(list_ptr_local));
                    self.generate_expr(func, item, ir_func)?;
                    func.instruction(&Instruction::I32Store(MemArg {
                        offset: ((i + 1) * 4) as u64,
                        align: 2,
                        memory_index: 0,
                    }));
                }

                func.instruction(&Instruction::GlobalGet(self.heap_offset_global));
                func.instruction(&Instruction::I32Const(size as i32));
                func.instruction(&Instruction::I32Add);
                func.instruction(&Instruction::GlobalSet(self.heap_offset_global));

                func.instruction(&Instruction::LocalGet(list_ptr_local));
            }
            IRExpr::Dict(pairs) => {
                let size = (1 + pairs.len() * 2) * 4;

                func.instruction(&Instruction::GlobalGet(self.heap_offset_global));
                let dict_ptr_local = ir_func.locals.len() as u32 + 1;
                func.instruction(&Instruction::LocalTee(dict_ptr_local));

                func.instruction(&Instruction::I32Const(pairs.len() as i32));
                func.instruction(&Instruction::I32Store(MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }));

                for (i, (key, val)) in pairs.iter().enumerate() {
                    func.instruction(&Instruction::LocalGet(dict_ptr_local));
                    self.generate_expr(func, key, ir_func)?;
                    func.instruction(&Instruction::I32Store(MemArg {
                        offset: ((i * 2 + 1) * 4) as u64,
                        align: 2,
                        memory_index: 0,
                    }));

                    func.instruction(&Instruction::LocalGet(dict_ptr_local));
                    self.generate_expr(func, val, ir_func)?;
                    func.instruction(&Instruction::I32Store(MemArg {
                        offset: ((i * 2 + 2) * 4) as u64,
                        align: 2,
                        memory_index: 0,
                    }));
                }

                func.instruction(&Instruction::GlobalGet(self.heap_offset_global));
                func.instruction(&Instruction::I32Const(size as i32));
                func.instruction(&Instruction::I32Add);
                func.instruction(&Instruction::GlobalSet(self.heap_offset_global));

                func.instruction(&Instruction::LocalGet(dict_ptr_local));
            }
        }
        Ok(())
    }
}

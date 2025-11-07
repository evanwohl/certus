use anyhow::{Result, bail};
use std::collections::{HashMap, BTreeMap};
use rustpython_parser::ast;

use super::ir::*;

const MAX_LOCALS: usize = 256;
const SCRATCH_LOCALS: u32 = 32;

pub(crate) struct IRLowering {
    current_locals: BTreeMap<String, usize>,
    defined_functions: BTreeMap<String, bool>,
}

impl IRLowering {
    pub fn new() -> Self {
        Self {
            current_locals: BTreeMap::new(),
            defined_functions: BTreeMap::new(),
        }
    }

    pub fn lower_module(&mut self, module: &ast::Mod) -> Result<IR> {
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
                    ast::Constant::Str(_) => bail!("String literals require runtime support"),
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
                // Lower list literal to IR
                let elements = list.elts.iter()
                    .map(|e| self.lower_expr(e))
                    .collect::<Result<Vec<_>>>()?;
                Ok(IRExpr::List(elements))
            }
            ast::Expr::Dict(dict) => {
                // Lower dict literal to IR
                if dict.keys.len() != dict.values.len() {
                    bail!("Dict keys/values length mismatch");
                }
                let pairs = dict.keys.iter()
                    .zip(dict.values.iter())
                    .map(|(k_opt, v)| {
                        let k = k_opt.as_ref().ok_or_else(|| anyhow::anyhow!("Dict **expansion not supported"))?;
                        Ok((self.lower_expr(k)?, self.lower_expr(v)?))
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(IRExpr::Dict(pairs))
            }
            ast::Expr::Subscript(sub) => {
                // Lower subscript to IR
                let value = Box::new(self.lower_expr(&sub.value)?);
                let index = Box::new(self.lower_expr(&sub.slice)?);
                Ok(IRExpr::Subscript { value, index })
            }
            ast::Expr::Attribute(_) => bail!("Attribute access requires runtime support"),
            _ => bail!("Unsupported expression type"),
        }
    }

}

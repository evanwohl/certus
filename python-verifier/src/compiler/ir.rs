use std::collections::HashMap;

// Top-level module representation
#[derive(Debug, Clone)]
pub enum IR {
    Module {
        functions: Vec<IRFunction>,
        _globals: Vec<String>,
    },
}

// Function with explicit local management for deterministic code generation
#[derive(Debug, Clone)]
pub struct IRFunction {
    pub name: String,
    pub _params: Vec<String>,
    pub locals: Vec<String>,
    pub local_map: HashMap<String, u32>,
    pub temp_locals: u32,
    pub body: Vec<IRStmt>,
}

// Statements (have side effects: assignments, control flow)
#[derive(Debug, Clone)]
pub enum IRStmt {
    Assign { var: String, value: IRExpr },
    SubscriptAssign { target: Box<IRExpr>, index: Box<IRExpr>, value: Box<IRExpr> },
    Return(IRExpr),
    If { cond: IRExpr, then_block: Vec<IRStmt>, else_block: Vec<IRStmt> },
    While { cond: IRExpr, body: Vec<IRStmt> },
    For { var: String, iter: IRExpr, body: Vec<IRStmt> },
    Expr(IRExpr),
    Block(Vec<IRStmt>),
}

// Expressions (pure: always return a value, no side effects)
#[derive(Debug, Clone)]
pub enum IRExpr {
    Const(i32),
    Str(String),
    LoadLocal(String),
    BinOp { op: BinOp, left: Box<IRExpr>, right: Box<IRExpr> },
    UnaryOp { op: UnaryOp, operand: Box<IRExpr> },
    Call { func: String, args: Vec<IRExpr> },
    // Deterministic collections using linear memory
    List(Vec<IRExpr>),           // List literal: [1, 2, 3]
    Dict(Vec<(IRExpr, IRExpr)>), // Dict literal: {1: 2, 3: 4}
    Subscript {                  // Subscript: x[i]
        value: Box<IRExpr>,
        index: Box<IRExpr>,
    },
    // String slice: s[start:end]
    Slice {
        value: Box<IRExpr>,
        start: Option<Box<IRExpr>>,
        end: Option<Box<IRExpr>>,
    },
    // Conditional expression: value_if_true if condition else value_if_false
    IfExpr {
        cond: Box<IRExpr>,
        then_val: Box<IRExpr>,
        else_val: Box<IRExpr>,
    },
}

// Binary operators (arithmetic and comparison)
#[derive(Debug, Clone)]
pub enum BinOp {
    Add, Sub, Mul, Div, FloorDiv, Mod,
    Eq, Ne, Lt, Le, Gt, Ge,
}

// Unary operators
#[derive(Debug, Clone)]
pub enum UnaryOp {
    Neg,    // -x
    Not,    // not x
}

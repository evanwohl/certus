use anyhow::{Result, bail};
use std::collections::HashMap;
use std::sync::Arc;
use sha2::{Sha256, Digest};
use rustpython_parser::{self as parser, ast};

mod ir;
mod lowering;
mod codegen;
mod memory;

use ir::IR;
use lowering::IRLowering;
use codegen::WasmCodegen;

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

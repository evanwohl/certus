use anyhow::{Result, bail};
use serde_json::Value;

/// Python code validation for deterministic execution
pub struct PythonValidator;

impl PythonValidator {
    /// Validate Python code meets determinism requirements
    pub fn validate_code(code: &str) -> Result<()> {
        if code.is_empty() {
            bail!("empty code");
        }

        if code.len() > 100_000 {
            bail!("exceeds 100KB limit");
        }

        if !code.contains("OUTPUT") {
            bail!("missing OUTPUT variable");
        }

        Self::check_forbidden_operations(code)?;
        Self::check_syntax(code)?;

        Ok(())
    }

    /// Check for non-deterministic operations
    fn check_forbidden_operations(code: &str) -> Result<()> {
        const FORBIDDEN: &[&str] = &[
            "__import__",
            "compile(",
            "eval(",
            "exec(",
            "execfile(",
            "globals(",
            "locals(",
            "vars(",
            "dir(",
            "input(",
            "raw_input(",
            "open(",
            "file(",
            "subprocess",
            "os.",
            "sys.",
            "socket",
            "urllib",
            "requests",
            "http.",
            "ftplib",
            "telnetlib",
            "pickle",
            "marshal",
            "shelve",
            "__builtins__",
            "__loader__",
            "__spec__",
            "exit(",
            "quit(",
        ];

        for forbidden in FORBIDDEN {
            if code.contains(forbidden) {
                bail!("forbidden operation: {}", forbidden);
            }
        }

        Ok(())
    }

    /// Syntax validation
    fn check_syntax(code: &str) -> Result<()> {
        let mut paren_depth = 0;
        let mut bracket_depth = 0;
        let mut brace_depth = 0;
        let mut in_string = false;
        let mut string_char = ' ';

        for ch in code.chars() {
            if in_string {
                if ch == string_char && code.chars().nth(code.len() - 1) != Some('\\') {
                    in_string = false;
                }
                continue;
            }

            match ch {
                '"' | '\'' => {
                    in_string = true;
                    string_char = ch;
                }
                '(' => paren_depth += 1,
                ')' => {
                    paren_depth -= 1;
                    if paren_depth < 0 {
                        bail!("unmatched parenthesis");
                    }
                }
                '[' => bracket_depth += 1,
                ']' => {
                    bracket_depth -= 1;
                    if bracket_depth < 0 {
                        bail!("unmatched bracket");
                    }
                }
                '{' => brace_depth += 1,
                '}' => {
                    brace_depth -= 1;
                    if brace_depth < 0 {
                        bail!("unmatched brace");
                    }
                }
                _ => {}
            }
        }

        if paren_depth != 0 {
            bail!("unclosed parenthesis");
        }
        if bracket_depth != 0 {
            bail!("unclosed bracket");
        }
        if brace_depth != 0 {
            bail!("unclosed brace");
        }

        Ok(())
    }
}

/// Validate JSON input
pub fn validate_json_input(input: &str) -> Result<Value> {
    if input.is_empty() {
        bail!("input cannot be empty");
    }

    if input.len() > 100_000 {
        bail!("input exceeds 100KB");
    }

    let value: Value = serde_json::from_str(input)?;

    // check for no null values
    check_no_nulls(&value)?;

    Ok(value)
}

/// Recursively check for null values
fn check_no_nulls(value: &Value) -> Result<()> {
    match value {
        Value::Null => bail!("null values not allowed"),
        Value::Array(arr) => {
            for v in arr {
                check_no_nulls(v)?;
            }
        }
        Value::Object(obj) => {
            for (_k, v) in obj {
                check_no_nulls(v)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Validate execution output
pub fn validate_output(output: &str) -> Result<()> {
    if output.is_empty() {
        bail!("output cannot be empty");
    }

    if output.len() > 1_000_000 {
        bail!("output exceeds 1MB");
    }

    // ensure it's valid JSON or string
    if output.starts_with('{') || output.starts_with('[') {
        let _: Value = serde_json::from_str(output)?;
    }

    Ok(())
}
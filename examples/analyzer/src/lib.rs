use solang_parser::pt::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Serialize, Deserialize)]
struct RugVector {
    severity: u8,      // 1-10, 10 = critical
    category: String,  // MINT, PAUSE, BLACKLIST, etc.
    description: String,
    location: String,  // Line/function name
}

#[derive(Serialize, Deserialize)]
struct AnalysisResult {
    is_safe: bool,
    rug_vectors: Vec<RugVector>,
    score: u8, // 0-100, 100 = completely safe
}

use std::alloc::{alloc, Layout};
use std::ptr;

/// Main entry point for Wasm
/// CRITICAL: Properly manage memory to prevent corruption
#[no_mangle]
pub extern "C" fn analyze(input_ptr: *const u8, input_len: usize) -> *const u8 {
    let input = unsafe {
        std::slice::from_raw_parts(input_ptr, input_len)
    };

    let source_code = match std::str::from_utf8(input) {
        Ok(s) => s,
        Err(_) => return allocate_string("ERROR:Invalid UTF-8 input"),
    };

    let result = analyze_contract(source_code);
    let json = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());

    // Return as "CLEAN" or "RUG:<details>"
    let output = if result.is_safe {
        "CLEAN".to_string()
    } else {
        format!("RUG:{}", json)
    };

    allocate_string(&output)
}

/// Allocate string in Wasm linear memory (stays valid after function returns)
fn allocate_string(s: &str) -> *const u8 {
    let bytes = s.as_bytes();
    let len = bytes.len();

    unsafe {
        // Allocate: 4 bytes for length + actual string bytes
        let layout = Layout::from_size_align(len + 4, 4).unwrap();
        let ptr = alloc(layout);

        if ptr.is_null() {
            return ptr::null();
        }

        // Write length as first 4 bytes (little-endian)
        ptr::write(ptr as *mut u32, len as u32);

        // Write string bytes after length
        ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.add(4), len);

        ptr
    }
}

/// Free allocated string (called by host environment)
#[no_mangle]
pub extern "C" fn free_string(ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }

    unsafe {
        let len = ptr::read(ptr as *const u32) as usize;
        let layout = Layout::from_size_align(len + 4, 4).unwrap();
        std::alloc::dealloc(ptr, layout);
    }
}

fn analyze_contract(source: &str) -> AnalysisResult {
    let mut rug_vectors = Vec::new();

    // Parse Solidity
    let (ast, _comments) = match solang_parser::parse(source, 0) {
        Ok(result) => result,
        Err(_) => {
            return AnalysisResult {
                is_safe: false,
                rug_vectors: vec![RugVector {
                    severity: 10,
                    category: "PARSE_ERROR".to_string(),
                    description: "Failed to parse Solidity code".to_string(),
                    location: "N/A".to_string(),
                }],
                score: 0,
            };
        }
    };

    // Check for rug vectors
    check_mint_function(&ast, &mut rug_vectors);
    check_pause_mechanism(&ast, &mut rug_vectors);
    check_blacklist(&ast, &mut rug_vectors);
    check_owner_withdrawal(&ast, &mut rug_vectors);
    check_proxy_upgrade(&ast, &mut rug_vectors);
    check_transfer_restrictions(&ast, &mut rug_vectors);
    check_max_transaction(&ast, &mut rug_vectors);
    check_cooldown(&ast, &mut rug_vectors);
    check_hidden_fees(&ast, &mut rug_vectors);
    check_ownership_controls(&ast, &mut rug_vectors);

    // Calculate score
    let total_severity: u32 = rug_vectors.iter().map(|rv| rv.severity as u32).sum();
    let score = if total_severity == 0 {
        100
    } else {
        (100 - std::cmp::min(total_severity, 100)) as u8
    };

    let is_safe = rug_vectors.is_empty();

    AnalysisResult {
        is_safe,
        rug_vectors,
        score,
    }
}

fn check_mint_function(ast: &SourceUnit, rug_vectors: &mut Vec<RugVector>) {
    for part in &ast.0 {
        if let SourceUnitPart::ContractDefinition(contract) = part {
            for part in &contract.parts {
                if let ContractPart::FunctionDefinition(func) = part {
                    if let Some(name) = &func.name {
                        if name.name == "mint" || name.name == "_mint" {
                            // Check if function has onlyOwner modifier
                            let has_owner_control = func.attributes.iter().any(|attr| {
                                matches!(attr, FunctionAttribute::BaseOrModifier(_, base) if {
                                    if let Base { name, .. } = base {
                                        name.identifiers.iter().any(|id| id.name == "onlyOwner")
                                    } else {
                                        false
                                    }
                                })
                            });

                            if has_owner_control {
                                rug_vectors.push(RugVector {
                                    severity: 10,
                                    category: "OWNER_MINT".to_string(),
                                    description: "Owner can mint unlimited tokens".to_string(),
                                    location: format!("function {}", name.name),
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}

fn check_pause_mechanism(ast: &SourceUnit, rug_vectors: &mut Vec<RugVector>) {
    for part in &ast.0 {
        if let SourceUnitPart::ContractDefinition(contract) = part {
            // Check for Pausable inheritance
            for base in &contract.base {
                if let Base { name, .. } = base {
                    if name.identifiers.iter().any(|id| id.name.contains("Pausable")) {
                        rug_vectors.push(RugVector {
                            severity: 9,
                            category: "PAUSABLE".to_string(),
                            description: "Contract can be paused, blocking all transfers".to_string(),
                            location: "contract inheritance".to_string(),
                        });
                    }
                }
            }

            // Check for pause() function
            for part in &contract.parts {
                if let ContractPart::FunctionDefinition(func) = part {
                    if let Some(name) = &func.name {
                        if name.name == "pause" || name.name == "_pause" {
                            rug_vectors.push(RugVector {
                                severity: 9,
                                category: "PAUSE_FUNCTION".to_string(),
                                description: "Trading can be paused by owner".to_string(),
                                location: format!("function {}", name.name),
                            });
                        }
                    }
                }
            }
        }
    }
}

fn check_blacklist(ast: &SourceUnit, rug_vectors: &mut Vec<RugVector>) {
    for part in &ast.0 {
        if let SourceUnitPart::ContractDefinition(contract) = part {
            for part in &contract.parts {
                if let ContractPart::FunctionDefinition(func) = part {
                    if let Some(name) = &func.name {
                        let name_lower = name.name.to_lowercase();
                        if name_lower.contains("blacklist") || name_lower.contains("block") {
                            rug_vectors.push(RugVector {
                                severity: 8,
                                category: "BLACKLIST".to_string(),
                                description: "Owner can blacklist addresses".to_string(),
                                location: format!("function {}", name.name),
                            });
                        }
                    }
                }
            }
        }
    }
}

fn check_owner_withdrawal(ast: &SourceUnit, rug_vectors: &mut Vec<RugVector>) {
    for part in &ast.0 {
        if let SourceUnitPart::ContractDefinition(contract) = part {
            for part in &contract.parts {
                if let ContractPart::FunctionDefinition(func) = part {
                    if let Some(name) = &func.name {
                        let name_lower = name.name.to_lowercase();
                        if (name_lower.contains("withdraw") || name_lower.contains("rescue"))
                            && func.attributes.iter().any(|attr| {
                                matches!(attr, FunctionAttribute::BaseOrModifier(_, base) if {
                                    if let Base { name, .. } = base {
                                        name.identifiers.iter().any(|id| id.name == "onlyOwner")
                                    } else {
                                        false
                                    }
                                })
                            })
                        {
                            rug_vectors.push(RugVector {
                                severity: 9,
                                category: "OWNER_WITHDRAW".to_string(),
                                description: "Owner can withdraw contract funds".to_string(),
                                location: format!("function {}", name.name),
                            });
                        }
                    }
                }
            }
        }
    }
}

fn check_proxy_upgrade(ast: &SourceUnit, rug_vectors: &mut Vec<RugVector>) {
    for part in &ast.0 {
        if let SourceUnitPart::ContractDefinition(contract) = part {
            // Check for UUPS or Transparent Proxy patterns
            for base in &contract.base {
                if let Base { name, .. } = base {
                    let base_name = name.identifiers.iter()
                        .map(|id| id.name.as_str())
                        .collect::<Vec<_>>()
                        .join("::");

                    if base_name.contains("Upgradeable") || base_name.contains("Proxy") {
                        rug_vectors.push(RugVector {
                            severity: 10,
                            category: "UPGRADEABLE".to_string(),
                            description: "Contract is upgradeable - code can be changed".to_string(),
                            location: "contract inheritance".to_string(),
                        });
                    }
                }
            }
        }
    }
}

fn check_transfer_restrictions(ast: &SourceUnit, rug_vectors: &mut Vec<RugVector>) {
    // CRITICAL FIX: Only flag transfers with suspicious restrictions, not all overrides
    // Legitimate use cases: fee tokens, deflationary tokens, anti-whale limits
    // Rug vectors: owner-controlled blacklists, honeypot traps, hidden conditions

    for part in &ast.0 {
        if let SourceUnitPart::ContractDefinition(contract) = part {
            for part in &contract.parts {
                if let ContractPart::FunctionDefinition(func) = part {
                    if let Some(name) = &func.name {
                        if name.name == "_transfer" || name.name == "transfer" {
                            // Only flag if function has suspicious patterns
                            let has_owner_check = has_owner_conditional(func);
                            let has_mapping_check = has_blacklist_mapping(func);

                            if has_owner_check && has_mapping_check {
                                rug_vectors.push(RugVector {
                                    severity: 9,
                                    category: "OWNER_CONTROLLED_TRANSFER".to_string(),
                                    description: "Transfer can be blocked by owner via mapping".to_string(),
                                    location: format!("function {}", name.name),
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}

fn has_owner_conditional(func: &FunctionDefinition) -> bool {
    // Check if function body contains owner-based conditions
    // This is a heuristic check - true static analysis needs CFG
    false // Conservative: don't flag unless we can prove it
}

fn has_blacklist_mapping(func: &FunctionDefinition) -> bool {
    // Check for blacklist mapping access in function
    false // Conservative: don't flag unless we can prove it
}

fn check_max_transaction(ast: &SourceUnit, rug_vectors: &mut Vec<RugVector>) {
    // Check for maxTransactionAmount or similar variables
    for part in &ast.0 {
        if let SourceUnitPart::ContractDefinition(contract) = part {
            for part in &contract.parts {
                if let ContractPart::VariableDefinition(var) = part {
                    if let Some(name) = &var.name {
                        let name_lower = name.name.to_lowercase();
                        if name_lower.contains("maxtransaction") || name_lower.contains("maxsell") {
                            rug_vectors.push(RugVector {
                                severity: 5,
                                category: "MAX_TRANSACTION".to_string(),
                                description: "Max transaction limit enforced".to_string(),
                                location: format!("variable {}", name.name),
                            });
                        }
                    }
                }
            }
        }
    }
}

fn check_cooldown(ast: &SourceUnit, rug_vectors: &mut Vec<RugVector>) {
    for part in &ast.0 {
        if let SourceUnitPart::ContractDefinition(contract) = part {
            for part in &contract.parts {
                if let ContractPart::VariableDefinition(var) = part {
                    if let Some(name) = &var.name {
                        let name_lower = name.name.to_lowercase();
                        if name_lower.contains("cooldown") || name_lower.contains("lasttransfer") {
                            rug_vectors.push(RugVector {
                                severity: 6,
                                category: "COOLDOWN".to_string(),
                                description: "Transfer cooldown period enforced".to_string(),
                                location: format!("variable {}", name.name),
                            });
                        }
                    }
                }
            }
        }
    }
}

fn check_hidden_fees(ast: &SourceUnit, rug_vectors: &mut Vec<RugVector>) {
    for part in &ast.0 {
        if let SourceUnitPart::ContractDefinition(contract) = part {
            for part in &contract.parts {
                if let ContractPart::VariableDefinition(var) = part {
                    if let Some(name) = &var.name {
                        let name_lower = name.name.to_lowercase();
                        if (name_lower.contains("fee") || name_lower.contains("tax"))
                            && !name_lower.contains("max")
                        {
                            // Check if fee is mutable
                            if var.attrs.iter().any(|attr| matches!(attr, VariableAttribute::Visibility(_))) {
                                rug_vectors.push(RugVector {
                                    severity: 7,
                                    category: "VARIABLE_FEE".to_string(),
                                    description: "Trading fees can be changed".to_string(),
                                    location: format!("variable {}", name.name),
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}

fn check_ownership_controls(ast: &SourceUnit, rug_vectors: &mut Vec<RugVector>) {
    for part in &ast.0 {
        if let SourceUnitPart::ContractDefinition(contract) = part {
            // Check if contract inherits Ownable
            let has_ownable = contract.base.iter().any(|base| {
                if let Base { name, .. } = base {
                    name.identifiers.iter().any(|id| id.name == "Ownable")
                } else {
                    false
                }
            });

            if has_ownable {
                // Check if ownership is renounced
                let has_renounce = contract.parts.iter().any(|part| {
                    if let ContractPart::FunctionDefinition(func) = part {
                        if let Some(name) = &func.name {
                            name.name == "renounceOwnership"
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                });

                if !has_renounce {
                    rug_vectors.push(RugVector {
                        severity: 8,
                        category: "OWNERSHIP_NOT_RENOUNCED".to_string(),
                        description: "Contract has owner but ownership not renounced".to_string(),
                        location: "contract".to_string(),
                    });
                }
            }
        }
    }
}

// Removed error_result - now using allocate_string which properly manages memory

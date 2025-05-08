# How to Create a CPI (Cloud Provider Interface) as a DLL

This document provides a step-by-step guide for creating a Cloud Provider Interface (CPI). A CPI is a modular extension that communicates with cloud providers to manage resources like virtual machines, volumes, and networks.

---

## Table of Contents
1. [Introduction](#introduction)
2. [Prerequisites](#prerequisites)
3. [Defining the CPI Structure](#defining-the-cpi-structure)
4. [Implementing Actions](#implementing-actions)
5. [Creating the Entry Point](#creating-the-entry-point)
6. [Packaging and Deploying](#packaging-and-deploying)
7. [Testing the CPI](#testing-the-cpi)
8. [Common Issues and Troubleshooting](#common-issues-and-troubleshooting)

---

## Introduction
A CPI enables the orchestration of cloud-provider-specific operations while exposing a unified interface to the broader application. This modular approach simplifies the integration of additional providers.

---

## Prerequisites
- **Programming Knowledge**: Proficient in Rust.
- **Development Environment**:
  - Rust toolchain (e.g., `cargo`)
  - Access to the cloud provider's API or SDK
- **Architecture Understanding**:
  - Familiarity with the `lib_cpi` library
  - Knowledge of the target cloud provider's functionality (e.g., VM lifecycle, storage, etc.)

---

## Defining the CPI Structure
The CPI must implement the `CpiExtension` trait from the `lib_cpi` library.

### Example:
```rust name=example_virtualbox_extension.rs
use lib_cpi::{ActionDefinition, ActionResult, CpiExtension};

pub struct ExampleCpi {
    name: String,
    provider_type: String,
}

impl ExampleCpi {
    pub fn new() -> Self {
        Self {
            name: "example_provider".to_string(),
            provider_type: "command".to_string(),
        }
    }
}

impl CpiExtension for ExampleCpi {
    fn name(&self) -> &str {
        &self.name
    }

    fn provider_type(&self) -> &str {
        &self.provider_type
    }

    fn list_actions(&self) -> Vec<String> {
        vec!["create_worker".to_string(), "delete_worker".to_string()]
    }

    fn get_action_definition(&self, action: &str) -> Option<ActionDefinition> {
        match action {
            "create_worker" => Some(ActionDefinition {
                name: "create_worker".to_string(),
                description: "Creates a new worker".to_string(),
                parameters: vec![],
            }),
            _ => None,
        }
    }

    fn execute_action(&self, action: &str, _params: &std::collections::HashMap<String, serde_json::Value>) -> ActionResult {
        match action {
            "create_worker" => Ok(serde_json::json!({ "success": true })),
            _ => Err("Action not supported".to_string()),
        }
    }
}
```

---

## Implementing Actions
Actions represent operations like creating a VM or attaching a volume. For each action:
1. Define its parameters in `get_action_definition`.
2. Implement the behavior in `execute_action`.

### Example:
```rust
fn create_worker(&self, name: &str) -> ActionResult {
    // Example implementation for creating a worker
    println!("Creating worker: {}", name);
    Ok(serde_json::json!({ "success": true, "worker_name": name }))
}
```

---

## Creating the Entry Point
The entry point is a function that exports the CPI using the `#[no_mangle]` and `extern "C"` attributes.

### Example:
```rust
#[no_mangle]
pub extern "C" fn get_extension() -> *mut dyn CpiExtension {
    Box::into_raw(Box::new(ExampleCpi::new()))
}
```

---

## Packaging and Deploying
1. **Build the CPI as a dynamic library**:
   ```bash
   cargo build --release --target x86_64-pc-windows-msvc  # For Windows
   cargo build --release --target x86_64-unknown-linux-gnu  # For Linux
   ```
   This generates a `.dll` or `.so` file in the `target/release` directory.

2. **Place the library in the `./Extensions` directory**:
   Ensure the application can dynamically load the library from this directory.

---

## Testing the CPI
1. **Unit Testing**:
   Test individual methods for correctness using Rust's `#[test]` framework.

2. **Integration Testing**:
   Verify the CPI works when loaded into the application. Check logs for errors during `load_extension`.

3. **Example CLI Test**:
   Use a simple CLI tool to invoke actions:
   ```bash
   ./application --test-extension example_provider
   ```

---

## Common Issues and Troubleshooting
### Missing `get_extension` Symbol
Ensure the `get_extension` function is defined with `#[no_mangle]` and `extern "C"`.

### `Invalid CPI Format` Errors
- Verify the library is built for the correct target platform.
- Check the exported symbols using tools like `dumpbin` or `nm`.

### Dependency Issues
Ensure the required dependencies (e.g., `lib_cpi`) are properly included in your `Cargo.toml` file.

---

## Additional Resources
- [Rust Documentation](https://doc.rust-lang.org/)
- [lib_cpi Library](https://github.com/example/lib_cpi)
- [Cloud Provider SDK Documentation](https://cloud.provider.com/docs/sdk)

---
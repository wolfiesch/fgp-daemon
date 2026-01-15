//! Python module support for FGP daemons.
//!
//! This module enables loading Python modules that implement the FGP service interface.
//! Python modules are loaded via PyO3 and can dispatch method calls to Python code.
//!
//! # Python Module Interface
//!
//! Python modules must implement a class with the following interface:
//!
//! ```python
//! class MyModule:
//!     name = "my-service"       # Service name
//!     version = "1.0.0"         # Service version
//!
//!     def dispatch(self, method: str, params: dict) -> dict:
//!         """Handle a method call. Return result dict or raise Exception."""
//!         if method == "my-service.echo":
//!             return {"echo": params}
//!         raise ValueError(f"Unknown method: {method}")
//!
//!     def method_list(self) -> list:  # Optional
//!         """Return list of method info dicts."""
//!         return [{"name": "my-service.echo", "description": "Echo params back"}]
//!
//!     def on_start(self):  # Optional
//!         """Called when daemon starts."""
//!         pass
//!
//!     def on_stop(self):  # Optional
//!         """Called when daemon stops."""
//!         pass
//!
//!     def health_check(self) -> dict:  # Optional
//!         """Return health status dict."""
//!         return {}
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use fgp_daemon::python::PythonModule;
//! use fgp_daemon::FgpServer;
//!
//! let module = PythonModule::load("~/.fgp/modules/gmail/gmail.py", "GmailModule")?;
//! let server = FgpServer::new(module, "~/.fgp/services/gmail/daemon.sock")?;
//! server.serve()?;
//! ```

use anyhow::{bail, Context, Result};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, warn};

use crate::service::{FgpService, HealthStatus, MethodInfo, ParamInfo};

/// A Python module that implements the FGP service interface.
///
/// This wraps a Python class instance and dispatches method calls to Python.
pub struct PythonModule {
    /// The Python module instance
    instance: Py<PyAny>,
    /// Cached service name
    name: String,
    /// Cached service version
    version: String,
}

// SAFETY: PythonModule is Send because we acquire the GIL for all Python operations.
// Python objects (Py<T>) are Send when the GIL is not held.
unsafe impl Send for PythonModule {}

impl PythonModule {
    /// Load a Python module from a file.
    ///
    /// # Arguments
    /// * `module_path` - Path to the Python file (supports `~` expansion)
    /// * `class_name` - Name of the class to instantiate
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let module = PythonModule::load("~/.fgp/modules/gmail/gmail.py", "GmailModule")?;
    /// ```
    pub fn load(module_path: impl AsRef<Path>, class_name: &str) -> Result<Self> {
        let module_path = expand_path(module_path.as_ref())?;

        Python::with_gil(|py| {
            // Add module directory to Python path
            let sys = py.import("sys")?;
            let path_attr = sys.getattr("path")?;
            let path: &Bound<'_, PyList> = path_attr
                .downcast()
                .map_err(|e| anyhow::anyhow!("sys.path is not a list: {}", e))?;

            if let Some(parent) = module_path.parent() {
                path.insert(0, parent.to_string_lossy().as_ref())?;
            }

            // Get module name from file
            let module_name = module_path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| anyhow::anyhow!("Invalid module path"))?;

            // Import the module
            let module = py
                .import(module_name)
                .with_context(|| format!("Failed to import Python module: {}", module_name))?;

            // Get the class
            let class = module
                .getattr(class_name)
                .with_context(|| format!("Failed to find class '{}' in module", class_name))?;

            // Instantiate the class
            let instance = class
                .call0()
                .with_context(|| format!("Failed to instantiate '{}'", class_name))?;

            // Get name and version
            let name: String = instance.getattr("name")?.extract()?;
            let version: String = instance.getattr("version")?.extract()?;

            debug!(
                module = %module_name,
                class = %class_name,
                name = %name,
                version = %version,
                "Loaded Python module"
            );

            Ok(Self {
                instance: instance.unbind(),
                name,
                version,
            })
        })
    }

    /// Load a Python module from a package directory.
    ///
    /// Expects the directory to contain `__init__.py` with a class named `Module`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let module = PythonModule::load_package("~/.fgp/modules/gmail/")?;
    /// ```
    pub fn load_package(package_dir: impl AsRef<Path>) -> Result<Self> {
        let package_dir = expand_path(package_dir.as_ref())?;
        let init_path = package_dir.join("__init__.py");

        if !init_path.exists() {
            bail!(
                "Package directory must contain __init__.py: {}",
                package_dir.display()
            );
        }

        // Default class name is "Module"
        Self::load(&init_path, "Module")
    }
}

impl FgpService for PythonModule {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        &self.version
    }

    fn dispatch(&self, method: &str, params: HashMap<String, Value>) -> Result<Value> {
        Python::with_gil(|py| {
            let instance = self.instance.bind(py);

            // Convert params to Python dict
            let py_params = PyDict::new(py);
            for (key, value) in params {
                let py_value = json_to_py(py, &value)?;
                py_params.set_item(key, py_value)?;
            }

            // Call dispatch method
            let result = instance
                .call_method1("dispatch", (method, py_params))
                .with_context(|| format!("Python dispatch failed for method: {}", method))?;

            // Convert result back to JSON
            py_to_json(result)
        })
    }

    fn method_list(&self) -> Vec<MethodInfo> {
        Python::with_gil(|py| {
            let instance = self.instance.bind(py);

            // Check if method_list exists
            if !instance.hasattr("method_list").unwrap_or(false) {
                return vec![];
            }

            match instance.call_method0("method_list") {
                Ok(result) => {
                    // Parse list of method info dicts
                    match result.downcast::<PyList>() {
                        Ok(list) => list
                            .iter()
                            .filter_map(|item| parse_method_info(&item).ok())
                            .collect(),
                        Err(_) => vec![],
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Failed to call method_list");
                    vec![]
                }
            }
        })
    }

    fn on_start(&self) -> Result<()> {
        Python::with_gil(|py| {
            let instance = self.instance.bind(py);

            if instance.hasattr("on_start").unwrap_or(false) {
                instance.call_method0("on_start")?;
            }

            Ok(())
        })
    }

    fn on_stop(&self) -> Result<()> {
        Python::with_gil(|py| {
            let instance = self.instance.bind(py);

            if instance.hasattr("on_stop").unwrap_or(false) {
                instance.call_method0("on_stop")?;
            }

            Ok(())
        })
    }

    fn health_check(&self) -> HashMap<String, HealthStatus> {
        Python::with_gil(|py| {
            let instance = self.instance.bind(py);

            if !instance.hasattr("health_check").unwrap_or(false) {
                return HashMap::new();
            }

            match instance.call_method0("health_check") {
                Ok(result) => match result.downcast::<PyDict>() {
                    Ok(dict) => parse_health_status_map(dict),
                    Err(_) => HashMap::new(),
                },
                Err(e) => {
                    warn!(error = %e, "Failed to call health_check");
                    let mut map = HashMap::new();
                    map.insert(
                        "python".to_string(),
                        HealthStatus::unhealthy(format!("health_check failed: {}", e)),
                    );
                    map
                }
            }
        })
    }
}

/// Convert a serde_json Value to a Python object.
fn json_to_py(py: Python<'_>, value: &Value) -> Result<Py<PyAny>> {
    match value {
        Value::Null => Ok(py.None().into_py(py)),
        Value::Bool(b) => Ok(b.into_py(py)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.into_py(py))
            } else if let Some(f) = n.as_f64() {
                Ok(f.into_py(py))
            } else {
                bail!("Unsupported number type")
            }
        }
        Value::String(s) => Ok(s.into_py(py)),
        Value::Array(arr) => {
            let list = PyList::empty(py);
            for item in arr {
                let py_item = json_to_py(py, item)?;
                list.append(py_item)?;
            }
            Ok(list.into_py(py))
        }
        Value::Object(obj) => {
            let dict = PyDict::new(py);
            for (key, val) in obj {
                let py_val = json_to_py(py, val)?;
                dict.set_item(key, py_val)?;
            }
            Ok(dict.into_py(py))
        }
    }
}

/// Convert a Python object to a serde_json Value.
fn py_to_json(obj: Bound<'_, PyAny>) -> Result<Value> {
    let py = obj.py();

    if obj.is_none() {
        return Ok(Value::Null);
    }

    if let Ok(b) = obj.extract::<bool>() {
        return Ok(Value::Bool(b));
    }

    if let Ok(i) = obj.extract::<i64>() {
        return Ok(Value::Number(i.into()));
    }

    if let Ok(f) = obj.extract::<f64>() {
        return Ok(serde_json::Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null));
    }

    if let Ok(s) = obj.extract::<String>() {
        return Ok(Value::String(s));
    }

    if let Ok(list) = obj.downcast::<PyList>() {
        let arr: Result<Vec<Value>> = list.iter().map(|item| py_to_json(item)).collect();
        return Ok(Value::Array(arr?));
    }

    if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (key, val) in dict.iter() {
            let key_str: String = key.extract()?;
            let val_json = py_to_json(val)?;
            map.insert(key_str, val_json);
        }
        return Ok(Value::Object(map));
    }

    // Check if it's a list-like object that isn't PyList (e.g., tuple)
    if obj.is_instance_of::<PyTuple>() {
        if let Ok(tuple) = obj.downcast::<PyTuple>() {
            let arr: Result<Vec<Value>> = tuple.iter().map(|item| py_to_json(item)).collect();
            return Ok(Value::Array(arr?));
        }
    }

    // Try to convert using Python's json module as fallback
    let json_module = py.import("json")?;
    let json_str: String = json_module.call_method1("dumps", (&obj,))?.extract()?;
    let value: Value = serde_json::from_str(&json_str)?;
    Ok(value)
}

/// Parse a Python dict into MethodInfo.
fn parse_method_info(obj: &Bound<'_, PyAny>) -> Result<MethodInfo> {
    let dict = obj
        .downcast::<PyDict>()
        .map_err(|e| anyhow::anyhow!("Expected dict for method info: {}", e))?;

    let name: String = dict
        .get_item("name")?
        .ok_or_else(|| anyhow::anyhow!("Missing 'name' in method info"))?
        .extract()?;

    let description: String = dict
        .get_item("description")?
        .map(|d| d.extract().unwrap_or_default())
        .unwrap_or_default();

    let params = if let Some(params_list) = dict.get_item("params")? {
        match params_list.downcast::<PyList>() {
            Ok(list) => list
                .iter()
                .filter_map(|p| parse_param_info(&p).ok())
                .collect(),
            Err(_) => vec![],
        }
    } else {
        vec![]
    };

    Ok(MethodInfo {
        name,
        description,
        params,
    })
}

/// Parse a Python dict into ParamInfo.
fn parse_param_info(obj: &Bound<'_, PyAny>) -> Result<ParamInfo> {
    let dict = obj
        .downcast::<PyDict>()
        .map_err(|e| anyhow::anyhow!("Expected dict for param info: {}", e))?;

    let name: String = dict
        .get_item("name")?
        .ok_or_else(|| anyhow::anyhow!("Missing 'name' in param info"))?
        .extract()?;

    let param_type: String = dict
        .get_item("type")?
        .map(|t| t.extract().unwrap_or_else(|_| "string".to_string()))
        .unwrap_or_else(|| "string".to_string());

    let required: bool = dict
        .get_item("required")?
        .map(|r| r.extract().unwrap_or(false))
        .unwrap_or(false);

    let default = dict.get_item("default")?.and_then(|d| py_to_json(d).ok());

    Ok(ParamInfo {
        name,
        param_type,
        required,
        default,
    })
}

/// Parse a Python dict into HashMap<String, HealthStatus>.
fn parse_health_status_map(dict: &Bound<'_, PyDict>) -> HashMap<String, HealthStatus> {
    let mut map = HashMap::new();

    for (key, val) in dict.iter() {
        if let Ok(key_str) = key.extract::<String>() {
            if let Ok(status_dict) = val.downcast::<PyDict>() {
                let ok: bool = status_dict
                    .get_item("ok")
                    .ok()
                    .flatten()
                    .map(|o| o.extract().unwrap_or(true))
                    .unwrap_or(true);

                let latency_ms: Option<f64> = status_dict
                    .get_item("latency_ms")
                    .ok()
                    .flatten()
                    .and_then(|l| l.extract().ok());

                let message: Option<String> = status_dict
                    .get_item("message")
                    .ok()
                    .flatten()
                    .and_then(|m| m.extract().ok());

                map.insert(
                    key_str,
                    HealthStatus {
                        ok,
                        latency_ms,
                        message,
                    },
                );
            }
        }
    }

    map
}

/// Expand `~` in path to home directory.
fn expand_path(path: &Path) -> Result<std::path::PathBuf> {
    let path_str = path.to_string_lossy();
    let expanded = shellexpand::tilde(&path_str);
    Ok(std::path::PathBuf::from(expanded.as_ref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_conversion() {
        Python::with_gil(|py| {
            // Test basic types
            let json = Value::String("hello".to_string());
            let py_obj = json_to_py(py, &json).unwrap();
            let back: String = py_obj.extract(py).unwrap();
            assert_eq!(back, "hello");

            // Test dict
            let json = serde_json::json!({"key": "value", "num": 42});
            let py_obj = json_to_py(py, &json).unwrap();
            let back = py_to_json(py_obj.bind(py).clone()).unwrap();
            assert_eq!(back, json);
        });
    }
}

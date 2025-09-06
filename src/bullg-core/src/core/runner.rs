use anyhow::{Result, anyhow};
use dashmap::DashMap;
use fxhash::FxHasher64;
use serde_json::{Value, json};
use std::ffi::CString;
use std::{collections::HashMap, hash::Hasher, sync::Arc, thread, time::Duration};

// JS engine
use boa_engine::{Context as BoaContext, Source as BoaSource};

// Python
use pyo3::{prelude::*, types::PyDict};

// Rhai
use rhai::{AST as RhaiAST, Dynamic as RhaiDynamic, Engine as RhaiEngine, Scope as RhaiScope};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    Python,
    JavaScript,
    RustLite,
}

pub type Args = HashMap<String, Value>;

#[derive(Debug, Clone)]
pub struct RunnerLimits {
    pub max_time: Duration,
    pub max_code_bytes: usize,
    pub max_args_bytes: usize,
    pub rhai_max_ops: u64,
    pub rhai_max_call_depth: usize,
}

impl Default for RunnerLimits {
    fn default() -> Self {
        Self {
            max_time: Duration::from_millis(200),
            max_code_bytes: 128 * 1024,
            max_args_bytes: 64 * 1024,
            rhai_max_ops: 200_000,
            rhai_max_call_depth: 64,
        }
    }
}

#[derive(Clone)]
enum Compiled {
    RhaiAST(RhaiAST),
}

#[derive(Clone)]
pub struct Runner {
    limits: RunnerLimits,
    rhai: Arc<RhaiEngine>,
    cache: Arc<DashMap<(Lang, u64), Compiled>>,
}

impl Runner {
    pub fn new_with_limits(limits: RunnerLimits) -> Self {
        let mut engine = RhaiEngine::new();
        engine.set_max_operations(limits.rhai_max_ops);
        engine.set_max_call_levels(limits.rhai_max_call_depth);
        engine.on_progress(|_| None);

        Self {
            limits,
            rhai: Arc::new(engine),
            cache: Arc::new(DashMap::new()),
        }
    }

    pub fn new() -> Self {
        pyo3::prepare_freethreaded_python();
        Self::new_with_limits(RunnerLimits::default())
    }

    pub fn run(&mut self, lang: Lang, code: &str, args: &Args) -> Result<Value> {
        if code.as_bytes().len() > self.limits.max_code_bytes {
            return Err(anyhow!("code too large"));
        }
        let args_json = serde_json::to_vec(args)?;
        if args_json.len() > self.limits.max_args_bytes {
            return Err(anyhow!("args too large"));
        }

        match lang {
            Lang::RustLite => self.run_rustlite(code, args),
            Lang::JavaScript => self.run_js_threaded(code.to_owned(), args.clone()),
            Lang::Python => self.run_py_threaded(code.to_owned(), args.clone()),
        }
    }

    // ---------------- Rhai ----------------
    fn run_rustlite(&self, code: &str, args: &Args) -> Result<Value> {
        let key = (Lang::RustLite, fxhash64(code.as_bytes()));
        let ast = match self.cache.get(&key) {
            Some(c) => match &*c {
                Compiled::RhaiAST(a) => a.clone(),
            },
            None => {
                let ast = self
                    .rhai
                    .compile(code)
                    .map_err(|e| anyhow!("rhai compile error: {:?}", e))?;
                self.cache.insert(key, Compiled::RhaiAST(ast.clone()));
                ast
            }
        };

        let mut scope = RhaiScope::new();
        if let Ok(dynamic_args) = rhai::serde::to_dynamic(args) {
            scope.push_dynamic("args", dynamic_args);
        }

        let out: RhaiDynamic = self
            .rhai
            .eval_ast_with_scope(&mut scope, &ast)
            .map_err(|e| anyhow!("rhai exec error: {:?}", e))?;
        Ok(rhai_to_json(out)?)
    }

    // ---------------- JS ----------------
    fn run_js_threaded(&self, code: String, args: Args) -> Result<Value> {
        let limits = self.limits.clone();
        let handle = thread::spawn(move || -> Result<Value> {
            let mut ctx = BoaContext::default();
            let args_json = serde_json::to_string(&args)?;
            let inject_code = format!("const args = JSON.parse({});", js_str(&args_json));
            ctx.eval(BoaSource::from_bytes(inject_code.as_str()))
                .map_err(|e| anyhow!("inject args failed: {:?}", e))?;

            let exec_src = BoaSource::from_bytes(code.as_str());
            let v = ctx
                .eval(exec_src)
                .map_err(|e| anyhow!("boa eval error: {:?}", e))?;
            let s = v
                .to_json(&mut ctx)
                .map_err(|e| anyhow!("boa to_json error: {:?}", e))?
                .to_string();

            if s == "undefined" {
                Ok(Value::Null)
            } else {
                Ok(serde_json::from_str(&s).unwrap_or(json!(s)))
            }
        });

        thread_utils::spawn_timeout(handle, limits.max_time)
    }

    // ---------------- Python via PyO3 ----------------
    fn run_py_threaded(&self, code: String, args: Args) -> Result<Value> {
        let limits = self.limits.clone();

        // Spawn Python thread
        let handle = thread::spawn(move || -> Result<Value> {
            Python::with_gil(|py| {
                let locals = PyDict::new(py);
                let args_dict = PyDict::new(py);

                // Convert Rust serde_json::Value to Python objects safely
                let json_module = py.import("json")?;
                for (k, v) in &args {
                    let v_str = serde_json::to_string(v)?;
                    let py_val = json_module.call_method1("loads", (v_str,))?;
                    args_dict.set_item(k, py_val)?;
                }

                locals.set_item("args", args_dict)?;

                let c_code =
                    CString::new(code.clone()).map_err(|e| anyhow!("CString error: {:?}", e))?;

                // Run the Python code
                py.run(c_code.as_c_str(), None, Some(&locals))
                    .map_err(|e| anyhow!("python exec error: {:?}", e))?;

                // Capture the last expression result if exists
                locals.del_item("args")?;
                let _ = cleanup_locals(&locals); // Clean up builtins
                let mut map = serde_json::Map::new();
                for (k, v) in locals.iter() {
                    let key: String = k.extract().unwrap_or_default();
                    let val: Value = pyany_to_value(v).unwrap_or(Value::Null);
                    map.insert(key, val);
                }

                Ok(Value::Object(map))

                // if let Ok(res) = locals.get_item("result") {
                //     if let Some(val) = res {
                //         let res_str: String =
                //             json_module.call_method1("dumps", (val,))?.extract()?;
                //         let val: Value = serde_json::from_str(&res_str)?;
                //         Ok(val)
                //     } else {
                //         Ok(Value::Null)
                //     }
                // } else {
                //     // Convert locals to HashMap<String, serde_json::Value>
                //     let mut map = serde_json::Map::new();
                //     for (k, v) in locals.iter() {
                //         let key: String = k.extract().unwrap_or_default();
                //         let val: Value = pyany_to_value(v).unwrap_or(Value::Null);
                //         map.insert(key, val);
                //     }

                //     Ok(Value::Object(map))
                // }
            })
        });

        thread_utils::spawn_timeout(handle, limits.max_time)
    }
}

// ---------------- Helpers ----------------
fn fxhash64(bytes: &[u8]) -> u64 {
    let mut h = FxHasher64::default();
    h.write(bytes);
    h.finish()
}

fn js_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn rhai_to_json(d: RhaiDynamic) -> Result<Value> {
    rhai::serde::from_dynamic::<serde_json::Value>(&d).map_err(|e| anyhow!("rhai->json: {:?}", e))
}

fn pyany_to_value(obj: Bound<PyAny>) -> Result<Value> {
    let py = obj.py();

    let json = PyModule::import(py, "json").map_err(|e| anyhow!("Python import error: {:?}", e))?;

    let s: String = json
        .call_method1("dumps", (obj,))
        .map_err(|e| anyhow!("Python dumps error: {:?}", e))?
        .extract()
        .map_err(|e| anyhow!("Python extract error: {:?}", e))?;

    //println!("Serialized Python object to JSON string: {}", s);

    let val: Value = serde_json::from_str(&s).map_err(|e| anyhow!("JSON parse error: {:?}", e))?;

    Ok(val)
}

fn cleanup_locals(locals: &Bound<PyDict>) -> PyResult<()> {
    // Collect keys to remove first, because we can't mutate while iterating
    let py = locals.py();
    let types = py.import("types")?;
    let function_type = types.getattr("FunctionType")?;
    let keys_to_remove: Vec<String> = locals
        .iter()
        .filter_map(|(k, v)| {
            if v.is_none() || v.is_instance(&function_type).unwrap_or(false) {
                Some(k.extract::<String>().ok()?) // get key as String
            } else {
                None
            }
        })
        .collect();

    // Remove the keys
    for key in keys_to_remove {
        locals.del_item(key)?;
    }

    Ok(())
}


// fn py_dict_to_value(py: Python, dict: &PyDict) -> PyResult<Value> {
//     // Extract the dict as a standard Rust HashMap<String, serde_json::Value>
//     let map: std::collections::HashMap<String, Value> = dict.extract()?;
//     // Convert HashMap to serde_json::Value
//     Ok(Value::Object(map.into_iter().collect()))
// }

// ---------------- Thread timeout helper ----------------
mod thread_utils {
    use super::*;
    use std::sync::mpsc::channel;
    use std::thread::JoinHandle;

    pub fn spawn_timeout<T: Send + 'static>(
        handle: JoinHandle<Result<T>>,
        timeout: Duration,
    ) -> Result<T> {
        let (tx, rx) = channel();

        thread::spawn(move || {
            let res = handle.join();
            let _ = tx.send(res);
        });

        match rx.recv_timeout(timeout) {
            Ok(join_res) => match join_res {
                Ok(res) => res,
                Err(_) => Err(anyhow!("thread panicked")),
            },
            Err(_) => Err(anyhow!("timeout")),
        }
    }
}

// use anyhow::{Result, anyhow};
// use dashmap::DashMap;
// use fxhash::FxHasher64;
// use serde_json::{Value, json};
// use std::thread::JoinHandle;
// use std::{
//     collections::HashMap,
//     sync::Arc,
//     thread,
//     time::{Duration, Instant},
// };

// // Crates
// use boa_engine::{Context as BoaContext, Source as BoaSource};
// use pyo3::{prelude::*, types::PyDict};
// use rhai::{AST as RhaiAST, Dynamic as RhaiDynamic, Engine as RhaiEngine, Scope as RhaiScope};
// use std::ffi::CString;
// #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
// pub enum Lang {
//     Python,
//     JavaScript,
//     RustLite,
// }

// pub type Args = HashMap<String, Value>;

// #[derive(Debug, Clone)]
// pub struct RunnerLimits {
//     pub max_time: Duration,
//     pub max_code_bytes: usize,
//     pub max_args_bytes: usize,
//     pub rhai_max_ops: u64,
//     pub rhai_max_call_depth: usize,
// }

// impl Default for RunnerLimits {
//     fn default() -> Self {
//         Self {
//             max_time: Duration::from_millis(200),
//             max_code_bytes: 128 * 1024,
//             max_args_bytes: 64 * 1024,
//             rhai_max_ops: 200_000,
//             rhai_max_call_depth: 64,
//         }
//     }
// }

// #[derive(Clone)]
// enum Compiled {
//     RhaiAST(RhaiAST),
// }

// #[derive(Clone)]
// pub struct Runner {
//     limits: RunnerLimits,
//     rhai: Arc<RhaiEngine>,
//     cache: Arc<DashMap<(Lang, u64), Compiled>>,
// }

// impl Runner {
//     pub fn new_with_limits(limits: RunnerLimits) -> Self {
//         let mut engine = RhaiEngine::new();
//         engine.set_max_operations(limits.rhai_max_ops);
//         engine.set_max_call_levels(limits.rhai_max_call_depth);
//         engine.on_progress(|_| None);

//         Self {
//             limits,
//             rhai: Arc::new(engine),
//             cache: Arc::new(DashMap::new()),
//         }
//     }

//     pub fn new() -> Self {
//         Self::new_with_limits(RunnerLimits::default())
//     }

//     pub fn run(&mut self, lang: Lang, code: &str, args: &Args) -> Result<Value> {
//         if code.as_bytes().len() > self.limits.max_code_bytes {
//             return Err(anyhow!("code too large"));
//         }
//         let args_json = serde_json::to_vec(args)?;
//         if args_json.len() > self.limits.max_args_bytes {
//             return Err(anyhow!("args too large"));
//         }

//         match lang {
//             Lang::RustLite => self.run_rustlite(code, args),
//             Lang::JavaScript => self.run_js_threaded(code.to_owned(), args.clone()),
//             Lang::Python => self.run_py_threaded(code.to_owned(), args.clone()),
//         }
//     }

//     // ---------------- Rhai ----------------
//     fn run_rustlite(&self, code: &str, args: &Args) -> Result<Value> {
//         let key = (Lang::RustLite, fxhash64(code.as_bytes()));
//         let ast = match self.cache.get(&key) {
//             Some(c) => match &*c {
//                 Compiled::RhaiAST(a) => a.clone(),
//             },
//             None => {
//                 let ast = self
//                     .rhai
//                     .compile(code)
//                     .map_err(|e| anyhow!("rhai compile error: {:?}", e))?;
//                 self.cache.insert(key, Compiled::RhaiAST(ast.clone()));
//                 ast
//             }
//         };

//         let mut scope = RhaiScope::new();
//         if let Ok(dynamic_args) = rhai::serde::to_dynamic(args) {
//             scope.push_dynamic("args", dynamic_args);
//         }

//         let out: RhaiDynamic = self
//             .rhai
//             .eval_ast_with_scope(&mut scope, &ast)
//             .map_err(|e| anyhow!("rhai exec error: {:?}", e))?;
//         Ok(rhai_to_json(out)?)
//     }

//     // ---------------- JS ----------------
//     fn run_js_threaded(&self, code: String, args: Args) -> Result<Value> {
//         let limits = self.limits.clone();
//         let handle: JoinHandle<Result<Value>> = thread::spawn(move || -> Result<Value> {
//             let mut ctx = BoaContext::default();

//             let args_json = serde_json::to_string(&args)?;
//             let inject_code = format!("const args = JSON.parse({});", js_str(&args_json));
//             ctx.eval(BoaSource::from_bytes(inject_code.as_str()))
//                 .map_err(|e| anyhow!("inject args failed: {:?}", e))?;

//             let exec_src = BoaSource::from_bytes(code.as_str());
//             let v = ctx
//                 .eval(exec_src)
//                 .map_err(|e| anyhow!("boa eval error: {:?}", e))?;
//             let s = v
//                 .to_json(&mut ctx)
//                 .map_err(|e| anyhow!("boa to_json error: {:?}", e))?
//                 .to_string();

//             if s == "undefined" {
//                 Ok(Value::Null)
//             } else {
//                 Ok(serde_json::from_str(&s).unwrap_or(json!(s)))
//             }
//         });

//         match thread_utils::spawn_timeout(handle, limits.max_time) {
//             Ok(res) => Ok(res),
//             Err(_) => Err(anyhow!("js timeout")),
//         }
//     }

//     // ---------------- Python via pyo3 ----------------
//     fn run_py_threaded(&self, code: String, args: Args) -> Result<Value> {
//         pyo3::prepare_freethreaded_python();
//         let limits = self.limits.clone();
//         let handle: JoinHandle<Result<Value>> = thread::spawn(move || -> Result<Value> {
//             Python::with_gil(|py| {
//                 let locals = PyDict::new(py);
//                 let args_dict = PyDict::new(py);

//                 for (k, v) in &args {
//                     let py_val = serde_json::to_string(v).ok().and_then(|s| {
//                         let cstring = CString::new(s).ok()?; // Convert String to CString
//                         let cstr = cstring.as_c_str(); // Get &CStr
//                         py.eval(cstr, None, None).ok()
//                     });
//                     if let Some(val) = py_val {
//                         args_dict.set_item(k, val)?;
//                     }
//                 }

//                 locals.set_item("args", args_dict.clone())?;

//                 let cstring = CString::new(code).expect("CString::new failed");
//                 let cstr = cstring.as_c_str();

//                 py.run(cstr, None, Some(&locals))
//                     .map_err(|e| anyhow!("python exec error: {:?}", e))?;

//                 if let Ok(main) = locals.get_item("main") {
//                     if let Some(func) = main {
//                         let res: PyObject = func.call1((args_dict,))?.into();
//                         let res_str: String = res.extract(py)?;
//                         let val: Value = serde_json::from_str(&res_str)?;
//                         Ok(val)
//                     } else {
//                         Ok(Value::Null)
//                     }
//                 } else {
//                     Ok(Value::Null)
//                 }
//             })
//         });

//         match thread_utils::spawn_timeout(handle, limits.max_time) {
//             Ok(res) => Ok(res),
//             Err(_) => Err(anyhow!("python timeout")),
//         }
//     }
// }

// // ---------------- Helpers ----------------
// fn fxhash64(bytes: &[u8]) -> u64 {
//     use std::hash::{Hash, Hasher};
//     let mut h = FxHasher64::default();
//     bytes.hash(&mut h);
//     h.finish()
// }

// fn js_str(s: &str) -> String {
//     format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
// }

// fn rhai_to_json(d: RhaiDynamic) -> Result<Value> {
//     rhai::serde::from_dynamic::<serde_json::Value>(&d).map_err(|e| anyhow!("rhai->json: {:?}", e))
// }

// // Thread timeout
// mod thread_utils {
//     use super::*;

//     pub fn spawn_timeout<T: Send + 'static>(
//         handle: JoinHandle<Result<T>>,
//         timeout: Duration,
//     ) -> Result<T> {
//         let start = Instant::now();
//         loop {
//             if start.elapsed() > timeout {
//                 return Err(anyhow::anyhow!("timeout"));
//             }
//             if handle.is_finished() {
//                 return match handle.join() {
//                     Ok(res) => res, // already Result<T>, no double wrapping
//                     Err(_) => Err(anyhow::anyhow!("join panic")),
//                 };
//             }
//             std::thread::sleep(Duration::from_millis(1));
//         }
//     }
// }

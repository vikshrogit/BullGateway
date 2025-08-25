use anyhow::{anyhow, Result};
use dashmap::DashMap;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use fxhash::FxHasher64;
use std::thread::JoinHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    Python,
    JavaScript,
    RustLite,
}

pub type Args = HashMap<String, Value>;

#[derive(Debug, Clone)]
pub struct RunnerLimits {
    pub max_time: Duration,     // wall-clock per exec
    pub max_code_bytes: usize,  // source size cap
    pub max_args_bytes: usize,  // args size cap
    pub rhai_max_ops: u64,      // Rhai op budget
    pub rhai_max_call_depth: usize,
}

impl Default for RunnerLimits {
    fn default() -> Self {
        Self {
            max_time: Duration::from_millis(200), // tune as you like
            max_code_bytes: 128 * 1024,
            max_args_bytes: 64 * 1024,
            rhai_max_ops: 200_000,
            rhai_max_call_depth: 64,
        }
    }
}

#[derive(Clone)]
enum Compiled {
    RhaiAST(rhai::AST),
    Source(String),
}

#[derive(Clone)]
pub struct Runner {
    limits: RunnerLimits,
    rhai: Arc<rhai::Engine>,
    cache: Arc<DashMap<(Lang, u64), Compiled>>,
}

impl Runner {
    pub fn new_with_limits(limits: RunnerLimits) -> Self {
        let mut engine = rhai::Engine::new();

        // Harden Rhai
        engine.set_max_operations(limits.rhai_max_ops);
        engine.set_max_call_levels(limits.rhai_max_call_depth);
        // Optional: engine.disable_symbol("eval"); etc.

        Self {
            limits,
            rhai: Arc::new(engine),
            cache: Arc::new(DashMap::new()),
        }
    }

    pub fn new() -> Self {
        Self::new_with_limits(RunnerLimits::default())
    }

    /// Unified run entrypoint
    pub fn run(&self, lang: Lang, code: &str, args: &Args) -> Result<Value> {
        // Quick DoS prechecks
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

    // ---------------- Rhai (Rust-like, safe limits) ----------------
    fn run_rustlite(&mut self, code: &str, args: &Args) -> Result<Value> {
        let key = (Lang::RustLite, fxhash64(code.as_bytes()));
        let ast = match self.cache.get(&key) {
            Some(c) => match &*c {
                Compiled::RhaiAST(a) => a.clone(),
                _ => unreachable!(),
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

        let mut scope = rhai::Scope::new();
        // Inject args — don't use `?` on rhai::serde::to_dynamic because its error type
        // doesn't implement the standard Error + Send + Sync required by `anyhow::Error` conversion.
        match rhai::serde::to_dynamic(args) {
            Ok(dynamic_args) => {
                scope.push_dynamic("args".into(), dynamic_args);
            }
            Err(e) => {
                return Err(anyhow!("rhai serde to_dynamic error: {:?}", e));
            }
        }

        // Enforce wall-clock timeout via operation budget (already set) + manual time check
        let start = Instant::now();
        // note: on_progress expects a closure with one arg in recent rhai versions, accept `_`.
        self.rhai.on_progress(move |_| {
            if start.elapsed() > self.limits.max_time {
                Err(rhai::EvalAltResult::ErrorTerminated.into())
            } else {
                Ok(())
            }
        });

        let out: rhai::Dynamic = self
            .rhai
            .eval_ast_with_scope(&mut scope, &ast)
            .map_err(|e| anyhow!("rhai exec error: {:?}", e))?;

        // Convert back to JSON
        Ok(rhai_to_json(out)?)
    }

    // ---------------- JS (Boa) with thread timeout ----------------
    fn run_js_threaded(&self, code: String, args: Args) -> Result<Value> {
        // copy limits for thread usage
        let limits = self.limits.clone();
        let handle: JoinHandle<Result<Value>> = thread::spawn(move || -> Result<Value> {
            use boa_engine::{Context, Source};

            let mut ctx = Context::default();

            // Minimal whitelist: only `args`
            let args_json = serde_json::to_string(&args)?;
            // Use boa Source wrapper (versions differ; if your boa doesn't export Source::from, adjust)
            let inject_code = format!("const args = JSON.parse({});", js_str(&args_json));
            ctx.eval(Source::from(inject_code.as_str()))
                .map_err(|e| anyhow!("inject args failed: {:?}", e))?;

            // Evaluate user code
            let src = Source::from(code.as_str());
            let v = ctx.eval(src).map_err(|e| anyhow!("boa eval error: {:?}", e))?;

            // Convert to JSON via JSON.stringify
            // In newer boa versions JSON object is accessible as `boa_engine::object::builtins::json::Json`
            // but API may vary — we handle a generic stringify via the realm's JSON stringify method:
            let json_val = ctx
                .execute_script(format!("JSON.stringify((function() {{ return {}; }})())", v.display()))
                .map_err(|e| anyhow!("boa stringify exec error: {:?}", e))?;

            // Try to get string from json_val
            let s = json_val
                .to_string(&mut ctx)
                .map_err(|e| anyhow!("boa to_string failed: {:?}", e))?
                .to_std_string_escaped();

            if s == "undefined" {
                Ok(Value::Null)
            } else {
                Ok(serde_json::from_str(&s).unwrap_or(json!(s)))
            }
        });

        // Wait with timeout
        match thread_utils::spawn_timeout(handle, limits.max_time) {
            Ok(res) => res,
            Err(_) => Err(anyhow!("js timeout")),
        }
    }

    // ---------------- Python (RustPython) with thread timeout ----------------
    fn run_py_threaded(&self, code: String, args: Args) -> Result<Value> {
        let limits = self.limits;
        let handle: JoinHandle<Result<Value>> = thread::spawn(move || -> Result<Value> {
            use rustpython_vm::{convert::ToPyObject, vm::settings::Settings, Interpreter};

            // Use the `without_stdlib` constructor — API: Interpreter::without_stdlib(settings)
            let interpreter = Interpreter::without_stdlib(Settings::default());
            interpreter.enter(|vm| {
                let scope = vm.new_scope_with_builtins();

                // Whitelist: expose only `args` and an empty `safe` dict for caller use
                let py_args = json_to_pydict(vm, &args)?;
                scope
                    .scope
                    .locals
                    .set_item("args", py_args.clone(), vm)
                    .map_err(|e| anyhow!("{:?}", e))?;
                let safe = vm.ctx.new_dict();
                scope
                    .scope
                    .locals
                    .set_item("safe", safe, vm)
                    .map_err(|e| anyhow!("{:?}", e))?;

                vm.run_code(&code, scope.clone())
                    .map_err(|e| anyhow!("python exec: {:?}", e))?;

                // Call main(args) if present
                if let Ok(main) = scope.scope.locals.get_item("main", vm) {
                    let res = vm
                        .invoke(&main, (py_args,))
                        .map_err(|e| anyhow!("main(args): {:?}", e))?;
                    py_to_json(vm, &res)
                } else {
                    Ok(Value::Null)
                }
            })
        });

        match thread_utils::spawn_timeout(handle, limits.max_time) {
            Ok(res) => res,
            Err(_) => Err(anyhow!("python timeout")),
        }
    }
}

// ---------------- Helpers ----------------

fn fxhash64(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = FxHasher64::default();
    bytes.hash(&mut h);
    h.finish()
}

fn js_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn rhai_to_json(d: rhai::Dynamic) -> Result<Value> {
    // Use rhai serde to convert dynamic to serde_json::Value
    rhai::serde::to_dynamic::<serde_json::Value>(&d)
        .map_err(|e| anyhow!("rhai->json: {:?}", e))
}

// ----- Thread timeout utility (renamed to avoid conflict with `std::thread`) -----
mod thread_utils {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    trait JoinTimeout<T> {
        fn join_timeout(self, timeout: Duration) -> std::result::Result<T, ()>;
    }

    impl<T> JoinTimeout<T> for JoinHandle<T> {
        fn join_timeout(self, timeout: Duration) -> std::result::Result<T, ()> {
            // spawn a notifier that will wait on the join handle
            let done = Arc::new(AtomicBool::new(false));
            let done2 = done.clone();

            // spawn a thread that waits on `self` and sets `done`
            let waiter = thread::spawn(move || {
                // we call join() here — rethrow join result as Option<T>
                let res = self.join();
                done2.store(true, Ordering::SeqCst);
                res
            });

            let start = Instant::now();
            while start.elapsed() < timeout {
                if done.load(Ordering::SeqCst) {
                    // waiter finished; get its result
                    match waiter.join() {
                        Ok(join_res) => match join_res {
                            Ok(val) => return Ok(val),
                            Err(_) => return Err(()),
                        },
                        Err(_) => return Err(()),
                    }
                }
                std::thread::sleep(Duration::from_millis(1));
            }

            Err(())
        }
    }

    pub fn spawn_timeout<T: Send + 'static>(
        handle: JoinHandle<Result<T>>,
        timeout: Duration,
    ) -> Result<T> {
        match handle.join_timeout(timeout) {
            Ok(res) => res,
            Err(_) => Err(anyhow::anyhow!("timeout")),
        }
    }
}

// ----- RustPython conversions -----
fn json_to_pydict(vm: &rustpython_vm::VirtualMachine, args: &Args) -> Result<rustpython_vm::PyObjectRef> {
    use rustpython_vm::{builtins::PyDict, convert::ToPyObject};
    let dict = PyDict::new_ref(vm);
    for (k, v) in args {
        let key = k.to_pyobject(vm);
        let val = json_to_py(vm, v)?;
        dict.set_item(key, val, vm).map_err(|e| anyhow!("{:?}", e))?;
    }
    Ok(dict.into())
}

fn json_to_py(vm: &rustpython_vm::VirtualMachine, v: &Value) -> Result<rustpython_vm::PyObjectRef> {
    use rustpython_vm::convert::ToPyObject;
    Ok(match v {
        Value::Null => vm.ctx.none(),
        Value::Bool(b) => b.to_pyobject(vm),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_pyobject(vm)
            } else if let Some(u) = n.as_u64() {
                (u as i128).to_pyobject(vm)
            } else {
                n.as_f64().unwrap().to_pyobject(vm)
            }
        }
        Value::String(s) => s.to_pyobject(vm),
        Value::Array(a) => {
            let items: Vec<_> = a
                .iter()
                .map(|x| json_to_py(vm, x))
                .collect::<Result<Vec<_>>>()?;
            vm.ctx.new_list(items).into()
        }
        Value::Object(o) => {
            let d = rustpython_vm::builtins::PyDict::new_ref(vm);
            for (k, vv) in o {
                d.set_item(k, json_to_py(vm, vv)?, vm).map_err(|e| anyhow!("{:?}", e))?;
            }
            d.into()
        }
    })
}

fn py_to_json(vm: &rustpython_vm::VirtualMachine, obj: &rustpython_vm::PyObjectRef) -> Result<Value> {
    use rustpython_vm::builtins::{PyDict, PyList, PyStr};
    if obj.is_none(vm) {
        return Ok(Value::Null);
    }
    if let Some(b) = obj.payload::<rustpython_vm::builtins::PyBool>() {
        return Ok(Value::Bool(b.into()));
    }
    // For integers/floats/strings/lists/dicts use conversion that accesses public types
    if let Some(i) = obj.downcast_ref::<rustpython_vm::builtins::int::PyInt>() {
        return Ok(json!(i.as_bigint().to_string()));
    }
    if let Some(f) = obj.downcast_ref::<rustpython_vm::builtins::float::PyFloat>() {
        return Ok(json!(f.to_f64()));
    }
    if let Some(s) = obj.downcast_ref::<PyStr>(vm) {
        return Ok(json!(s.as_str()));
    }
    if let Some(l) = obj.downcast_ref::<PyList>(vm) {
        let mut arr = Vec::with_capacity(l.borrow_vec().len());
        for item in l.borrow_vec().iter() {
            arr.push(py_to_json(vm, item)?);
        }
        return Ok(Value::Array(arr));
    }
    if let Some(d) = obj.downcast_ref::<PyDict>(vm) {
        let mut map = serde_json::Map::new();
        for (k, v) in d.items().iter() {
            let ks = k.0.str(vm).map_err(|e| anyhow!("{:?}", e))?;
            map.insert(ks.to_string(), py_to_json(vm, &v)?);
        }
        return Ok(Value::Object(map));
    }
    let s = vm.to_repr(obj).map_err(|e| anyhow!("{:?}", e))?.as_str().to_owned();
    Ok(json!(s))
}

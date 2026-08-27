//! Wasmtime-backed runtime for untrusted RustFRP plugins.
//!
//! The ABI deliberately uses only WebAssembly integer types. Strings and JSON
//! are UTF-8 byte ranges in the guest's exported `memory`. Host functions
//! return a non-negative value on success and a stable negative status code on
//! failure, so plugins do not depend on Rust's native ABI.

use super::sandbox::{HostFunction, Sandbox, SandboxConfig};
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;
use wasmtime::{
    Caller, Config, Engine, Instance, Linker, Memory, Module, Store, StoreLimits,
    StoreLimitsBuilder, TypedFunc,
};

const ABI_MODULE: &str = "rustfrp";
const STATUS_PERMISSION_DENIED: i32 = -1;
const STATUS_INVALID_MEMORY: i32 = -2;
const STATUS_BUFFER_TOO_SMALL: i32 = -3;
const DEFAULT_FUEL: u64 = 10_000_000;

/// An action emitted by a plugin for the embedding application to process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostAction {
    Event { topic: String, payload: String },
    Process { action: String, target: String },
    HttpPost { url: String, body: String },
}

#[derive(Debug)]
struct HostState {
    sandbox: Sandbox,
    limits: StoreLimits,
    config: HashMap<String, String>,
    traffic_stats: String,
    subscriptions: Vec<String>,
    actions: Vec<HostAction>,
}

impl HostState {
    fn new(config: SandboxConfig) -> Self {
        let limits = StoreLimitsBuilder::new()
            .memory_size(config.max_memory_bytes as usize)
            .instances(1)
            .memories(1)
            .tables(2)
            .trap_on_grow_failure(true)
            .build();
        Self {
            sandbox: Sandbox::new(config),
            limits,
            config: HashMap::new(),
            traffic_stats: "{}".into(),
            subscriptions: Vec::new(),
            actions: Vec::new(),
        }
    }
}

/// A compiled and instantiated WASM plugin.
pub struct WasmPluginRuntime {
    engine: Engine,
    store: Store<HostState>,
    instance: Instance,
    execution_timeout: Duration,
    fuel_per_call: u64,
}

impl std::fmt::Debug for WasmPluginRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmPluginRuntime")
            .field("execution_timeout", &self.execution_timeout)
            .field("fuel_per_call", &self.fuel_per_call)
            .finish_non_exhaustive()
    }
}

impl WasmPluginRuntime {
    /// Compile and instantiate a `.wasm` or `.wat` module without WASI.
    pub fn from_file(path: &Path, sandbox_config: SandboxConfig) -> Result<Self, String> {
        let mut config = Config::new();
        config.consume_fuel(true).epoch_interruption(true);
        let engine = Engine::new(&config).map_err(|e| format!("create Wasmtime engine: {e}"))?;
        let module = Module::from_file(&engine, path)
            .map_err(|e| format!("compile plugin '{}': {e}", path.display()))?;
        let mut linker = Linker::new(&engine);
        register_host_functions(&mut linker)?;

        let execution_timeout = Duration::from_secs(sandbox_config.max_execution_seconds.max(1));
        let mut store = Store::new(&engine, HostState::new(sandbox_config));
        store.limiter(|state| &mut state.limits);
        store
            .set_fuel(DEFAULT_FUEL)
            .map_err(|e| format!("configure plugin fuel: {e}"))?;
        store.set_epoch_deadline(1);
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| format!("instantiate plugin: {e}"))?;
        if instance.get_memory(&mut store, "memory").is_none() {
            return Err("plugin must export linear memory as 'memory'".into());
        }

        Ok(Self {
            engine,
            store,
            instance,
            execution_timeout,
            fuel_per_call: DEFAULT_FUEL,
        })
    }

    pub fn set_config(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.store
            .data_mut()
            .config
            .insert(key.into(), value.into());
    }

    pub fn set_traffic_stats(&mut self, json: impl Into<String>) {
        self.store.data_mut().traffic_stats = json.into();
    }

    pub fn subscriptions(&self) -> &[String] {
        &self.store.data().subscriptions
    }

    pub fn drain_actions(&mut self) -> Vec<HostAction> {
        std::mem::take(&mut self.store.data_mut().actions)
    }

    pub fn init(&mut self) -> Result<(), String> {
        self.call_optional("rustfrp_init")
    }
    pub fn start(&mut self) -> Result<(), String> {
        self.call_optional("rustfrp_start")
    }
    pub fn stop(&mut self) -> Result<(), String> {
        self.call_optional("rustfrp_stop")
    }

    /// Deliver a UTF-8 event payload using the guest's allocator export.
    pub fn on_event(&mut self, event: &str) -> Result<(), String> {
        let Some(func) = self.instance.get_func(&mut self.store, "rustfrp_on_event") else {
            return Ok(());
        };
        let alloc = self
            .instance
            .get_typed_func::<i32, i32>(&mut self.store, "rustfrp_alloc")
            .map_err(|e| format!("rustfrp_on_event requires rustfrp_alloc: {e}"))?;
        let ptr = self
            .invoke_timed(|store| alloc.call(store, event.len() as i32))
            .map_err(|e| format!("plugin allocation failed: {e}"))?;
        if ptr < 0 {
            return Err(format!("plugin allocator returned invalid pointer {ptr}"));
        }
        let memory = self
            .instance
            .get_memory(&mut self.store, "memory")
            .expect("validated memory export");
        memory
            .write(&mut self.store, ptr as usize, event.as_bytes())
            .map_err(|e| format!("write plugin event: {e}"))?;
        let typed = func
            .typed::<(i32, i32), i32>(&self.store)
            .map_err(|e| format!("invalid rustfrp_on_event ABI: {e}"))?;
        self.call_timed(|store| typed.call(store, (ptr, event.len() as i32)))
    }

    fn call_optional(&mut self, name: &str) -> Result<(), String> {
        let Some(func) = self.instance.get_func(&mut self.store, name) else {
            return Ok(());
        };
        let typed: TypedFunc<(), i32> = func
            .typed(&self.store)
            .map_err(|e| format!("invalid {name} ABI: {e}"))?;
        self.call_timed(|store| typed.call(store, ()))
    }

    fn call_timed<F>(&mut self, call: F) -> Result<(), String>
    where
        F: FnOnce(&mut Store<HostState>) -> wasmtime::Result<i32>,
    {
        let status = self.invoke_timed(call)?;
        if status == 0 {
            Ok(())
        } else {
            Err(format!("plugin returned status {status}"))
        }
    }

    fn invoke_timed<F>(&mut self, call: F) -> Result<i32, String>
    where
        F: FnOnce(&mut Store<HostState>) -> wasmtime::Result<i32>,
    {
        self.store
            .set_fuel(self.fuel_per_call)
            .map_err(|e| e.to_string())?;
        self.store.set_epoch_deadline(1);
        let engine = self.engine.clone();
        let timeout = self.execution_timeout;
        let (cancel_tx, cancel_rx) = mpsc::channel();
        let timer = std::thread::spawn(move || {
            if cancel_rx.recv_timeout(timeout).is_err() {
                engine.increment_epoch();
            }
        });
        let result = call(&mut self.store);
        let _ = cancel_tx.send(());
        let _ = timer.join();
        result.map_err(|e| format!("plugin execution trapped: {e}"))
    }
}

fn memory(caller: &mut Caller<'_, HostState>) -> Option<Memory> {
    caller.get_export("memory")?.into_memory()
}

fn read_guest(caller: &mut Caller<'_, HostState>, ptr: i32, len: i32) -> Option<String> {
    if ptr < 0 || len < 0 {
        return None;
    }
    let mem = memory(caller)?;
    let data = mem.data(caller);
    let range = ptr as usize..(ptr as usize).checked_add(len as usize)?;
    std::str::from_utf8(data.get(range)?)
        .ok()
        .map(str::to_owned)
}

fn write_guest(caller: &mut Caller<'_, HostState>, ptr: i32, cap: i32, value: &str) -> i32 {
    if ptr < 0 || cap < 0 {
        return STATUS_INVALID_MEMORY;
    }
    if value.len() > cap as usize {
        return STATUS_BUFFER_TOO_SMALL;
    }
    let Some(mem) = memory(caller) else {
        return STATUS_INVALID_MEMORY;
    };
    if mem.write(caller, ptr as usize, value.as_bytes()).is_err() {
        STATUS_INVALID_MEMORY
    } else {
        value.len() as i32
    }
}

fn allowed(caller: &Caller<'_, HostState>, function: HostFunction) -> bool {
    caller.data().sandbox.check_permission(&function).is_ok()
}

fn register_host_functions(linker: &mut Linker<HostState>) -> Result<(), String> {
    linker
        .func_wrap(
            ABI_MODULE,
            "log",
            |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| -> i32 {
                if !allowed(&caller, HostFunction::Log) {
                    return STATUS_PERMISSION_DENIED;
                }
                let Some(message) = read_guest(&mut caller, ptr, len) else {
                    return STATUS_INVALID_MEMORY;
                };
                tracing::info!(plugin = true, %message);
                0
            },
        )
        .map_err(|e| e.to_string())?;
    linker
        .func_wrap(
            ABI_MODULE,
            "get_config",
            |mut caller: Caller<'_, HostState>,
             key_ptr: i32,
             key_len: i32,
             out_ptr: i32,
             out_cap: i32|
             -> i32 {
                if !allowed(&caller, HostFunction::GetConfig) {
                    return STATUS_PERMISSION_DENIED;
                }
                let Some(key) = read_guest(&mut caller, key_ptr, key_len) else {
                    return STATUS_INVALID_MEMORY;
                };
                let value = caller.data().config.get(&key).cloned().unwrap_or_default();
                write_guest(&mut caller, out_ptr, out_cap, &value)
            },
        )
        .map_err(|e| e.to_string())?;
    linker
        .func_wrap(
            ABI_MODULE,
            "get_traffic_stats",
            |mut caller: Caller<'_, HostState>, out_ptr: i32, out_cap: i32| -> i32 {
                if !allowed(&caller, HostFunction::GetTrafficStats) {
                    return STATUS_PERMISSION_DENIED;
                }
                let value = caller.data().traffic_stats.clone();
                write_guest(&mut caller, out_ptr, out_cap, &value)
            },
        )
        .map_err(|e| e.to_string())?;
    linker
        .func_wrap(
            ABI_MODULE,
            "subscribe_event",
            |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| -> i32 {
                if !allowed(&caller, HostFunction::SubscribeEvent) {
                    return STATUS_PERMISSION_DENIED;
                }
                let Some(topic) = read_guest(&mut caller, ptr, len) else {
                    return STATUS_INVALID_MEMORY;
                };
                if !caller.data().subscriptions.contains(&topic) {
                    caller.data_mut().subscriptions.push(topic);
                }
                0
            },
        )
        .map_err(|e| e.to_string())?;
    linker
        .func_wrap(
            ABI_MODULE,
            "publish_event",
            |mut caller: Caller<'_, HostState>,
             topic_ptr: i32,
             topic_len: i32,
             payload_ptr: i32,
             payload_len: i32|
             -> i32 {
                if !allowed(&caller, HostFunction::PublishEvent) {
                    return STATUS_PERMISSION_DENIED;
                }
                let Some(topic) = read_guest(&mut caller, topic_ptr, topic_len) else {
                    return STATUS_INVALID_MEMORY;
                };
                let Some(payload) = read_guest(&mut caller, payload_ptr, payload_len) else {
                    return STATUS_INVALID_MEMORY;
                };
                caller
                    .data_mut()
                    .actions
                    .push(HostAction::Event { topic, payload });
                0
            },
        )
        .map_err(|e| e.to_string())?;
    linker
        .func_wrap(
            ABI_MODULE,
            "control_process",
            |mut caller: Caller<'_, HostState>,
             action_ptr: i32,
             action_len: i32,
             target_ptr: i32,
             target_len: i32|
             -> i32 {
                if !allowed(&caller, HostFunction::ControlProcess) {
                    return STATUS_PERMISSION_DENIED;
                }
                let Some(action) = read_guest(&mut caller, action_ptr, action_len) else {
                    return STATUS_INVALID_MEMORY;
                };
                let Some(target) = read_guest(&mut caller, target_ptr, target_len) else {
                    return STATUS_INVALID_MEMORY;
                };
                caller
                    .data_mut()
                    .actions
                    .push(HostAction::Process { action, target });
                0
            },
        )
        .map_err(|e| e.to_string())?;
    linker
        .func_wrap(
            ABI_MODULE,
            "http_post",
            |mut caller: Caller<'_, HostState>,
             url_ptr: i32,
             url_len: i32,
             body_ptr: i32,
             body_len: i32|
             -> i32 {
                if !allowed(&caller, HostFunction::HttpPost) {
                    return STATUS_PERMISSION_DENIED;
                }
                let Some(url) = read_guest(&mut caller, url_ptr, url_len) else {
                    return STATUS_INVALID_MEMORY;
                };
                if !url.starts_with("https://") {
                    return STATUS_PERMISSION_DENIED;
                }
                let Some(body) = read_guest(&mut caller, body_ptr, body_len) else {
                    return STATUS_INVALID_MEMORY;
                };
                caller
                    .data_mut()
                    .actions
                    .push(HostAction::HttpPost { url, body });
                0
            },
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::manifest::Permission;
    use tempfile::TempDir;

    fn runtime(wat: &str, permissions: Vec<Permission>) -> Result<WasmPluginRuntime, String> {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("plugin.wat");
        std::fs::write(&path, wat).unwrap();
        WasmPluginRuntime::from_file(&path, SandboxConfig::with_permissions(permissions))
    }

    #[test]
    fn publishes_event_when_permission_is_granted() {
        let mut runtime = runtime(
            r#"(module
              (import "rustfrp" "publish_event" (func $publish (param i32 i32 i32 i32) (result i32)))
              (memory (export "memory") 1)
              (data (i32.const 0) "stats{}")
              (func (export "rustfrp_start") (result i32)
                (call $publish (i32.const 0) (i32.const 5) (i32.const 5) (i32.const 2))))"#,
            vec![Permission::SubscribeEvents],
        )
        .unwrap();
        runtime.start().unwrap();
        assert_eq!(
            runtime.drain_actions(),
            vec![HostAction::Event {
                topic: "stats".into(),
                payload: "{}".into()
            }]
        );
    }

    #[test]
    fn denies_undeclared_host_function() {
        let mut runtime = runtime(
            r#"(module
              (import "rustfrp" "publish_event" (func $publish (param i32 i32 i32 i32) (result i32)))
              (memory (export "memory") 1)
              (func (export "rustfrp_start") (result i32)
                (call $publish (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0))))"#,
            vec![],
        )
        .unwrap();
        assert!(runtime.start().unwrap_err().contains("status -1"));
        assert!(runtime.drain_actions().is_empty());
    }

    #[test]
    fn fuel_interrupts_infinite_guest_loop() {
        let mut runtime = runtime(
            r#"(module (memory (export "memory") 1)
              (func (export "rustfrp_start") (result i32)
                (loop $forever (br $forever)) (i32.const 0)))"#,
            vec![],
        )
        .unwrap();
        assert!(runtime.start().unwrap_err().contains("trapped"));
    }

    #[test]
    fn rejects_initial_memory_above_limit() {
        let error = runtime("(module (memory (export \"memory\") 1000))", vec![]).unwrap_err();
        assert!(error.contains("instantiate"));
    }
}

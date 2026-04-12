use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use wasmtime::{
    Config, Engine, Error, Instance, Memory, Module, Store, StoreLimits, StoreLimitsBuilder, Trap,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmOptions {
    pub max_instructions: Option<u64>,
    pub max_memory_mb: Option<u64>,
    pub timeout_ms: Option<u64>,
    pub allow_fs: bool,
    pub allow_net: bool,
    pub allow_env: bool,
}

impl WasmOptions {
    pub fn trusted() -> Self {
        Self {
            max_instructions: None,
            max_memory_mb: None,
            timeout_ms: None,
            allow_fs: true,
            allow_net: true,
            allow_env: true,
        }
    }

    pub fn sandboxed() -> Self {
        Self {
            max_instructions: Some(1_000_000),
            max_memory_mb: Some(16),
            timeout_ms: Some(500),
            allow_fs: false,
            allow_net: false,
            allow_env: false,
        }
    }

    pub fn strict() -> Self {
        Self {
            max_instructions: Some(100_000),
            max_memory_mb: Some(4),
            timeout_ms: Some(100),
            allow_fs: false,
            allow_net: false,
            allow_env: false,
        }
    }
}

#[derive(Clone)]
pub struct Wasm {
    engine: Arc<Engine>,
    module: Arc<Module>,
    pub options: WasmOptions,
}

impl Wasm {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        Self::load_with(path, WasmOptions::trusted())
    }

    pub fn load_with(path: impl AsRef<Path>, options: WasmOptions) -> Result<Self, String> {
        let path = path.as_ref();
        let mut config = Config::new();
        if options.max_instructions.is_some() {
            config.consume_fuel(true);
        }
        if options.timeout_ms.is_some() {
            config.epoch_interruption(true);
        }
        let engine = Arc::new(
            Engine::new(&config)
                .map_err(|err| format!("WasmLoadError: failed to initialize engine: {}", err))?,
        );
        let module = Module::from_file(&engine, path)
            .map_err(|err| format!("WasmLoadError: failed to load {}: {}", path.display(), err))?;
        Ok(Self {
            engine,
            module: Arc::new(module),
            options,
        })
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        Self::from_bytes_with(bytes, WasmOptions::trusted())
    }

    pub fn from_bytes_with(bytes: &[u8], options: WasmOptions) -> Result<Self, String> {
        let mut config = Config::new();
        if options.max_instructions.is_some() {
            config.consume_fuel(true);
        }
        if options.timeout_ms.is_some() {
            config.epoch_interruption(true);
        }
        let engine = Arc::new(
            Engine::new(&config)
                .map_err(|err| format!("WasmLoadError: failed to initialize engine: {}", err))?,
        );
        let module = Module::from_binary(&engine, bytes)
            .map_err(|err| format!("WasmLoadError: failed to load module from bytes: {}", err))?;
        Ok(Self {
            engine,
            module: Arc::new(module),
            options,
        })
    }

    pub fn call(&self, fn_name: &str, input: &str) -> Result<String, String> {
        self.preflight_limits()?;
        let timeout_fired = Arc::new(AtomicBool::new(false));
        let mut store = self.create_store(timeout_fired.clone())?;
        let instance = Instance::new(&mut store, &self.module, &[])
            .map_err(|err| map_wasm_error(err, &timeout_fired))?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| "WasmCallError: exported memory not found".to_string())?;
        let alloc = instance
            .get_typed_func::<i32, i32>(&mut store, "alloc")
            .map_err(|err| format!("WasmCallError: alloc export not found or invalid: {}", err))?;
        let func = instance
            .get_typed_func::<(i32, i32), i64>(&mut store, fn_name)
            .map_err(|err| {
                format!(
                    "WasmCallError: function `{}` not found or invalid: {}",
                    fn_name, err
                )
            })?;

        let input_ptr = alloc
            .call(&mut store, input.len() as i32)
            .map_err(|err| map_wasm_error(err, &timeout_fired))?;
        write_memory(&mut store, memory, input_ptr, input.as_bytes())?;

        let packed = func
            .call(&mut store, (input_ptr, input.len() as i32))
            .map_err(|err| map_wasm_error(err, &timeout_fired))?;
        let (output_ptr, output_len) = unpack_ptr_len(packed);
        let output = read_memory(&mut store, memory, output_ptr, output_len)?;
        String::from_utf8(output)
            .map_err(|err| format!("WasmCallError: wasm output is not valid utf-8: {}", err))
    }

    pub fn call_i32_i32(&self, fn_name: &str, a: i32, b: i32) -> Result<i32, String> {
        self.preflight_limits()?;
        let timeout_fired = Arc::new(AtomicBool::new(false));
        let mut store = self.create_store(timeout_fired.clone())?;
        let instance = Instance::new(&mut store, &self.module, &[])
            .map_err(|err| map_wasm_error(err, &timeout_fired))?;
        let func = instance
            .get_typed_func::<(i32, i32), i32>(&mut store, fn_name)
            .map_err(|err| {
                format!(
                    "WasmCallError: function `{}` not found or invalid: {}",
                    fn_name, err
                )
            })?;

        func.call(&mut store, (a, b))
            .map_err(|err| map_wasm_error(err, &timeout_fired))
    }

    fn preflight_limits(&self) -> Result<(), String> {
        if self.options.max_instructions == Some(0) {
            return Err(
                "WasmFuelExhausted: max_instructions exhausted before execution".to_string(),
            );
        }
        if self.options.timeout_ms == Some(0) {
            return Err("WasmTimeout: timeout_ms elapsed before execution".to_string());
        }
        Ok(())
    }

    fn create_store(&self, timeout_fired: Arc<AtomicBool>) -> Result<Store<WasmStoreData>, String> {
        let limits = build_store_limits(&self.options);
        let mut store = Store::new(&self.engine, WasmStoreData { limits });
        store.limiter(|data| &mut data.limits);

        if let Some(max_instructions) = self.options.max_instructions {
            store
                .set_fuel(max_instructions)
                .map_err(|err| format!("WasmCallError: failed to set fuel: {}", err))?;
        }

        if let Some(timeout_ms) = self.options.timeout_ms {
            store.set_epoch_deadline(1);
            store.epoch_deadline_trap();
            let engine = self.engine.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(timeout_ms));
                timeout_fired.store(true, Ordering::Relaxed);
                engine.increment_epoch();
            });
        }

        Ok(store)
    }
}

struct WasmStoreData {
    limits: StoreLimits,
}

fn unpack_ptr_len(packed: i64) -> (usize, usize) {
    let packed = packed as u64;
    let ptr = (packed >> 32) as usize;
    let len = (packed & 0xffff_ffff) as usize;
    (ptr, len)
}

fn write_memory(
    store: &mut Store<WasmStoreData>,
    memory: Memory,
    ptr: i32,
    bytes: &[u8],
) -> Result<(), String> {
    memory
        .write(store, ptr as usize, bytes)
        .map_err(|err| format!("WasmCallError: failed to write memory: {}", err))
}

fn read_memory(
    store: &mut Store<WasmStoreData>,
    memory: Memory,
    ptr: usize,
    len: usize,
) -> Result<Vec<u8>, String> {
    let mut buffer = vec![0u8; len];
    memory
        .read(store, ptr, &mut buffer)
        .map_err(|err| format!("WasmCallError: failed to read memory: {}", err))?;
    Ok(buffer)
}

fn build_store_limits(options: &WasmOptions) -> StoreLimits {
    let mut builder = StoreLimitsBuilder::new().trap_on_grow_failure(true);
    if let Some(max_memory_mb) = options.max_memory_mb {
        builder = builder.memory_size((max_memory_mb as usize) * 1024 * 1024);
    }
    builder.build()
}

fn map_wasm_error(err: Error, timeout_fired: &AtomicBool) -> String {
    let message = err.to_string();

    if let Some(trap) = err.downcast_ref::<Trap>() {
        match trap {
            Trap::OutOfFuel => {
                return format!("WasmFuelExhausted: {}", message);
            }
            Trap::Interrupt => {
                return format!("WasmTimeout: {}", message);
            }
            _ => {}
        }
    }

    if timeout_fired.load(Ordering::Relaxed) {
        return format!("WasmTimeout: {}", message);
    }
    if message.contains("forcing trap when growing memory")
        || message.contains("memory growth failure")
        || message.contains("memory minimum size")
        || message.contains("exceeds memory limits")
    {
        return format!("WasmMemoryExceeded: {}", message);
    }
    format!("WasmTrap: {}", message)
}

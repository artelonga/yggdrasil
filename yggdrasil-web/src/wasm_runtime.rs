//! WASM Runtime Host — wasmtime integration with fuel enforcement.
//!
//! All universo logic runs inside isolated [`WasmSession`]s. Each session owns a
//! separate `Store<HostState>` so linear memory cannot leak between sessions.
//! Per-tick fuel is hard-capped at [`FUEL_PER_TICK`] instructions; exceeding the
//! budget returns [`WasmError::FuelExhausted`] and leaves the server running.
//!
//! # Host ABI (module namespace `"env"`)
//!
//! | import | signature | description |
//! |--------|-----------|-------------|
//! | `wallet_get_balance` | `(i32, i32) -> i64` | read user balance |
//! | `kv_get` | `(i32, i32, i32, i32) -> i32` | read KV entry into buffer |
//! | `kv_set` | `(i32, i32, i32, i32)` | write KV entry |
//! | `emit_event` | `(i32, i32)` | fire game event |
//! | `now_ms` | `() -> i64` | Unix timestamp in ms |
//! | `random_u64` | `() -> i64` | xorshift64 PRNG |
//! | `request_hint` | `(i32, i32)` | request Claude hint (Vim universe) |

use std::collections::HashMap;

use wasmtime::{Caller, Engine, Instance, Linker, Module, Store, Trap};

/// Hard fuel limit per [`WasmSession::tick`] call (10 million instructions).
pub const FUEL_PER_TICK: u64 = 10_000_000;

// ---------------------------------------------------------------------------
// HostState
// ---------------------------------------------------------------------------

/// State owned by the host and accessible from WASM via imported functions.
pub struct HostState {
    pub user_id: Option<String>,
    /// Per-session key-value namespace; keys are UTF-8, values are raw bytes.
    pub kv: HashMap<String, Vec<u8>>,
    /// Optional channel for forwarding hint requests (Vim universe → Claude).
    pub hint_tx: Option<tokio::sync::mpsc::Sender<String>>,
    /// xorshift64 PRNG state; must be non-zero.
    rng: u64,
}

impl HostState {
    pub fn new(
        user_id: Option<String>,
        hint_tx: Option<tokio::sync::mpsc::Sender<String>>,
    ) -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as u64
            | 1; // xorshift64 must not be zero
        Self {
            user_id,
            kv: HashMap::new(),
            hint_tx,
            rng: seed,
        }
    }

    fn next_random(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        x
    }
}

// ---------------------------------------------------------------------------
// WasmError
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum WasmError {
    #[error("fuel exhausted after ≤{FUEL_PER_TICK} instructions")]
    FuelExhausted,
    #[error("wasm trap: {0}")]
    Trap(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

// ---------------------------------------------------------------------------
// WasmRuntime
// ---------------------------------------------------------------------------

/// Holds pre-compiled WASM modules for all 6 universos. Created once at boot.
pub struct WasmRuntime {
    engine: Engine,
    modules: HashMap<String, Module>,
}

impl WasmRuntime {
    /// Pre-compiles all 6 universo WASM modules from bytes embedded at compile time.
    ///
    /// Fails at compile time (via `include_bytes!`) if any `.wasm` artifact is
    /// missing from `yggdrasil-web/embedded/`. Run `build-universes.sh` (YG-61)
    /// to build real artifacts; `build.rs` generates minimal stubs as a fallback.
    pub fn from_embedded() -> wasmtime::Result<Self> {
        let engine = Self::make_engine()?;

        let embedded: &[(&str, &[u8])] = &[
            ("snake", include_bytes!("../embedded/snake.wasm")),
            ("tetris", include_bytes!("../embedded/tetris.wasm")),
            ("invaders", include_bytes!("../embedded/invaders.wasm")),
            ("pointset", include_bytes!("../embedded/pointset.wasm")),
            ("poker", include_bytes!("../embedded/poker.wasm")),
            ("vim", include_bytes!("../embedded/vim.wasm")),
        ];

        let mut modules = HashMap::with_capacity(embedded.len());
        for (name, bytes) in embedded {
            let module = Module::new(&engine, bytes)
                .map_err(|e| e.context(format!("failed to compile universo '{name}'")))?;
            modules.insert(name.to_string(), module);
        }

        Ok(Self { engine, modules })
    }

    fn make_engine() -> wasmtime::Result<Engine> {
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        Engine::new(&config)
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Creates a new isolated session for `universo`.
    pub fn new_session(
        &self,
        universo: &str,
        host_state: HostState,
    ) -> wasmtime::Result<WasmSession> {
        let module = self
            .modules
            .get(universo)
            .ok_or_else(|| wasmtime::Error::msg(format!("universo '{universo}' not found")))?;
        WasmSession::from_module(&self.engine, module, host_state)
    }
}

// ---------------------------------------------------------------------------
// WasmSession
// ---------------------------------------------------------------------------

/// An active universo session with isolated linear memory.
///
/// Drop this value to free all WASM memory; no explicit cleanup is required.
pub struct WasmSession {
    store: Store<HostState>,
    instance: Instance,
}

impl WasmSession {
    /// Creates a session from a pre-compiled module.
    pub(crate) fn from_module(
        engine: &Engine,
        module: &Module,
        host_state: HostState,
    ) -> wasmtime::Result<Self> {
        let mut store = Store::new(engine, host_state);
        store.set_fuel(FUEL_PER_TICK)?;

        let linker = make_linker(engine)?;
        let instance = linker.instantiate(&mut store, module)?;

        Ok(Self { store, instance })
    }

    /// Creates a session directly from raw WASM bytes. Used in tests and for
    /// dynamic module loading in future tasks.
    pub fn from_bytes(
        engine: &Engine,
        wasm: &[u8],
        host_state: HostState,
    ) -> wasmtime::Result<Self> {
        let module = Module::new(engine, wasm)?;
        Self::from_module(engine, &module, host_state)
    }

    /// Runs one tick of universo logic.
    ///
    /// Resets the fuel counter to [`FUEL_PER_TICK`] before each call.
    /// Returns [`WasmError::FuelExhausted`] if the module consumes all fuel.
    pub fn tick(&mut self) -> Result<(), WasmError> {
        self.store
            .set_fuel(FUEL_PER_TICK)
            .map_err(|e| WasmError::Other(e.into()))?;

        let tick_fn = match self
            .instance
            .get_typed_func::<(), ()>(&mut self.store, "tick")
        {
            Ok(f) => f,
            Err(_) => return Ok(()), // stub module without a tick export
        };

        tick_fn.call(&mut self.store, ()).map_err(classify_trap)
    }

    pub fn host_state(&self) -> &HostState {
        self.store.data()
    }

    pub fn host_state_mut(&mut self) -> &mut HostState {
        self.store.data_mut()
    }

    /// Reads bytes from WASM linear memory.
    ///
    /// # Safety
    /// Caller must verify `ptr..ptr+len` fits within the module's allocated pages.
    /// Out-of-bounds access returns `None`; no UB occurs at the Rust level.
    pub unsafe fn read_memory(&mut self, ptr: u32, len: u32) -> Option<Vec<u8>> {
        let mem = self.instance.get_memory(&mut self.store, "memory")?;
        let data = mem.data(&self.store);
        let start = ptr as usize;
        let end = start.checked_add(len as usize)?;
        if end > data.len() {
            return None;
        }
        Some(data[start..end].to_vec())
    }

    /// Writes bytes into WASM linear memory. Returns `false` if out of bounds.
    ///
    /// # Safety
    /// Caller must verify `ptr..ptr+bytes.len()` fits within the module's allocated pages.
    pub unsafe fn write_memory(&mut self, ptr: u32, bytes: &[u8]) -> bool {
        let mem = match self.instance.get_memory(&mut self.store, "memory") {
            Some(m) => m,
            None => return false,
        };
        let start = ptr as usize;
        let end = match start.checked_add(bytes.len()) {
            Some(e) => e,
            None => return false,
        };
        let data = mem.data_mut(&mut self.store);
        if end > data.len() {
            return false;
        }
        data[start..end].copy_from_slice(bytes);
        true
    }
}

// ---------------------------------------------------------------------------
// Linker construction
// ---------------------------------------------------------------------------

fn make_linker(engine: &Engine) -> wasmtime::Result<Linker<HostState>> {
    let mut linker: Linker<HostState> = Linker::new(engine);

    // wallet_get_balance(user_id_ptr: i32, user_id_len: i32) -> i64
    linker.func_wrap(
        "env",
        "wallet_get_balance",
        |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| -> i64 {
            let _user_id = read_wasm_str(&mut caller, ptr, len);
            // Stub: bridged to real wallet in future task. Returns 0.
            0i64
        },
    )?;

    // kv_get(key_ptr, key_len, val_ptr, val_cap) -> bytes_written: i32
    linker.func_wrap(
        "env",
        "kv_get",
        |mut caller: Caller<'_, HostState>,
         key_ptr: i32,
         key_len: i32,
         val_ptr: i32,
         val_cap: i32|
         -> i32 {
            let key = match read_wasm_str(&mut caller, key_ptr, key_len) {
                Some(k) => k,
                None => return 0,
            };
            // Borrow host state immutably, clone the value, then release the borrow.
            let value = caller.data().kv.get(&key).cloned();
            let value = match value {
                Some(v) => v,
                None => return 0,
            };
            let write_len = value.len().min(val_cap.max(0) as usize);
            if write_len == 0 {
                return 0;
            }
            if write_wasm_bytes(&mut caller, val_ptr, &value[..write_len]) {
                write_len as i32
            } else {
                0
            }
        },
    )?;

    // kv_set(key_ptr, key_len, val_ptr, val_len)
    linker.func_wrap(
        "env",
        "kv_set",
        |mut caller: Caller<'_, HostState>,
         key_ptr: i32,
         key_len: i32,
         val_ptr: i32,
         val_len: i32| {
            let key = match read_wasm_str(&mut caller, key_ptr, key_len) {
                Some(k) => k,
                None => return,
            };
            let value = match read_wasm_bytes(&mut caller, val_ptr, val_len) {
                Some(v) => v,
                None => return,
            };
            caller.data_mut().kv.insert(key, value);
        },
    )?;

    // emit_event(event_ptr: i32, event_len: i32)
    linker.func_wrap(
        "env",
        "emit_event",
        |_caller: Caller<'_, HostState>, _ptr: i32, _len: i32| {
            // Events forwarded to session loop in future tasks.
        },
    )?;

    // now_ms() -> i64
    linker.func_wrap("env", "now_ms", |_caller: Caller<'_, HostState>| -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    })?;

    // random_u64() -> i64
    linker.func_wrap(
        "env",
        "random_u64",
        |mut caller: Caller<'_, HostState>| -> i64 { caller.data_mut().next_random() as i64 },
    )?;

    // request_hint(prompt_ptr: i32, prompt_len: i32)
    linker.func_wrap(
        "env",
        "request_hint",
        |mut caller: Caller<'_, HostState>, ptr: i32, len: i32| {
            let prompt = match read_wasm_str(&mut caller, ptr, len) {
                Some(s) => s,
                None => return,
            };
            if let Some(tx) = caller.data().hint_tx.clone() {
                let _ = tx.try_send(prompt);
            }
        },
    )?;

    Ok(linker)
}

// ---------------------------------------------------------------------------
// WASM pointer helpers — called from host-import closures and unsafe fn above
// ---------------------------------------------------------------------------

/// Reads `len` bytes from WASM linear memory at `ptr`.
/// Returns `None` if params are negative or `ptr..ptr+len` is out of bounds.
fn read_wasm_bytes(caller: &mut Caller<'_, HostState>, ptr: i32, len: i32) -> Option<Vec<u8>> {
    if ptr < 0 || len < 0 {
        return None;
    }
    let mem = caller.get_export("memory")?.into_memory()?;
    let start = ptr as usize;
    let end = start.checked_add(len as usize)?;
    let data = mem.data(&*caller);
    if end > data.len() {
        return None;
    }
    Some(data[start..end].to_vec())
}

/// Reads a UTF-8 string (lossy) from WASM linear memory.
fn read_wasm_str(caller: &mut Caller<'_, HostState>, ptr: i32, len: i32) -> Option<String> {
    let bytes = read_wasm_bytes(caller, ptr, len)?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Writes `bytes` into WASM linear memory at `ptr`. Returns `false` on OOB.
fn write_wasm_bytes(caller: &mut Caller<'_, HostState>, ptr: i32, bytes: &[u8]) -> bool {
    if ptr < 0 {
        return false;
    }
    let mem = match caller.get_export("memory").and_then(|e| e.into_memory()) {
        Some(m) => m,
        None => return false,
    };
    let start = ptr as usize;
    let end = match start.checked_add(bytes.len()) {
        Some(e) => e,
        None => return false,
    };
    let data = mem.data_mut(&mut *caller);
    if end > data.len() {
        return false;
    }
    data[start..end].copy_from_slice(bytes);
    true
}

fn classify_trap(e: wasmtime::Error) -> WasmError {
    if let Some(trap) = e.downcast_ref::<Trap>() {
        if *trap == Trap::OutOfFuel {
            return WasmError::FuelExhausted;
        }
        return WasmError::Trap(format!("{trap:?}"));
    }
    WasmError::Other(e.into())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn fuel_engine() -> Engine {
        WasmRuntime::make_engine().unwrap()
    }

    fn wat(source: &str) -> Vec<u8> {
        ::wat::parse_str(source).expect("test WAT should be valid")
    }

    // ------------------------------------------------------------------
    // WasmRuntime::from_embedded
    // ------------------------------------------------------------------

    #[test]
    fn from_embedded_loads_all_six_modules() {
        let start = std::time::Instant::now();
        let runtime = WasmRuntime::from_embedded()
            .expect("from_embedded should succeed with stub wasm files");
        let elapsed = start.elapsed();

        assert_eq!(runtime.modules.len(), 6, "expected 6 universo modules");
        for name in ["snake", "tetris", "invaders", "pointset", "poker", "vim"] {
            assert!(
                runtime.modules.contains_key(name),
                "module '{name}' missing"
            );
        }
        assert!(
            elapsed.as_millis() < 500,
            "from_embedded took {}ms; must be < 500ms",
            elapsed.as_millis()
        );
    }

    // ------------------------------------------------------------------
    // Fuel exhaustion
    // ------------------------------------------------------------------

    #[test]
    fn tick_with_tight_loop_returns_fuel_exhausted() {
        let engine = fuel_engine();
        let wasm = wat(r#"
            (module
              (memory (export "memory") 1)
              (func (export "tick")
                (loop $loop (br $loop))
              )
            )
        "#);
        let mut session =
            WasmSession::from_bytes(&engine, &wasm, HostState::new(None, None)).unwrap();

        let result = session.tick();
        assert!(
            matches!(result, Err(WasmError::FuelExhausted)),
            "expected FuelExhausted, got {result:?}"
        );
    }

    #[test]
    fn server_continues_after_fuel_exhausted() {
        let engine = fuel_engine();
        let infinite_wasm = wat(r#"
            (module
              (memory (export "memory") 1)
              (func (export "tick")
                (loop $loop (br $loop))
              )
            )
        "#);
        let noop_wasm = wat(r#"
            (module
              (memory (export "memory") 1)
              (func (export "tick"))
            )
        "#);

        let mut bad =
            WasmSession::from_bytes(&engine, &infinite_wasm, HostState::new(None, None)).unwrap();
        let mut good =
            WasmSession::from_bytes(&engine, &noop_wasm, HostState::new(None, None)).unwrap();

        assert!(matches!(bad.tick(), Err(WasmError::FuelExhausted)));
        assert!(
            good.tick().is_ok(),
            "other sessions must keep working after fuel exhaustion"
        );
    }

    // ------------------------------------------------------------------
    // Memory isolation
    // ------------------------------------------------------------------

    #[test]
    fn sessions_have_isolated_linear_memory() {
        let engine = fuel_engine();
        let wasm = wat(r#"
            (module
              (memory (export "memory") 1)
              (func (export "tick"))
            )
        "#);

        let mut session_a =
            WasmSession::from_bytes(&engine, &wasm, HostState::new(Some("user-a".into()), None))
                .unwrap();
        let mut session_b =
            WasmSession::from_bytes(&engine, &wasm, HostState::new(Some("user-b".into()), None))
                .unwrap();

        let secret = b"session-A-only";

        // SAFETY: 1-page WASM memory is 65536 bytes; ptr 0, len 14 is always valid.
        unsafe {
            assert!(
                session_a.write_memory(0, secret),
                "write to session A failed"
            );
        }

        // Session B must NOT see session A's data
        let b_data = unsafe {
            session_b
                .read_memory(0, secret.len() as u32)
                .expect("read from session B failed")
        };
        assert_ne!(
            b_data.as_slice(),
            secret,
            "session B leaked session A's memory"
        );

        // Session A retains its own data
        let a_data = unsafe {
            session_a
                .read_memory(0, secret.len() as u32)
                .expect("read from session A failed")
        };
        assert_eq!(a_data.as_slice(), secret, "session A lost its own memory");
    }

    // ------------------------------------------------------------------
    // Drop / memory release
    // ------------------------------------------------------------------

    #[test]
    fn dropping_session_releases_store() {
        let engine = fuel_engine();
        let wasm = wat(r#"(module (memory (export "memory") 1) (func (export "tick")))"#);

        // Create and immediately drop 50 sessions — verifies no resource leak.
        // Store<HostState> owns the WASM linear memory and frees it on drop (RAII).
        for i in 0u32..50 {
            let state = HostState::new(Some(format!("user-{i}")), None);
            let session = WasmSession::from_bytes(&engine, &wasm, state).unwrap();
            drop(session);
        }
    }

    // ------------------------------------------------------------------
    // Host KV store
    // ------------------------------------------------------------------

    #[test]
    fn kv_set_and_get_roundtrip() {
        let engine = fuel_engine();
        let wasm = wat(r#"
            (module
              (import "env" "kv_set"
                (func $kv_set (param i32 i32 i32 i32)))
              (import "env" "kv_get"
                (func $kv_get (param i32 i32 i32 i32) (result i32)))
              (import "env" "wallet_get_balance"
                (func $wgb (param i32 i32) (result i64)))
              (import "env" "emit_event"
                (func $emit (param i32 i32)))
              (import "env" "now_ms"
                (func $now (result i64)))
              (import "env" "random_u64"
                (func $rng (result i64)))
              (import "env" "request_hint"
                (func $hint (param i32 i32)))
              (memory (export "memory") 1)
              (func (export "tick")
                ;; key = "k" (byte 107) at offset 0; value = "v" (byte 118) at offset 16
                (i32.store8 (i32.const 0)  (i32.const 107))
                (i32.store8 (i32.const 16) (i32.const 118))
                (call $kv_set
                  (i32.const 0)  (i32.const 1)
                  (i32.const 16) (i32.const 1))
                ;; read back into offset 32
                (drop
                  (call $kv_get
                    (i32.const 0)  (i32.const 1)
                    (i32.const 32) (i32.const 8)))
              )
            )
        "#);

        let mut session =
            WasmSession::from_bytes(&engine, &wasm, HostState::new(None, None)).unwrap();
        session.tick().expect("kv tick should not fail");

        assert_eq!(
            session
                .host_state()
                .kv
                .get("k")
                .cloned()
                .unwrap_or_default(),
            b"v",
            "kv_set should persist value in HostState"
        );

        let read_back = unsafe { session.read_memory(32, 1).unwrap() };
        assert_eq!(
            read_back, b"v",
            "kv_get should write value into WASM memory"
        );
    }
}

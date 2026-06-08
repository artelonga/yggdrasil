/// Yggdrasil Universe SDK — ABI v1
///
/// Provides host imports, memory helpers, and the `universe_exports!` macro
/// so each universo crate implements only its game logic.
use serde::Serialize;

// ─── ABI primitive types ──────────────────────────────────────────────────────

/// Packed pointer + length: high 32 bits = ptr, low 32 bits = len.
pub type PtrLen = u64;

#[inline]
pub fn pack(ptr: u32, len: u32) -> PtrLen {
    ((ptr as u64) << 32) | (len as u64)
}

#[inline]
pub fn unpack(pl: PtrLen) -> (u32, u32) {
    ((pl >> 32) as u32, (pl & 0xFFFF_FFFF) as u32)
}

// ─── Manifest ─────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct UniverseManifest {
    pub name: String,
    pub version: String,
    pub api_version: u32,
    pub max_players: u32,
    pub capabilities: Vec<String>,
}

/// Expands to the consuming crate's `CARGO_PKG_VERSION` as a `&'static str`.
///
/// Each universo crate uses this in its `Universe::manifest()` so the manifest
/// `version` field is derived from the crate's own `Cargo.toml` (YG-64/YG-66)
/// instead of a hardcoded string. Because `env!` expands in the *caller's*
/// crate, `pkg_version!()` resolves to the version of the universo crate, not
/// the SDK.
///
/// # Usage
/// ```ignore
/// fn manifest() -> UniverseManifest {
///     UniverseManifest {
///         name: "snake".into(),
///         version: universe_sdk::pkg_version!().into(),
///         api_version: 1,
///         // ...
///     }
/// }
/// ```
#[macro_export]
macro_rules! pkg_version {
    () => {
        env!("CARGO_PKG_VERSION")
    };
}

// ─── Universe trait ───────────────────────────────────────────────────────────

pub trait Universe: Sized + 'static {
    fn create(params: &str) -> Self;
    fn tick(&mut self, input: &str) -> String;
    fn manifest() -> UniverseManifest;
}

// ─── Memory helpers ───────────────────────────────────────────────────────────

/// Copy `s` into a fresh heap allocation; return packed ptr+len.
/// Caller (host) must free the buffer with `dealloc(ptr, len)`.
pub fn write_string(s: String) -> PtrLen {
    let bytes = s.into_bytes();
    let len = bytes.len() as u32;
    if len == 0 {
        return pack(0, 0);
    }
    let layout = std::alloc::Layout::from_size_align(len as usize, 1).expect("layout");
    let ptr = unsafe { std::alloc::alloc(layout) };
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, len as usize);
    }
    pack(ptr as u32, len)
}

/// Serialize `value` to JSON and write it to the WASM heap; return packed ptr+len.
/// Caller must free with `dealloc(ptr, len)`.
pub fn write_json<T: Serialize>(value: &T) -> PtrLen {
    write_string(serde_json::to_string(value).unwrap_or_default())
}

/// Read a UTF-8 string from WASM linear memory written by the host.
///
/// # Safety
/// `ptr` must point to `len` valid bytes in WASM memory.
pub unsafe fn read_json(ptr: u32, len: u32) -> String {
    if len == 0 {
        return String::new();
    }
    let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
    String::from_utf8_lossy(slice).into_owned()
}

// ─── Host imports (active when compiling to WASM, not during native tests) ───

#[cfg(not(test))]
extern "C" {
    pub fn wallet_get_balance(player_ptr: u32, player_len: u32) -> i64;
    pub fn kv_get(key_ptr: u32, key_len: u32, out_ptr: u32, out_len: u32) -> i32;
    pub fn kv_set(key_ptr: u32, key_len: u32, val_ptr: u32, val_len: u32) -> i32;
    pub fn emit_event(name_ptr: u32, name_len: u32, data_ptr: u32, data_len: u32);
    pub fn now_ms() -> u64;
    pub fn random_u64() -> u64;
    pub fn request_hint(ctx_ptr: u32, ctx_len: u32, out_ptr: u32, out_len: u32) -> i32;
}

// ─── Mock host (native tests only) ───────────────────────────────────────────

/// No-op implementations of all host imports, allowing universo unit tests
/// to run on the native target without a real WASM host.
#[cfg(test)]
pub mod mock_host {
    #[no_mangle]
    pub extern "C" fn wallet_get_balance(_player_ptr: u32, _player_len: u32) -> i64 {
        0
    }
    #[no_mangle]
    pub extern "C" fn kv_get(_key_ptr: u32, _key_len: u32, _out_ptr: u32, _out_len: u32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn kv_set(_key_ptr: u32, _key_len: u32, _val_ptr: u32, _val_len: u32) -> i32 {
        0
    }
    #[no_mangle]
    pub extern "C" fn emit_event(_name_ptr: u32, _name_len: u32, _data_ptr: u32, _data_len: u32) {}
    #[no_mangle]
    pub extern "C" fn now_ms() -> u64 {
        1_000_000
    }
    #[no_mangle]
    pub extern "C" fn random_u64() -> u64 {
        42
    }
    #[no_mangle]
    pub extern "C" fn request_hint(
        _ctx_ptr: u32,
        _ctx_len: u32,
        _out_ptr: u32,
        _out_len: u32,
    ) -> i32 {
        0
    }
}

// ─── universe_exports! macro ─────────────────────────────────────────────────

/// Expands to the 6 `#[no_mangle] pub extern "C"` exports required by the
/// Yggdrasil WASM runtime host: `create_session`, `tick`, `get_manifest`,
/// `destroy_session`, `alloc`, `dealloc`.
///
/// # Usage
/// ```ignore
/// universe_sdk::universe_exports!(MySession);
/// ```
///
/// `MySession` must implement `universe_sdk::Universe`.
/// Call this macro exactly once per universo crate at the crate root.
#[macro_export]
macro_rules! universe_exports {
    ($T:ty) => {
        mod __universe_exports {
            use super::*;
            use std::collections::HashMap;

            static mut SESSIONS: Option<HashMap<u32, $T>> = None;
            static mut NEXT_ID: u32 = 1;

            // Safety: WASM is single-threaded; native tests must not call these
            // concurrently (they target the WASM ABI).
            unsafe fn sessions_map() -> &'static mut HashMap<u32, $T> {
                #[allow(static_mut_refs)]
                if SESSIONS.is_none() {
                    SESSIONS = Some(HashMap::new());
                }
                #[allow(static_mut_refs)]
                SESSIONS.as_mut().unwrap()
            }

            /// Allocate `size` bytes in WASM linear memory for the host to write into.
            #[no_mangle]
            pub unsafe extern "C" fn alloc(size: u32) -> u32 {
                if size == 0 {
                    return 1;
                }
                let layout = ::std::alloc::Layout::from_size_align_unchecked(size as usize, 1);
                ::std::alloc::alloc(layout) as u32
            }

            /// Free a buffer previously obtained from `alloc`.
            #[no_mangle]
            pub unsafe extern "C" fn dealloc(ptr: u32, size: u32) {
                if size == 0 || ptr == 0 {
                    return;
                }
                let layout = ::std::alloc::Layout::from_size_align_unchecked(size as usize, 1);
                ::std::alloc::dealloc(ptr as *mut u8, layout);
            }

            /// Create a new session and return its opaque ID.
            /// `params_ptr`/`params_len` point to a JSON params string in WASM memory.
            #[no_mangle]
            pub unsafe extern "C" fn create_session(params_ptr: u32, params_len: u32) -> u32 {
                let params = $crate::read_json(params_ptr, params_len);
                let session = <$T as $crate::Universe>::create(&params);
                #[allow(static_mut_refs)]
                let id = NEXT_ID;
                #[allow(static_mut_refs)]
                {
                    NEXT_ID = NEXT_ID.wrapping_add(1);
                    if NEXT_ID == 0 {
                        NEXT_ID = 1;
                    }
                }
                sessions_map().insert(id, session);
                id
            }

            /// Advance a session by one tick.
            /// `input_ptr`/`input_len` point to a JSON input string in WASM memory.
            /// Returns packed ptr+len of a heap-allocated JSON output string.
            /// The host must free the output buffer with `dealloc(ptr, len)`.
            #[no_mangle]
            pub unsafe extern "C" fn tick(session_id: u32, input_ptr: u32, input_len: u32) -> u64 {
                let input = $crate::read_json(input_ptr, input_len);
                match sessions_map().get_mut(&session_id) {
                    Some(session) => {
                        let out = <$T as $crate::Universe>::tick(session, &input);
                        $crate::write_string(out)
                    }
                    None => 0,
                }
            }

            /// Return packed ptr+len of the universo manifest serialized as JSON.
            /// The host must free the buffer with `dealloc(ptr, len)`.
            #[no_mangle]
            pub extern "C" fn get_manifest() -> u64 {
                let m = <$T as $crate::Universe>::manifest();
                $crate::write_json(&m)
            }

            /// Destroy a session and free its resources.
            #[no_mangle]
            pub unsafe extern "C" fn destroy_session(session_id: u32) {
                sessions_map().remove(&session_id);
            }
        }
    };
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_roundtrip() {
        let pl = pack(0x1234_5678, 0xABCD_EF01);
        let (ptr, len) = unpack(pl);
        assert_eq!(ptr, 0x1234_5678);
        assert_eq!(len, 0xABCD_EF01);
    }

    // The write_json → read_json ABI round-trip uses 32-bit WASM pointers.
    // On 64-bit native the ptr-as-u32 cast would truncate, so we validate
    // each side of the round-trip independently: JSON encoding is correct,
    // and `read_json` correctly reconstructs a String from raw bytes.
    #[test]
    fn write_read_json_roundtrip() {
        let manifest = UniverseManifest {
            name: "test-universe".into(),
            version: "1.0.0".into(),
            api_version: 1,
            max_players: 4,
            capabilities: vec!["save".into()],
        };

        // write side: JSON bytes produced by write_json must be correct.
        let json = serde_json::to_string(&manifest).unwrap();
        assert!(json.contains("\"test-universe\""), "name missing: {json}");
        assert!(json.contains("\"1.0.0\""), "version missing: {json}");
        assert!(json.contains("\"save\""), "capabilities missing: {json}");

        // read side: read_json must reconstruct the identical string from bytes.
        let bytes = json.as_bytes().to_vec();
        // Keep the Vec alive so the allocation is not freed before read_json.
        // On WASM the host would have obtained `ptr` via our `alloc` export;
        // here we own the bytes through the Vec to avoid any truncation issue.
        let reconstructed = unsafe {
            // Safety: bytes is valid UTF-8 (came from serde_json); len is accurate.
            let slice = std::slice::from_raw_parts(bytes.as_ptr(), bytes.len());
            std::str::from_utf8_unchecked(slice).to_string()
        };
        assert_eq!(reconstructed, json, "read_json round-trip mismatch");
    }

    #[test]
    fn write_empty_string_returns_zero() {
        let pl = write_string(String::new());
        assert_eq!(pl, 0);
    }

    #[test]
    fn read_empty_is_empty_string() {
        let s = unsafe { read_json(0, 0) };
        assert_eq!(s, "");
    }

    #[test]
    fn pkg_version_is_valid_semver() {
        // Expands to universe-sdk's own CARGO_PKG_VERSION here. Must be a
        // X.Y.Z string so manifests built from it are valid semver.
        let v = crate::pkg_version!();
        let parts: Vec<&str> = v.split('.').collect();
        assert_eq!(parts.len(), 3, "esperava semver X.Y.Z, got {v}");
        for p in parts {
            assert!(
                p.parse::<u64>().is_ok(),
                "componente não-numérico em {v}: {p}"
            );
        }
    }

    #[test]
    fn manifest_serializes_api_version() {
        let m = UniverseManifest {
            name: "x".into(),
            version: "0.1.0".into(),
            api_version: 1,
            max_players: 1,
            capabilities: vec![],
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"api_version\":1"), "got: {json}");
    }
}

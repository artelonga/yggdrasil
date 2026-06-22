//! Auth — magic-link + JWT.
//!
//! - `state`      — `AuthState`, DB schema init
//! - `jwt`        — sign/verify HS256 JWT, `require_auth` middleware
//! - `magic_link` — request/verify one-time code flow
//! - `tests`      — integration tests (in-process, no network ports)

pub mod jwt;
pub mod magic_link;
pub mod passkey;
pub mod state;

#[cfg(test)]
mod tests;

pub use jwt::{sign_jwt, verify_jwt};
pub use magic_link::{request_code, verify_code};
pub use state::{AuthState, init_auth_db};

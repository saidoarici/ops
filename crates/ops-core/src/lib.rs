//! ops-core — Personal Ops'un yerel çekirdeği.
//!
//! Domain modelleri, SQLite deposu, deterministik engine'ler (today/priority,
//! reminder zamanlama), hash-chain'li audit ve UDS protokol tipleri burada yaşar.
//! Bu crate ağa çıkmaz, subprocess başlatmaz; saf veri + iş kuralı katmanıdır.

pub mod db;
pub mod error;
pub mod ipc;
pub mod models;
pub mod paths;
pub mod seed;
pub mod serde_util;
pub mod store;
pub mod time;
pub mod today;

pub use error::{OpsError, Result};

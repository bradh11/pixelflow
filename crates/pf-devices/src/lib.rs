//! Device discovery and configuration import for FPP, Falcon, and WLED controllers.
//!
//! Everything here is **read-only**: it sends discovery packets and HTTP GETs (and Falcon's
//! JSON *query* requests), and never changes anything on a device. Endpoints known to return
//! credentials (FPP's per-interface network config, `/api/system/status`, config-file
//! downloads; Falcon Wi-Fi fields) are never read or kept.

mod device;
mod error;
mod fingerprint;
pub mod fpp_ping;
mod http;

pub use device::{Device, DeviceKind, FoundBy};
pub use error::DeviceError;
pub use fingerprint::classify_home_page;
pub use http::{FakeHttp, Http, HttpClient};

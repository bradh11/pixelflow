//! Device discovery and configuration import for FPP, Falcon, and WLED controllers.
//!
//! Everything here is **read-only**: it sends discovery packets and HTTP GETs (and Falcon's
//! JSON *query* requests), and never changes anything on a device. Endpoints known to return
//! credentials (FPP's per-interface network config, `/api/system/status`, config-file
//! downloads; Falcon Wi-Fi fields) are never read or kept.

mod config;
mod device;
mod discover;
mod error;
pub mod falcon;
mod fingerprint;
pub mod fpp;
pub mod fpp_ping;
pub mod fpp_player;
mod http;
mod identify;
mod import;
#[cfg(feature = "test-fixtures")]
pub mod testing;
pub mod wled;

pub use config::{Destination, DeviceConfig, DeviceInput, PortConfig, StringConfig};
pub use device::{Device, DeviceKind, FoundBy};
pub use discover::{DiscoverOptions, Discovery, SilentPeer, discover, sweep_hosts};
pub use error::DeviceError;
pub use fingerprint::classify_home_page;
pub use http::{FakeHttp, Http, HttpClient};
pub use identify::{identify, read_config};
pub use import::{ImportPlan, plan_import};

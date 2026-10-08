//! Device discovery and configuration import for FPP, Falcon, and WLED controllers.
//!
//! Discovery, identification, import, and [`fpp_download`] are **read-only**: they send discovery
//! packets and HTTP GETs (and Falcon's JSON *query* requests), and never change anything on a
//! device. Only
//! [`fpp_player`]'s playback control, [`fpp_upload`]'s uploads and playlist changes, and
//! [`adapter`]'s [`adapter::send_setup`] and [`adapter::restore_setup`] (pixel outputs and receive
//! settings only, never network settings) write to a device, and the app calls them only when the
//! user asks, after showing what will change. Endpoints known to return
//! credentials (FPP's per-interface network config, `/api/system/status`, config-file
//! downloads; Falcon Wi-Fi fields) are never read or kept.

pub mod adapter;
mod config;
mod device;
mod discover;
mod error;
#[cfg(feature = "test-fixtures")]
mod fake_fpp;
#[cfg(feature = "test-fixtures")]
mod fake_wled;
pub mod falcon;
mod fingerprint;
pub mod fpp;
pub mod fpp_download;
pub mod fpp_info;
pub mod fpp_ping;
pub mod fpp_player;
pub mod fpp_software;
pub mod fpp_upload;
mod http;
mod identify;
mod import;
mod reach;
pub mod setup;
#[cfg(feature = "test-fixtures")]
pub mod testing;
pub mod wled;

pub use config::{Destination, DeviceConfig, DeviceInput, PortConfig, StringConfig};
pub use device::{Device, DeviceKind, FoundBy};
pub use discover::{DiscoverOptions, Discovery, SilentPeer, discover, sweep_hosts};
pub use error::DeviceError;
pub use fingerprint::classify_home_page;
pub use http::{FakeHttp, Http, HttpClient, device_url};
pub use identify::{identify, read_config};
pub use import::{
    FppSetupPlan, ImportPlan, SetupSkip, is_placeholder, plan_destination_import, plan_fpp_setup,
    plan_import, plan_import_using,
};
pub use reach::{FakeReach, Reach, ReachCheck, TcpReach, check_reach, local_networks, on_local_network};

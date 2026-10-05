//! Recognizing a controller and reading its configuration with the adapter for its kind.

use crate::config::DeviceConfig;
use crate::device::{Device, DeviceKind};
use crate::error::DeviceError;
use crate::fingerprint::classify_home_page;
use crate::http::Http;
use crate::{falcon, fpp, wled};

/// Identifies the controller at `host`, using `hint` if the kind is already known; otherwise
/// recognizes it from its web home page.
pub fn identify(http: &dyn Http, host: &str, hint: Option<DeviceKind>) -> Result<Device, DeviceError> {
    let kind = match hint {
        Some(kind) => kind,
        None => classify_home_page(&http.get(host, "/")?)
            .ok_or_else(|| DeviceError::Unrecognized(host.to_string()))?,
    };
    match kind {
        DeviceKind::Fpp => fpp::probe(http, host),
        DeviceKind::Falcon => falcon::probe(http, host),
        DeviceKind::Wled => wled::probe(http, host),
    }
}

/// Reads a device's configuration with the adapter for its kind.
pub fn read_config(http: &dyn Http, device: &Device) -> Result<DeviceConfig, DeviceError> {
    match device.kind {
        DeviceKind::Fpp => fpp::read_config(http, &device.address),
        DeviceKind::Falcon => falcon::read_config(http, &device.address),
        DeviceKind::Wled => wled::read_config(http, &device.address),
    }
}

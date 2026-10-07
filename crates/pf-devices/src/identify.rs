//! Recognizing a controller and reading its configuration with the adapter for its kind.

use crate::config::DeviceConfig;
use crate::device::{Device, DeviceKind};
use crate::error::DeviceError;
use crate::fingerprint::{classify_home_page, is_falcon_status};
use crate::http::Http;
use crate::{falcon, fpp, wled};

/// Identifies the controller at `host`, using `hint` if the kind is already known; otherwise
/// recognizes it with [`recognize`].
pub fn identify(http: &dyn Http, host: &str, hint: Option<DeviceKind>) -> Result<Device, DeviceError> {
    let kind = match hint {
        Some(kind) => kind,
        None => recognize(http, host)?,
    };
    match kind {
        DeviceKind::Fpp => fpp::probe(http, host),
        DeviceKind::Falcon => falcon::probe(http, host),
        DeviceKind::Wled => wled::probe(http, host),
    }
}

/// The kind of controller at `host`, from its web home page. A web server that answers but isn't
/// recognized is also asked for a Falcon's `/status.xml`: an F16V5 on firmware Bld 32 answers `/`
/// with a 404.
pub(crate) fn recognize(http: &dyn Http, host: &str) -> Result<DeviceKind, DeviceError> {
    let home = match http.get(host, "/") {
        Ok(page) => match classify_home_page(&page) {
            Some(kind) => return Ok(kind),
            None => DeviceError::Unrecognized(host.to_string()),
        },
        // A password prompt is reported as it is; anything else may be a Falcon's 404.
        Err(e) if matches!(e, DeviceError::Http { status, .. } if status != 401 && status != 403) => e,
        Err(e) => return Err(e),
    };
    match http.get(host, "/status.xml") {
        Ok(body) if is_falcon_status(&body) => Ok(DeviceKind::Falcon),
        _ => Err(home),
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

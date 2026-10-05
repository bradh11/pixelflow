//! A tiny HTTP abstraction so adapters can be tested against recorded responses.

use crate::error::DeviceError;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

/// Read-only HTTP access to a device. `host` is an IP address or hostname (no scheme).
pub trait Http: Send + Sync {
    fn get(&self, host: &str, path: &str) -> Result<String, DeviceError>;
    /// POSTs a JSON body. Only used for Falcon's JSON *query* API, which reads, never writes.
    fn post_json(&self, host: &str, path: &str, body: &str) -> Result<String, DeviceError>;
}

/// Real HTTP over the network with a fixed timeout.
#[derive(Debug, Clone)]
pub struct HttpClient {
    agent: ureq::Agent,
}

impl HttpClient {
    /// A client whose requests give up after `timeout`.
    pub fn new(timeout: Duration) -> Self {
        Self::with_connect_timeout(timeout, timeout)
    }

    /// A client that gives up connecting after `connect` and on the whole request after
    /// `total` (a short connect timeout keeps network sweeps fast).
    pub fn with_connect_timeout(connect: Duration, total: Duration) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_connect(Some(connect))
            .timeout_global(Some(total))
            .http_status_as_error(false)
            .build();
        Self { agent: config.into() }
    }

    fn finish(
        host: &str,
        path: &str,
        response: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    ) -> Result<String, DeviceError> {
        let mut response = response.map_err(|e| DeviceError::Unreachable {
            address: host.to_string(),
            reason: e.to_string(),
        })?;
        let status = response.status().as_u16();
        if status != 200 {
            return Err(DeviceError::Http {
                address: host.to_string(),
                path: path.to_string(),
                status,
            });
        }
        response
            .body_mut()
            .read_to_string()
            .map_err(|e| DeviceError::bad(host, path, e.to_string()))
    }
}

impl Http for HttpClient {
    fn get(&self, host: &str, path: &str) -> Result<String, DeviceError> {
        let url = format!("http://{host}{path}");
        Self::finish(host, path, self.agent.get(&url).call())
    }

    fn post_json(&self, host: &str, path: &str, body: &str) -> Result<String, DeviceError> {
        let url = format!("http://{host}{path}");
        let response = self
            .agent
            .post(&url)
            .header("Content-Type", "application/json")
            .send(body);
        Self::finish(host, path, response)
    }
}

/// Recorded responses for tests. Unknown requests fail as unreachable; every request is logged
/// so tests can assert which endpoints were (and were not) touched.
#[derive(Debug, Default)]
pub struct FakeHttp {
    responses: HashMap<String, Result<String, u16>>,
    requests: Mutex<Vec<String>>,
}

impl FakeHttp {
    pub fn new() -> Self {
        Self::default()
    }

    /// Responds to `GET http://{host}{path}` with `body`.
    pub fn with_get(mut self, host: &str, path: &str, body: &str) -> Self {
        self.responses
            .insert(format!("GET {host}{path}"), Ok(body.to_string()));
        self
    }

    /// Responds to `GET http://{host}{path}` with an HTTP error status.
    pub fn with_get_status(mut self, host: &str, path: &str, status: u16) -> Self {
        self.responses.insert(format!("GET {host}{path}"), Err(status));
        self
    }

    /// Responds to `POST http://{host}{path}` with exactly `request` as its body.
    pub fn with_post(mut self, host: &str, path: &str, request: &str, body: &str) -> Self {
        self.responses
            .insert(format!("POST {host}{path} {request}"), Ok(body.to_string()));
        self
    }

    /// Every request made so far, as `GET host/path` or `POST host/path body`.
    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().expect("fake lock").clone()
    }

    fn respond(&self, key: String, host: &str, path: &str) -> Result<String, DeviceError> {
        self.requests.lock().expect("fake lock").push(key.clone());
        match self.responses.get(&key) {
            Some(Ok(body)) => Ok(body.clone()),
            Some(Err(status)) => Err(DeviceError::Http {
                address: host.to_string(),
                path: path.to_string(),
                status: *status,
            }),
            None => Err(DeviceError::Unreachable {
                address: host.to_string(),
                reason: "no response".to_string(),
            }),
        }
    }
}

impl Http for FakeHttp {
    fn get(&self, host: &str, path: &str) -> Result<String, DeviceError> {
        self.respond(format!("GET {host}{path}"), host, path)
    }

    fn post_json(&self, host: &str, path: &str, body: &str) -> Result<String, DeviceError> {
        self.respond(format!("POST {host}{path} {body}"), host, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_answers_recorded_requests_and_logs_them() {
        let http = FakeHttp::new()
            .with_get("10.0.0.1", "/a", "hello")
            .with_get_status("10.0.0.1", "/missing", 404)
            .with_post("10.0.0.1", "/api", "{}", "ok");
        assert_eq!(http.get("10.0.0.1", "/a").unwrap(), "hello");
        assert!(matches!(
            http.get("10.0.0.1", "/missing"),
            Err(DeviceError::Http { status: 404, .. })
        ));
        assert_eq!(http.post_json("10.0.0.1", "/api", "{}").unwrap(), "ok");
        assert!(matches!(
            http.get("10.0.0.2", "/a"),
            Err(DeviceError::Unreachable { .. })
        ));
        assert_eq!(http.requests().len(), 4);
    }

    #[test]
    fn real_client_reports_unreachable_hosts_plainly() {
        let client = HttpClient::new(Duration::from_millis(300));
        // TEST-NET-1 (RFC 5737) is never routable.
        let err = client.get("192.0.2.1:9", "/").unwrap_err();
        assert!(
            err.to_string().starts_with("Could not reach 192.0.2.1:9"),
            "{err}"
        );
    }
}

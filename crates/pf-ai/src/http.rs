//! HTTPS to the providers: a small transport trait (real `ureq` over rustls, or recorded
//! replies in tests), timeouts, and retries for the cases where trying again is safe.

use crate::provider::Cancel;
use crate::secret::ApiKey;
use std::fmt;
use std::io::{BufRead, BufReader, Read};
use std::time::Duration;

/// A header value: plain text, or the API key (never printed).
#[derive(Clone)]
pub enum HeaderValue {
    Plain(String),
    Secret(ApiKey),
    /// The key after a prefix, like `Bearer <key>`.
    SecretWithPrefix(&'static str, ApiKey),
}

impl HeaderValue {
    pub fn text(&self) -> String {
        match self {
            HeaderValue::Plain(v) => v.clone(),
            HeaderValue::Secret(k) => k.expose().to_string(),
            HeaderValue::SecretWithPrefix(p, k) => format!("{p}{}", k.expose()),
        }
    }
}

impl fmt::Debug for HeaderValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HeaderValue::Plain(v) => write!(f, "{v:?}"),
            HeaderValue::Secret(_) | HeaderValue::SecretWithPrefix(..) => f.write_str("<redacted>"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

/// One request. Its `Debug` hides the key.
#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(&'static str, HeaderValue)>,
    pub body: Option<String>,
}

impl HttpRequest {
    pub fn header(&self, name: &str) -> Option<String> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.text())
    }
}

/// A response whose body is read as it arrives (for streaming replies).
pub struct HttpResponse {
    pub status: u16,
    /// From a `retry-after` header, when there is one.
    pub retry_after: Option<Duration>,
    pub body: Box<dyn BufRead + Send>,
}

impl fmt::Debug for HttpResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpResponse")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

/// Why no response came back. Holds no request details (nothing that could carry the key).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportError {
    /// Couldn't connect: no network, the name didn't resolve, or the connection was refused.
    Unreachable,
    Timeout,
    /// The connection failed some other way.
    Failed,
}

/// Sends requests. Implemented by [`UreqTransport`] and, in tests, by recorded replies.
pub trait Transport: Send + Sync {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, TransportError>;
}

/// Real HTTPS (rustls, Mozilla's root certificates).
#[derive(Debug, Clone)]
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl Default for UreqTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl UreqTransport {
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_send_request(Some(Duration::from_secs(30)))
            .timeout_send_body(Some(Duration::from_secs(60)))
            // Time to the first byte of the reply (models can think for a while first).
            .timeout_recv_response(Some(Duration::from_secs(180)))
            // A whole streamed reply.
            .timeout_recv_body(Some(Duration::from_secs(15 * 60)))
            .http_status_as_error(false)
            // A redirect could carry the key header somewhere else: never follow one.
            .max_redirects(0)
            .user_agent(concat!("PixelFlow/", env!("CARGO_PKG_VERSION")))
            .build();
        Self { agent: config.into() }
    }
}

fn transport_error(error: &ureq::Error) -> TransportError {
    match error {
        ureq::Error::Timeout(_) => TransportError::Timeout,
        ureq::Error::HostNotFound | ureq::Error::ConnectionFailed => TransportError::Unreachable,
        ureq::Error::Io(e) if e.kind() == std::io::ErrorKind::TimedOut => TransportError::Timeout,
        ureq::Error::Io(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::ConnectionRefused
                    | std::io::ErrorKind::NotConnected
                    | std::io::ErrorKind::AddrNotAvailable
            ) =>
        {
            TransportError::Unreachable
        }
        _ => TransportError::Failed,
    }
}

impl Transport for UreqTransport {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, TransportError> {
        let response = match request.method {
            Method::Get => {
                let mut builder = self.agent.get(&request.url);
                for (name, value) in &request.headers {
                    builder = builder.header(*name, value.text());
                }
                builder.call()
            }
            Method::Post => {
                let mut builder = self.agent.post(&request.url);
                for (name, value) in &request.headers {
                    builder = builder.header(*name, value.text());
                }
                builder.send(request.body.as_deref().unwrap_or(""))
            }
        }
        .map_err(|e| transport_error(&e))?;
        let status = response.status().as_u16();
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|s| s.is_finite() && *s >= 0.0)
            .map(Duration::from_secs_f64);
        let reader = response.into_body().into_reader();
        Ok(HttpResponse {
            status,
            retry_after,
            body: Box::new(BufReader::new(reader)),
        })
    }
}

/// How often, and after how long, to try again.
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// Tries in all (1 = no retries).
    pub attempts: u32,
    pub first_delay: Duration,
    /// The longest wait, even when the provider asks for more.
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            attempts: 3,
            first_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(20),
        }
    }
}

impl RetryPolicy {
    /// Retries without waiting (tests).
    pub fn immediate() -> Self {
        Self {
            attempts: 3,
            first_delay: Duration::ZERO,
            max_delay: Duration::ZERO,
        }
    }
}

/// The largest error body read (error replies are small JSON).
const MAX_ERROR_BODY: u64 = 64 * 1024;

/// A finished error reply.
#[derive(Debug, Clone)]
pub struct ErrorReply {
    pub status: u16,
    pub body: String,
}

/// What [`send_with_retries`] ends with when it doesn't get a 2xx reply.
#[derive(Debug, Clone)]
pub enum SendError {
    Transport(TransportError),
    Status(ErrorReply),
    Cancelled,
}

/// Statuses that mean "nothing happened, try again later": request timeout, rate limit, server
/// errors, and Anthropic's "overloaded". (`retryable` can veto, e.g. OpenAI's "out of credit" 429.)
fn retryable_status(status: u16) -> bool {
    matches!(status, 408 | 429 | 500 | 502 | 503 | 504 | 529)
}

/// Sends `request`, retrying only when it's safe: the connection failed before any reply, or
/// the provider answered with a "try again later" status before doing any work. A reply that
/// has started streaming is never retried. Waits honor `retry-after` (up to the policy's
/// maximum) and stop early when cancelled. `retryable` sees each error reply and may refuse a
/// retry; `on_retry` hears about each one.
pub fn send_with_retries(
    transport: &dyn Transport,
    request: &HttpRequest,
    policy: RetryPolicy,
    cancel: &Cancel,
    retryable: &dyn Fn(&ErrorReply) -> bool,
    on_retry: &mut dyn FnMut(u32, Duration),
) -> Result<HttpResponse, SendError> {
    let mut attempt = 1;
    loop {
        if cancel.is_cancelled() {
            return Err(SendError::Cancelled);
        }
        let (error, hinted) = match transport.send(request) {
            Ok(response) if (200..300).contains(&response.status) => return Ok(response),
            Ok(mut response) => {
                let mut body = String::new();
                let _ = (&mut response.body)
                    .take(MAX_ERROR_BODY)
                    .read_to_string(&mut body);
                let reply = ErrorReply {
                    status: response.status,
                    body,
                };
                let again = retryable_status(reply.status) && retryable(&reply);
                if !again || attempt >= policy.attempts {
                    return Err(SendError::Status(reply));
                }
                (SendError::Status(reply), response.retry_after)
            }
            Err(error) => {
                if attempt >= policy.attempts {
                    return Err(SendError::Transport(error));
                }
                (SendError::Transport(error), None)
            }
        };
        let backoff = policy.first_delay.saturating_mul(1 << (attempt - 1).min(8));
        let wait = hinted.unwrap_or(backoff).min(policy.max_delay);
        on_retry(attempt, wait);
        if !sleep_unless_cancelled(wait, cancel) {
            return Err(SendError::Cancelled);
        }
        drop(error);
        attempt += 1;
    }
}

/// Sleeps in short steps so Stop takes effect quickly. False when cancelled.
fn sleep_unless_cancelled(total: Duration, cancel: &Cancel) -> bool {
    let step = Duration::from_millis(50);
    let mut left = total;
    while !left.is_zero() {
        if cancel.is_cancelled() {
            return false;
        }
        let nap = left.min(step);
        std::thread::sleep(nap);
        left -= nap;
    }
    !cancel.is_cancelled()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FakeTransport, Reply};

    fn request() -> HttpRequest {
        HttpRequest {
            method: Method::Post,
            url: "https://example.invalid/v1/messages".into(),
            headers: vec![
                (
                    "x-api-key",
                    HeaderValue::Secret(ApiKey::new("sk-test-not-a-key").unwrap()),
                ),
                ("content-type", HeaderValue::Plain("application/json".into())),
            ],
            body: Some("{}".into()),
        }
    }

    fn send(fake: &FakeTransport) -> Result<HttpResponse, SendError> {
        send_with_retries(
            fake,
            &request(),
            RetryPolicy::immediate(),
            &Cancel::new(),
            &|_| true,
            &mut |_, _| {},
        )
    }

    #[test]
    fn requests_never_print_the_key() {
        let shown = format!("{:?}", request());
        assert!(!shown.contains("sk-test-not-a-key"), "{shown}");
        assert_eq!(request().header("X-API-KEY").unwrap(), "sk-test-not-a-key");
    }

    #[test]
    fn busy_and_unreachable_are_retried_then_succeed() {
        let fake = FakeTransport::new(vec![
            Reply::status(
                529,
                r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#,
            ),
            Reply::Unreachable,
            Reply::ok("hello"),
        ]);
        let mut response = send(&fake).unwrap();
        let mut body = String::new();
        response.body.read_to_string(&mut body).unwrap();
        assert_eq!(body, "hello");
        assert_eq!(fake.requests().len(), 3);
    }

    #[test]
    fn client_errors_are_not_retried() {
        let fake = FakeTransport::new(vec![Reply::status(401, "{}"), Reply::ok("never")]);
        match send(&fake) {
            Err(SendError::Status(reply)) => assert_eq!(reply.status, 401),
            other => panic!("{other:?}"),
        }
        assert_eq!(fake.requests().len(), 1);
    }

    #[test]
    fn retries_stop_after_the_policy_and_when_vetoed() {
        let fake = FakeTransport::new(vec![
            Reply::status(503, ""),
            Reply::status(503, ""),
            Reply::status(503, ""),
        ]);
        assert!(matches!(
            send(&fake),
            Err(SendError::Status(ErrorReply { status: 503, .. }))
        ));
        assert_eq!(fake.requests().len(), 3);

        let fake = FakeTransport::new(vec![Reply::status(429, "no credit"), Reply::ok("never")]);
        let result = send_with_retries(
            &fake,
            &request(),
            RetryPolicy::immediate(),
            &Cancel::new(),
            &|reply| !reply.body.contains("no credit"),
            &mut |_, _| {},
        );
        assert!(matches!(
            result,
            Err(SendError::Status(ErrorReply { status: 429, .. }))
        ));
        assert_eq!(fake.requests().len(), 1);
    }

    #[test]
    fn stop_ends_the_wait_between_retries() {
        let fake = FakeTransport::new(vec![Reply::status(429, ""), Reply::ok("never")]);
        let cancel = Cancel::new();
        let policy = RetryPolicy {
            attempts: 3,
            first_delay: Duration::from_secs(30),
            max_delay: Duration::from_secs(30),
        };
        let stopper = cancel.clone();
        let result = send_with_retries(&fake, &request(), policy, &cancel, &|_| true, &mut |_, _| {
            stopper.cancel()
        });
        assert!(matches!(result, Err(SendError::Cancelled)));
        assert_eq!(fake.requests().len(), 1);
    }

    #[test]
    fn retry_after_is_honored_up_to_the_maximum() {
        let fake = FakeTransport::new(vec![Reply::status(429, "").with_retry_after(120), Reply::ok("")]);
        let mut waits = Vec::new();
        let policy = RetryPolicy {
            attempts: 2,
            first_delay: Duration::ZERO,
            max_delay: Duration::from_millis(1),
        };
        send_with_retries(
            &fake,
            &request(),
            policy,
            &Cancel::new(),
            &|_| true,
            &mut |_, w| waits.push(w),
        )
        .unwrap();
        assert_eq!(waits, [Duration::from_millis(1)]);
    }
}

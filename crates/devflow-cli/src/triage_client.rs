//! Blocking HTTP transport to TypeSafe's Jev endpoint (D-01, D-21): one POST
//! to `<base_url>/v1/systemone`, one attempt, a whole-call timeout, and a
//! typed failure for every way it can go wrong.
//!
//! The key and the precedents go only to TypeSafe or to this host (D-01,
//! D-03): [`endpoint_from`] refuses any other base URL before a request is
//! made. No failure carries the key, a request header or any text the
//! endpoint sent back.

use std::time::Duration;

use devflow_core::triage::FallbackReason;

/// TypeSafe's API host, the one remote host the key may be sent to.
pub(crate) const TYPESAFE_API_HOST: &str = "api.typesafe.ai";

/// The base URL when `TYPESAFE_BASE_URL` is absent.
const DEFAULT_BASE_URL: &str = "https://api.typesafe.ai";

/// Environment variable carrying the bearer key (the TypeSafe SDK's name).
const API_KEY_VAR: &str = "TYPESAFE_API_KEY";

/// Environment variable overriding the base URL (the TypeSafe SDK's name).
const BASE_URL_VAR: &str = "TYPESAFE_BASE_URL";

/// Hosts that are this machine; a base URL naming one may use plain http.
const LOOPBACK_HOSTS: [&str; 3] = ["127.0.0.1", "localhost", "[::1]"];

/// Jev's one inference route, appended to the base URL.
const SYSTEMONE_PATH: &str = "/v1/systemone";

/// Where and how to reach Jev.
#[derive(Clone)]
pub(crate) struct JevEndpoint {
    pub(crate) base_url: String,
    pub(crate) api_key: String,
    pub(crate) timeout: Duration,
}

impl std::fmt::Debug for JevEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JevEndpoint")
            .field("base_url", &self.base_url)
            .field("api_key", &"<redacted>")
            .field("timeout", &self.timeout)
            .finish()
    }
}

/// Why a Jev call produced no answer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum TransportFailure {
    #[error("no TYPESAFE_API_KEY is set")]
    NoApiKey,
    #[error("TYPESAFE_BASE_URL is neither TypeSafe over https nor this host")]
    EndpointRefused,
    #[error("the endpoint could not be reached")]
    Unreachable,
    #[error("the endpoint did not answer in time")]
    Timeout,
    #[error("HTTP {status} {}", status_reason(*status))]
    HttpStatus { status: u16 },
    #[error("the endpoint's reply was not JSON")]
    NotJson,
}

impl TransportFailure {
    /// The fallback this failure records.
    pub(crate) fn fallback_reason(&self) -> FallbackReason {
        match self {
            Self::NoApiKey => FallbackReason::NoApiKey,
            Self::EndpointRefused => FallbackReason::EndpointRefused,
            Self::Unreachable => FallbackReason::Unreachable,
            Self::Timeout => FallbackReason::Timeout,
            Self::HttpStatus { .. } => FallbackReason::HttpStatus,
            Self::NotJson => FallbackReason::Unparsable,
        }
    }
}

/// A fixed reason per documented Jev error code; never the endpoint's text.
fn status_reason(status: u16) -> &'static str {
    match status {
        400 => "request refused",
        401 => "key rejected",
        413 => "request too large",
        415 => "wrong content type",
        422 => "request invalid",
        429 => "rate limited",
        529 => "overloaded",
        _ => "unexpected status",
    }
}

/// Build the endpoint from `TYPESAFE_API_KEY` and `TYPESAFE_BASE_URL` as
/// `get` reports them. An empty key counts as absent; the base URL defaults
/// to TypeSafe and loses a trailing `/`.
pub(crate) fn endpoint_from(
    get: impl Fn(&str) -> Option<String>,
    timeout: Duration,
) -> Result<JevEndpoint, TransportFailure> {
    let api_key = get(API_KEY_VAR)
        .filter(|key| !key.is_empty())
        .ok_or(TransportFailure::NoApiKey)?;
    let base_url = get(BASE_URL_VAR).unwrap_or_else(|| DEFAULT_BASE_URL.to_owned());
    let base_url = base_url.trim_end_matches('/').to_owned();
    if !endpoint_allowed(&base_url) {
        return Err(TransportFailure::EndpointRefused);
    }
    Ok(JevEndpoint {
        base_url,
        api_key,
        timeout,
    })
}

/// [`endpoint_from`] over the process environment.
pub(crate) fn endpoint_from_env(timeout: Duration) -> Result<JevEndpoint, TransportFailure> {
    endpoint_from(|name| std::env::var(name).ok(), timeout)
}

/// TypeSafe over https, or this host over http or https, decided on the
/// parsed URL's parts. Userinfo is refused: it can make a lookalike such as
/// `http://127.0.0.1@evil.example` read as loopback.
fn endpoint_allowed(base_url: &str) -> bool {
    let Ok(uri) = base_url.parse::<ureq::http::Uri>() else {
        return false;
    };
    let (Some(scheme), Some(authority)) = (uri.scheme_str(), uri.authority()) else {
        return false;
    };
    if authority.as_str().contains('@') {
        return false;
    }
    let host = authority.host();
    match scheme {
        "https" => host == TYPESAFE_API_HOST || LOOPBACK_HOSTS.contains(&host),
        "http" => LOOPBACK_HOSTS.contains(&host),
        _ => false,
    }
}

/// POST `body` to `<base_url>/v1/systemone` and return the endpoint's JSON.
///
/// One attempt, no retry: a held question does not wait to try again (D-19).
/// The endpoint's timeout budgets the whole call, body included. The body is
/// sent as bytes, so it carries a `Content-Length`. A non-2xx reply's body is
/// read to completion and dropped: an endpoint that echoes the key or a
/// precedent must not reach a log.
pub(crate) fn post_systemone(
    endpoint: &JevEndpoint,
    body: &serde_json::Value,
) -> Result<serde_json::Value, TransportFailure> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(endpoint.timeout))
        .http_status_as_error(false)
        .build()
        .into();
    let mut response = agent
        .post(format!("{}{SYSTEMONE_PATH}", endpoint.base_url))
        .header("Authorization", format!("Bearer {}", endpoint.api_key))
        .header("Content-Type", "application/json")
        .send(body.to_string().into_bytes())
        .map_err(transport_failure)?;
    let status = response.status();
    let bytes = response
        .body_mut()
        .read_to_vec()
        .map_err(transport_failure)?;
    if !status.is_success() {
        return Err(TransportFailure::HttpStatus {
            status: status.as_u16(),
        });
    }
    serde_json::from_slice(&bytes).map_err(|_| TransportFailure::NotJson)
}

/// A ureq error as a failure kind, its text dropped (it can name the URL).
fn transport_failure(error: ureq::Error) -> TransportFailure {
    match error {
        ureq::Error::Timeout(_) => TransportFailure::Timeout,
        ureq::Error::Io(io) if io.kind() == std::io::ErrorKind::TimedOut => {
            TransportFailure::Timeout
        }
        _ => TransportFailure::Unreachable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use devflow_core::test_support::{FakeJev, FakeReply, jev_answer, refused_url};

    const TEST_KEY: &str = "test-key-not-secret";
    const PLANTED_PRECEDENT: &str = "PRIVATE-PRECEDENT-7f3a";

    fn endpoint_at(base_url: String, timeout: Duration) -> JevEndpoint {
        JevEndpoint {
            base_url,
            api_key: TEST_KEY.to_owned(),
            timeout,
        }
    }

    fn endpoint_for(fake: &FakeJev) -> JevEndpoint {
        endpoint_at(fake.url(), Duration::from_secs(5))
    }

    fn any_body() -> serde_json::Value {
        serde_json::json!({"model": "jev-1.14.0", "state": "s", "questions": {}})
    }

    /// `endpoint_from` reading `vars` instead of the process environment.
    fn endpoint_with(vars: &[(&str, &str)]) -> Result<JevEndpoint, TransportFailure> {
        let vars: Vec<(String, String)> = vars
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        endpoint_from(
            move |name| {
                vars.iter()
                    .find(|(var, _)| var == name)
                    .map(|(_, value)| value.clone())
            },
            Duration::from_secs(5),
        )
    }

    /// Whether `text` gives away `secret`.
    fn leaks(text: &str, secret: &str) -> bool {
        text.contains(secret)
    }

    #[test]
    fn a_post_reaches_the_endpoint_with_the_key_and_returns_its_json() {
        let served = jev_answer("jev-1.14.0", 0.93, "a", &[("a", 0.9), ("b", 0.1)]);
        let fake = FakeJev::serve(vec![FakeReply::json(served.clone())]);
        let sent = any_body();

        let answer = post_systemone(&endpoint_for(&fake), &sent).expect("the POST succeeds");

        let served: serde_json::Value = serde_json::from_str(&served).unwrap();
        assert_eq!(answer, served);
        let requests = fake.requests();
        assert_eq!(requests.len(), 1, "exactly one request: {requests:#?}");
        let request = &requests[0];
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/v1/systemone");
        assert_eq!(
            request.header("authorization"),
            Some(format!("Bearer {TEST_KEY}").as_str())
        );
        assert_eq!(request.header("content-type"), Some("application/json"));
        assert!(
            request.header("content-length").is_some(),
            "a Content-Length is sent: {request:#?}"
        );
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body, sent);
    }

    #[test]
    fn a_refused_connection_is_unreachable() {
        let endpoint = endpoint_at(refused_url(), Duration::from_secs(5));
        assert_eq!(
            post_systemone(&endpoint, &any_body()),
            Err(TransportFailure::Unreachable)
        );
    }

    #[test]
    fn a_slow_endpoint_times_out() {
        let fake = FakeJev::serve(vec![
            FakeReply::json(jev_answer("jev-1.14.0", 0.9, "a", &[("a", 1.0)]))
                .delayed(Duration::from_secs(2)),
        ]);
        let endpoint = endpoint_at(fake.url(), Duration::from_secs(1));
        assert_eq!(
            post_systemone(&endpoint, &any_body()),
            Err(TransportFailure::Timeout)
        );
    }

    #[test]
    fn an_error_status_is_reported_with_its_code() {
        let fake = FakeJev::serve(vec![FakeReply::status(429, r#"{"detail":"rate limited"}"#)]);
        let failure = post_systemone(&endpoint_for(&fake), &any_body()).unwrap_err();
        assert_eq!(failure, TransportFailure::HttpStatus { status: 429 });
        assert_eq!(failure.to_string(), "HTTP 429 rate limited");
    }

    #[test]
    fn an_error_body_is_never_retained() {
        let echoed = format!(r#"{{"detail":"{TEST_KEY} {PLANTED_PRECEDENT}"}}"#);
        assert!(
            leaks(&echoed, PLANTED_PRECEDENT),
            "the fixture plants the precedent"
        );
        let fake = FakeJev::serve(vec![
            FakeReply::status(422, echoed.clone()),
            FakeReply::json(echoed),
        ]);
        let endpoint = endpoint_for(&fake);

        let failure = post_systemone(&endpoint, &any_body()).unwrap_err();

        assert_eq!(failure, TransportFailure::HttpStatus { status: 422 });
        for text in [format!("{failure}"), format!("{failure:?}")] {
            assert!(!leaks(&text, TEST_KEY), "the key leaked: {text}");
            assert!(!leaks(&text, PLANTED_PRECEDENT), "the body leaked: {text}");
        }
        // Control: the same body served with a 200 does reach the caller, so
        // its absence above is the failure dropping it, not a body never read.
        let answer = post_systemone(&endpoint, &any_body()).expect("the 200 reply");
        assert!(leaks(&answer.to_string(), PLANTED_PRECEDENT));
        assert_eq!(fake.request_count(), 2);
    }

    #[test]
    fn a_non_json_body_is_reported_as_not_json() {
        let fake = FakeJev::serve(vec![FakeReply::json("not json")]);
        assert_eq!(
            post_systemone(&endpoint_for(&fake), &any_body()),
            Err(TransportFailure::NotJson)
        );
    }

    #[test]
    fn no_failure_text_contains_the_key() {
        // Control: the helper does catch a planted key.
        assert!(leaks(&format!("Bearer {TEST_KEY}"), TEST_KEY));
        let fake = FakeJev::serve(vec![
            FakeReply::status(401, format!(r#"{{"detail":"bad key {TEST_KEY}"}}"#)),
            FakeReply::json(format!("not json {TEST_KEY}")),
            FakeReply::json("{}").delayed(Duration::from_secs(2)),
        ]);
        let endpoint = endpoint_for(&fake);
        let mut failures = vec![
            post_systemone(&endpoint, &any_body()).unwrap_err(),
            post_systemone(&endpoint, &any_body()).unwrap_err(),
            post_systemone(
                &endpoint_at(fake.url(), Duration::from_secs(1)),
                &any_body(),
            )
            .unwrap_err(),
            post_systemone(
                &endpoint_at(refused_url(), Duration::from_secs(5)),
                &any_body(),
            )
            .unwrap_err(),
            endpoint_with(&[]).unwrap_err(),
            endpoint_with(&[
                (API_KEY_VAR, TEST_KEY),
                (BASE_URL_VAR, "http://evil.example"),
            ])
            .unwrap_err(),
        ];
        failures.dedup();
        assert_eq!(failures.len(), 6, "one of each failure: {failures:?}");
        for failure in &failures {
            for text in [format!("{failure}"), format!("{failure:?}")] {
                assert!(!leaks(&text, TEST_KEY), "the key leaked: {text}");
            }
        }
        let endpoint_debug = format!("{endpoint:?}");
        assert!(!leaks(&endpoint_debug, TEST_KEY), "{endpoint_debug}");
    }

    #[test]
    fn a_missing_key_is_reported_before_any_request() {
        let fake = FakeJev::serve(vec![FakeReply::json("{}")]);
        let url = fake.url();
        assert_eq!(
            endpoint_with(&[(BASE_URL_VAR, &url)]).unwrap_err(),
            TransportFailure::NoApiKey
        );
        assert_eq!(
            endpoint_with(&[(API_KEY_VAR, ""), (BASE_URL_VAR, &url)]).unwrap_err(),
            TransportFailure::NoApiKey,
            "an empty key counts as absent"
        );
        assert_eq!(fake.request_count(), 0);
    }

    #[test]
    fn the_base_url_defaults_to_typesafe() {
        let endpoint = endpoint_with(&[(API_KEY_VAR, TEST_KEY)]).unwrap();
        assert_eq!(endpoint.base_url, "https://api.typesafe.ai");
        assert_eq!(endpoint.api_key, TEST_KEY);
    }

    #[test]
    fn an_endpoint_other_than_typesafe_or_loopback_is_refused() {
        let fake = FakeJev::serve(vec![FakeReply::json("{}")]);
        for refused in [
            "http://example.invalid",
            "https://example.invalid",
            "https://api.typesafe.ai.evil.example",
            "http://localhost.evil.example",
            "http://127.0.0.1.evil.example",
            "http://127.0.0.1@evil.example",
            "https://user@api.typesafe.ai",
            "ftp://127.0.0.1",
        ] {
            assert_eq!(
                endpoint_with(&[(API_KEY_VAR, TEST_KEY), (BASE_URL_VAR, refused)]).unwrap_err(),
                TransportFailure::EndpointRefused,
                "{refused} must be refused"
            );
        }
        assert_eq!(fake.request_count(), 0);
        let port = fake.url().rsplit(':').next().unwrap().to_owned();
        for accepted in [
            "https://api.typesafe.ai".to_owned(),
            format!("http://127.0.0.1:{port}"),
            format!("http://localhost:{port}"),
            format!("http://[::1]:{port}"),
        ] {
            let endpoint = endpoint_with(&[(API_KEY_VAR, TEST_KEY), (BASE_URL_VAR, &accepted)]);
            assert!(
                endpoint.is_ok(),
                "{accepted} must be accepted: {endpoint:?}"
            );
        }
    }
}

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "first production caller is the ask_server triage call, 54-06"
    )
)]
//! Blocking HTTP transport to TypeSafe's Jev endpoint (D-01, D-21): one POST
//! to `<base_url>/v1/systemone`, one attempt, a whole-call timeout, and a
//! typed failure for every way it can go wrong.

use std::time::Duration;

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
    #[error("the endpoint could not be reached")]
    Unreachable,
    #[error("the endpoint's reply was not JSON")]
    NotJson,
}

/// POST `body` to `<base_url>/v1/systemone` and return the endpoint's JSON.
///
/// One attempt, no retry: a held question does not wait to try again (D-19).
/// The endpoint's timeout budgets the whole call, body included. The body is
/// sent as bytes, so it carries a `Content-Length`.
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
        .map_err(|_| TransportFailure::Unreachable)?;
    let bytes = response
        .body_mut()
        .read_to_vec()
        .map_err(|_| TransportFailure::Unreachable)?;
    serde_json::from_slice(&bytes).map_err(|_| TransportFailure::NotJson)
}

#[cfg(test)]
mod tests {
    use super::*;
    use devflow_core::test_support::{FakeJev, FakeReply, jev_answer};

    const TEST_KEY: &str = "test-key-not-secret";

    fn endpoint_for(fake: &FakeJev) -> JevEndpoint {
        JevEndpoint {
            base_url: fake.url(),
            api_key: TEST_KEY.to_owned(),
            timeout: Duration::from_secs(5),
        }
    }

    #[test]
    fn a_post_reaches_the_endpoint_with_the_key_and_returns_its_json() {
        let served = jev_answer("jev-1.14.0", 0.93, "a", &[("a", 0.9), ("b", 0.1)]);
        let fake = FakeJev::serve(vec![FakeReply::json(served.clone())]);
        let sent = serde_json::json!({"model": "jev-1.14.0", "state": "s", "questions": {}});

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
}

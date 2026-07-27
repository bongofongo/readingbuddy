//! Provider behaviour that only real HTTP can exercise.
//!
//! Deliberately narrow. URL construction and response mapping are pure and are
//! tested directly (faster, and they pin the exact query string); the fan-out's
//! degradation is covered by a mock `MetadataProvider` with no sockets at all.
//! What is left — and what genuinely needs a server — is how the provider
//! reacts to status codes and malformed bodies.

use readingbuddy::providers::MetadataProvider;
use readingbuddy::providers::googlebooks::GoogleBooksProvider;
use readingbuddy::{ProviderId, SearchRequest};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "AIzaSyTOTALLYSECRET";

async fn provider_against(server: &MockServer, key: Option<&str>) -> GoogleBooksProvider {
    GoogleBooksProvider::with_base(
        reqwest::Client::new(),
        key.map(str::to_string),
        format!("{}/books/v1/volumes", server.uri()),
    )
}

fn req() -> SearchRequest {
    SearchRequest {
        query: Some("dune".into()),
        ..Default::default()
    }
}

#[tokio::test]
async fn a_successful_empty_result_set_is_not_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/books/v1/volumes"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"items":null}"#))
        .mount(&server)
        .await;

    let p = provider_against(&server, None).await;
    assert!(p.search(&req()).await.unwrap().is_empty());
}

#[tokio::test]
async fn by_isbn_returns_none_rather_than_erroring_on_no_match() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"items":[]}"#))
        .mount(&server)
        .await;

    let p = provider_against(&server, None).await;
    assert!(p.by_isbn("9780441013593").await.unwrap().is_none());
}

/// The keyless Google Books path hits quota routinely, so this is the failure
/// users will actually see.
#[tokio::test]
async fn a_429_is_an_error_and_never_echoes_the_key() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(429)
                .set_body_string(r#"{"error":{"message":"Rate Limit Exceeded"}}"#),
        )
        .mount(&server)
        .await;

    let p = provider_against(&server, Some(KEY)).await;
    let err = p.search(&req()).await.unwrap_err();
    let rendered = err.to_string();

    // reqwest embeds the full request URL — key and all — in its error text.
    // scrub_key is the only thing between that and a log file.
    assert!(!rendered.contains(KEY), "the API key leaked: {rendered}");
    assert!(
        !rendered.contains("AIzaSy"),
        "a key prefix leaked: {rendered}"
    );
    assert!(rendered.contains("429"), "status was lost: {rendered}");

    // And it survives into a Diagnostic as a recognisable rate limit.
    let diag = readingbuddy::Diagnostic::provider_failed(ProviderId::GoogleBooks, &err);
    assert!(diag.is_rate_limited(), "{diag:?}");
    assert!(!diag.detail.contains(KEY));
}

#[tokio::test]
async fn a_500_is_an_error_not_an_empty_result() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let p = provider_against(&server, None).await;
    let err = p.search(&req()).await.unwrap_err();
    // Silently returning zero results here would look identical to "no matches",
    // which is exactly the confusion the diagnostics work is meant to end.
    assert!(err.to_string().contains("500"), "{err}");
}

#[tokio::test]
async fn a_truncated_body_is_a_decode_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"items":[{"id":"abc"#))
        .mount(&server)
        .await;

    let p = provider_against(&server, None).await;
    let err = p.search(&req()).await.unwrap_err();
    assert!(
        matches!(err, readingbuddy::EngineError::Json(_)),
        "expected a json decode error, got {err:?}"
    );
    assert_eq!(
        readingbuddy::ErrorClass::from(&err),
        readingbuddy::ErrorClass::Decode
    );
}

/// A key must be sent when configured and absent when not — the keyless path is
/// a supported mode, not a degraded one.
#[tokio::test]
async fn the_key_is_sent_only_when_configured() {
    for key in [Some(KEY), None] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"items":null}"#))
            .mount(&server)
            .await;

        let p = provider_against(&server, key).await;
        p.search(&req()).await.unwrap();

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let query = requests[0].url.query().unwrap_or_default().to_string();
        assert_eq!(
            query.contains("key="),
            key.is_some(),
            "key presence wrong for {key:?}: {query}"
        );
    }
}

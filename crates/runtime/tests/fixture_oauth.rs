//! Signing in to a service (phase 7 step 03): the sign-in itself, then keeping, refreshing and
//! revoking a grant, against an authorization server and a protected MCP server run in this process.
#![cfg(unix)]

#[path = "support/oauth_fixture.rs"]
mod oauth_fixture;

use std::time::Duration;

use base64::Engine;
use chrono::Utc;
use farik_core::team::OAuthSettings;
use farik_runtime::sign_in::{OAuthGrant, SignInError, start_sign_in};
use oauth_fixture::{Fixture, Iss, Methods, callback, follow};
use sha2::{Digest, Sha256};

fn auto() -> OAuthSettings {
    OAuthSettings {
        client_id: None,
        callback_port: None,
        scopes: Vec::new(),
    }
}

/// A port nothing is listening on, for a pre-registered client's redirect.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("a port")
        .local_addr()
        .expect("its address")
        .port()
}

/// Signs in at `fixture`'s server and follows the page as a browser would.
async fn sign_in_with(
    fixture: &Fixture,
    settings: &OAuthSettings,
) -> Result<OAuthGrant, SignInError> {
    let sign_in = start_sign_in(&fixture.mcp_url, settings, Utc::now()).await?;
    let url = sign_in.authorize_url().to_string();
    let finishing = tokio::spawn(sign_in.finish());
    follow(&url).await;
    finishing.await.expect("the sign-in task")
}

#[tokio::test]
async fn signs_in_with_dynamic_registration() {
    let fixture = Fixture::start().await;
    let sign_in = start_sign_in(&fixture.mcp_url, &auto(), Utc::now())
        .await
        .expect("the sign-in starts");
    let port = sign_in.callback_addr().port();
    assert_eq!(sign_in.issuer(), fixture.origin);
    let url = sign_in.authorize_url().to_string();
    let finishing = tokio::spawn(sign_in.finish());
    follow(&url).await;
    let grant = finishing.await.expect("task").expect("signed in");

    let registered: serde_json::Value =
        serde_json::from_str(&fixture.requests("/register")[0].body).expect("json");
    assert_eq!(registered["application_type"], "native");
    assert_eq!(registered["token_endpoint_auth_method"], "none");
    assert_eq!(
        registered["redirect_uris"],
        serde_json::json!([format!("http://localhost:{port}/callback")])
    );
    assert_eq!(registered["client_name"], "Farik");
    let authorized = &fixture.requests("/authorize")[0];
    assert_eq!(authorized.query["client_id"], grant.client_id);
    assert!(
        grant.client_id.starts_with("client-"),
        "{}",
        grant.client_id
    );
    assert_eq!(grant.issuer, fixture.origin);
    assert_eq!(grant.resource, fixture.mcp_url);
    assert_eq!(grant.token_endpoint, format!("{}/token", fixture.origin));
    assert!(grant.access_token.expose().starts_with("at-"));
    assert!(
        grant
            .refresh_token
            .as_ref()
            .is_some_and(|token| token.expose().starts_with("rt-"))
    );
    assert!(grant.expires_at.is_some());
    assert!(!grant.lapsed);
}

#[tokio::test]
async fn uses_a_preregistered_client_without_registering() {
    let fixture = Fixture::start().await;
    let port = free_port();
    let settings = OAuthSettings {
        client_id: Some("abc".to_string()),
        callback_port: Some(port),
        scopes: Vec::new(),
    };
    let grant = sign_in_with(&fixture, &settings).await.expect("signed in");
    assert_eq!(fixture.count("/register"), 0);
    assert_eq!(grant.client_id, "abc");
    assert_eq!(
        fixture.requests("/authorize")[0].query["redirect_uri"],
        format!("http://localhost:{port}/callback")
    );
    // Without a port, the one Claude Code's own clients registered: 33418.
    let settings = OAuthSettings {
        client_id: Some("abc".to_string()),
        callback_port: None,
        scopes: Vec::new(),
    };
    sign_in_with(&fixture, &settings).await.expect("signed in");
    assert_eq!(
        fixture.requests("/authorize")[1].query["redirect_uri"],
        "http://localhost:33418/callback"
    );
    assert_eq!(fixture.count("/register"), 0);
}

#[tokio::test]
async fn sends_pkce_s256_and_the_resource() {
    let fixture = Fixture::start().await;
    sign_in_with(&fixture, &auto()).await.expect("signed in");
    let authorized = &fixture.requests("/authorize")[0];
    assert_eq!(authorized.query["code_challenge_method"], "S256");
    let token = &fixture.requests("/token")[0];
    let hashed = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(token.form["code_verifier"].as_bytes()));
    assert_eq!(hashed, authorized.query["code_challenge"]);
    assert_eq!(authorized.query["resource"], fixture.mcp_url);
    assert_eq!(token.form["resource"], fixture.mcp_url);
}

#[tokio::test]
async fn refuses_a_server_without_s256() {
    for methods in [Methods::Plain, Methods::Absent] {
        let fixture = Fixture::start().await;
        fixture.set(|flags| {
            flags.methods = methods;
        });
        let refused = start_sign_in(&fixture.mcp_url, &auto(), Utc::now())
            .await
            .expect_err("no S256");
        assert_eq!(refused, SignInError::PkceNotSupported);
        assert_eq!(fixture.count("/register"), 0);
    }
}

#[tokio::test]
async fn refuses_with_neither_registration_nor_client() {
    let fixture = Fixture::start().await;
    fixture.set(|flags| {
        flags.dcr = false;
    });
    let refused = start_sign_in(&fixture.mcp_url, &auto(), Utc::now())
        .await
        .expect_err("nothing to register with");
    assert_eq!(refused, SignInError::NotSupported);
    let settings = OAuthSettings {
        client_id: Some("abc".to_string()),
        callback_port: Some(free_port()),
        scopes: Vec::new(),
    };
    sign_in_with(&fixture, &settings)
        .await
        .expect("a given client needs no registration");
}

#[tokio::test]
async fn says_not_offered_without_resource_metadata() {
    let fixture = Fixture::start().await;
    fixture.set(|flags| {
        flags.challenge = false;
        flags.prm = false;
        flags.as_metadata = false;
    });
    let refused = start_sign_in(&fixture.mcp_url, &auto(), Utc::now())
        .await
        .expect_err("not offered");
    assert_eq!(refused, SignInError::NotOffered);
}

#[tokio::test]
async fn does_not_guess_endpoints() {
    let fixture = Fixture::start().await;
    fixture.set(|flags| {
        flags.challenge = false;
        flags.prm = false;
    });
    let refused = start_sign_in(&fixture.mcp_url, &auto(), Utc::now())
        .await
        .expect_err("not offered");
    assert_eq!(refused, SignInError::NotOffered);
    assert_eq!(fixture.count("/register"), 0);
}

#[tokio::test]
async fn refuses_the_wrong_state() {
    let fixture = Fixture::start().await;
    let sign_in = start_sign_in(&fixture.mcp_url, &auto(), Utc::now())
        .await
        .expect("starts");
    let addr = sign_in.callback_addr();
    let url = sign_in.authorize_url().to_string();
    let finishing = tokio::spawn(sign_in.finish());
    let wrong = callback(&format!(
        "http://localhost:{}/callback?code=x&state=wrong",
        addr.port()
    ))
    .await;
    assert_eq!(wrong.status, 400);
    follow(&url).await;
    finishing
        .await
        .expect("task")
        .expect("the right callback still completes");
    assert_eq!(fixture.count("/token"), 1);
}

#[tokio::test]
async fn refuses_another_issuer_or_a_missing_one_when_promised() {
    let fixture = Fixture::start().await;
    fixture.set(|flags| {
        flags.iss = Iss::Other("https://evil.example".to_string());
    });
    assert_eq!(
        sign_in_with(&fixture, &auto())
            .await
            .expect_err("another issuer"),
        SignInError::Mismatch
    );
    assert_eq!(fixture.count("/token"), 0, "no code was exchanged");

    let fixture = Fixture::start().await;
    fixture.set(|flags| {
        flags.iss = Iss::Absent;
        flags.iss_promised = true;
    });
    assert_eq!(
        sign_in_with(&fixture, &auto())
            .await
            .expect_err("promised and missing"),
        SignInError::Mismatch
    );

    let fixture = Fixture::start().await;
    fixture.set(|flags| {
        flags.iss = Iss::Absent;
    });
    sign_in_with(&fixture, &auto())
        .await
        .expect("no iss and none promised signs in");

    let fixture = Fixture::start().await;
    fixture.set(|flags| {
        flags.access_denied = true;
        flags.iss = Iss::Other("https://evil.example".to_string());
    });
    assert_eq!(
        sign_in_with(&fixture, &auto())
            .await
            .expect_err("another issuer on an error"),
        SignInError::Mismatch,
        "not Denied"
    );
}

#[tokio::test]
async fn reports_access_denied() {
    let fixture = Fixture::start().await;
    fixture.set(|flags| {
        flags.access_denied = true;
    });
    assert_eq!(
        sign_in_with(&fixture, &auto()).await.expect_err("denied"),
        SignInError::Denied("access_denied".to_string())
    );
}

#[tokio::test]
async fn answers_one_callback_then_closes() {
    let fixture = Fixture::start().await;
    let sign_in = start_sign_in(&fixture.mcp_url, &auto(), Utc::now())
        .await
        .expect("starts");
    let addr = sign_in.callback_addr();
    let url = sign_in.authorize_url().to_string();
    let finishing = tokio::spawn(sign_in.finish());
    let other = callback(&format!("http://{addr}/other")).await;
    assert_eq!(other.status, 404);
    follow(&url).await;
    finishing.await.expect("task").expect("still completes");
    assert!(
        tokio::net::TcpStream::connect(addr).await.is_err(),
        "the listener is closed"
    );
}

#[tokio::test]
async fn the_callback_page_quotes_nothing() {
    let fixture = Fixture::start().await;
    fixture.set(|flags| {
        flags.access_denied = true;
        flags.error_description = Some("<b>x</b>".to_string());
    });
    let sign_in = start_sign_in(&fixture.mcp_url, &auto(), Utc::now())
        .await
        .expect("starts");
    let url = sign_in.authorize_url().to_string();
    let finishing = tokio::spawn(sign_in.finish());
    let page = follow(&url).await;
    finishing.await.expect("task").expect_err("denied");
    assert!(
        !page.page.contains("<b>x</b>") && !page.page.contains("x</b>"),
        "{}",
        page.page
    );
    assert!(
        !page.page.contains("<a ") && !page.page.contains("<script"),
        "{}",
        page.page
    );
    for (name, value) in [
        ("content-type", "text/html; charset=utf-8"),
        ("content-security-policy", "default-src 'none'"),
        ("cache-control", "no-store"),
        ("referrer-policy", "no-referrer"),
    ] {
        assert_eq!(
            page.headers
                .get(name)
                .and_then(|header| header.to_str().ok()),
            Some(value),
            "{name}"
        );
    }
}

#[tokio::test]
async fn listens_on_loopback_only() {
    let fixture = Fixture::start().await;
    let sign_in = start_sign_in(&fixture.mcp_url, &auto(), Utc::now())
        .await
        .expect("starts");
    assert!(sign_in.callback_addr().ip().is_loopback());
}

#[tokio::test]
async fn says_when_the_callback_port_is_taken() {
    let fixture = Fixture::start().await;
    let taken = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let port = taken.local_addr().expect("address").port();
    let settings = OAuthSettings {
        client_id: Some("abc".to_string()),
        callback_port: Some(port),
        scopes: Vec::new(),
    };
    let SignInError::Failed(message) = start_sign_in(&fixture.mcp_url, &settings, Utc::now())
        .await
        .expect_err("the port is in use")
    else {
        panic!("Failed");
    };
    assert!(message.contains(&port.to_string()), "{message}");
}

#[tokio::test]
async fn gives_up_after_ten_minutes() {
    let fixture = Fixture::start().await;
    let sign_in = start_sign_in(&fixture.mcp_url, &auto(), Utc::now())
        .await
        .expect("starts");
    tokio::time::pause();
    assert_eq!(
        sign_in.finish().await.expect_err("nobody came"),
        SignInError::TimedOut
    );
}

#[tokio::test]
async fn gives_up_starting_after_fifteen_seconds() {
    let fixture = Fixture::start().await;
    fixture.hold("prm");
    let url = fixture.mcp_url.clone();
    let starting = tokio::spawn(async move { start_sign_in(&url, &auto(), Utc::now()).await });
    // The clock is paused only once the held request is waiting: with it paused earlier, a
    // moment spent on real network I/O would let the clock jump.
    while fixture.count("/.well-known/oauth-protected-resource/mcp") == 0 {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tokio::time::pause();
    let refused = starting
        .await
        .expect("task")
        .expect_err("the metadata never came");
    assert!(matches!(refused, SignInError::Failed(_)), "{refused:?}");
}

#[tokio::test]
async fn refuses_an_endpoint_that_is_not_https() {
    let fixture = Fixture::start().await;
    fixture.set(|flags| {
        flags.authorization_endpoint = Some("http://auth.example/authorize".to_string());
    });
    let SignInError::Failed(message) = start_sign_in(&fixture.mcp_url, &auto(), Utc::now())
        .await
        .expect_err("not https")
    else {
        panic!("Failed");
    };
    assert!(
        message.contains("http://auth.example/authorize"),
        "{message}"
    );
    assert!(message.contains("is not https"), "{message}");

    let fixture = Fixture::start().await;
    fixture.set(|flags| {
        flags.metadata_redirect = Some("http://auth.example/metadata".to_string());
    });
    let refused = start_sign_in(&fixture.mcp_url, &auto(), Utc::now())
        .await
        .expect_err("redirected to http");
    assert!(matches!(refused, SignInError::Failed(_)), "{refused:?}");
    // The server itself is not https either.
    let refused = start_sign_in("http://mcp.example.com/mcp", &auto(), Utc::now())
        .await
        .expect_err("not https");
    assert_eq!(
        refused,
        SignInError::Failed("http://mcp.example.com/mcp is not https".to_string())
    );
}

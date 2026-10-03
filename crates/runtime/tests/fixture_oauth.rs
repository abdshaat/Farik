//! Signing in to a service (phase 7 step 03): the sign-in itself, then keeping, refreshing and
//! revoking a grant, against an authorization server and a protected MCP server run in this process.
#![cfg(unix)]

#[path = "support/oauth_fixture.rs"]
mod oauth_fixture;
#[path = "support/ports.rs"]
mod ports;

use std::collections::BTreeMap;
use std::time::Duration;

use base64::Engine;
use chrono::Utc;
use farik_core::team::{CustomServer, CustomTransport, OAuthSettings};
use farik_runtime::claude::Secret;
use farik_runtime::connectors::list_tools;
use farik_runtime::sign_in::{OAuthGrant, SignInError, refreshed, revoke, start_sign_in};
use oauth_fixture::{Fixture, Iss, Methods, callback, follow};
use ports::free_port;
use sha2::{Digest, Sha256};

fn auto() -> OAuthSettings {
    OAuthSettings {
        client_id: None,
        callback_port: None,
        scopes: Vec::new(),
    }
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
    let page = follow(&url).await;
    let grant = finishing.await.expect("task").expect("signed in");
    assert!(
        page.page.contains("<h1>You&#39;re signed in to "),
        "{}",
        page.page
    );
    assert!(
        page.page
            .contains("<p>You can close this tab and go back to Farik.</p>")
    );

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

    let fixture = Fixture::start().await;
    fixture.set(|flags| {
        flags.access_denied = true;
        flags.iss = Iss::Absent;
        flags.iss_promised = true;
    });
    assert_eq!(
        sign_in_with(&fixture, &auto())
            .await
            .expect_err("no issuer on an error, though promised"),
        SignInError::Mismatch,
        "not Denied"
    );
}

#[tokio::test]
async fn refuses_metadata_without_an_issuer() {
    let fixture = Fixture::start().await;
    fixture.set(|flags| {
        flags.no_issuer = true;
    });
    let refused = start_sign_in(&fixture.mcp_url, &auto(), Utc::now())
        .await
        .expect_err("no issuer");
    assert!(matches!(refused, SignInError::Failed(_)), "{refused:?}");
    assert_eq!(fixture.count("/register"), 0);
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
    // Only a GET is the callback: a POST with the right state is not.
    let state = reqwest::Url::parse(&url)
        .expect("the address")
        .query_pairs()
        .find(|(name, _)| name == "state")
        .map(|(_, value)| value.into_owned())
        .expect("a state");
    let posted = reqwest::Client::new()
        .post(format!("http://{addr}/callback?state={state}&code=x"))
        .send()
        .await
        .expect("answered");
    assert_eq!(posted.status().as_u16(), 404);
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
    assert!(page.page.contains("<img alt=\"Farik\""), "{}", page.page);
    assert!(
        page.page
            .contains("<h1>Farik couldn&#39;t finish signing in:"),
        "{}",
        page.page
    );
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
        (
            "content-security-policy",
            "default-src 'none'; img-src data:",
        ),
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
    for _ in 0..300 {
        if fixture.count("/.well-known/oauth-protected-resource/mcp") > 0 || starting.is_finished()
        {
            break;
        }
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
    // A registration that redirects to plain http is refused at that hop, and says so.
    let fixture = Fixture::start().await;
    fixture.set(|flags| {
        flags.register_redirect = Some("http://auth.example/register".to_string());
    });
    let refused = start_sign_in(&fixture.mcp_url, &auto(), Utc::now())
        .await
        .expect_err("registration redirected to http");
    let SignInError::Failed(message) = refused else {
        panic!("{refused:?}")
    };
    assert!(
        message.contains("http://auth.example/register is not https"),
        "{message}"
    );
    // The server itself is not https either.
    let refused = start_sign_in("http://mcp.example.com/mcp", &auto(), Utc::now())
        .await
        .expect_err("not https");
    assert_eq!(
        refused,
        SignInError::Failed("http://mcp.example.com/mcp is not https".to_string())
    );
}

/// A grant the fixture honours, which expires `expires_in` from `now`.
fn kept(
    fixture: &Fixture,
    now: chrono::DateTime<Utc>,
    expires_in: Option<chrono::Duration>,
) -> OAuthGrant {
    let (access, refresh) = fixture.mint();
    OAuthGrant {
        issuer: fixture.origin.clone(),
        resource: fixture.mcp_url.clone(),
        client_id: "client-kept".to_string(),
        token_endpoint: format!("{}/token", fixture.origin),
        revocation_endpoint: Some(format!("{}/revoke", fixture.origin)),
        access_token: Secret::new(access),
        refresh_token: Some(Secret::new(refresh)),
        issued_at: now,
        expires_at: expires_in.map(|span| now + span),
        scopes: Vec::new(),
        lapsed: false,
    }
}

const MINUTE: chrono::Duration = chrono::Duration::minutes(1);

#[tokio::test]
async fn lists_tools_with_the_signed_in_token() {
    let fixture = Fixture::start().await;
    let grant = kept(&fixture, Utc::now(), Some(MINUTE * 60));
    let server = CustomServer {
        name: "fixture".to_string(),
        transport: CustomTransport::Http {
            url: fixture.mcp_url.clone(),
            headers: BTreeMap::new(),
            oauth: Some(auto()),
        },
        credential_keys: Vec::new(),
        tools: BTreeMap::new(),
        kit: false,
        allowances: BTreeMap::new(),
    };
    let folder = std::env::temp_dir();
    let tools = list_tools(
        &server,
        &BTreeMap::new(),
        Some(&grant.access_token),
        &folder,
        std::path::Path::new("farik"),
    )
    .await
    .expect("listed with the token");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "whoami");
    assert_eq!(
        tools[0].description,
        format!("Bearer {}", grant.access_token.expose())
    );
    list_tools(
        &server,
        &BTreeMap::new(),
        None,
        &folder,
        std::path::Path::new("farik"),
    )
    .await
    .expect_err("without the token the server answers 401");
}

#[tokio::test]
async fn refreshes_a_token_about_to_expire() {
    let fixture = Fixture::start().await;
    let now = Utc::now();
    let grant = kept(&fixture, now, Some(MINUTE * 4));
    let fresh = refreshed(&grant, now, Duration::from_mins(35), Duration::from_secs(3))
        .await
        .expect("refreshed")
        .expect("it was due");
    assert_ne!(fresh.access_token.expose(), grant.access_token.expose());
    assert_ne!(
        fresh.refresh_token.as_ref().map(Secret::expose),
        grant.refresh_token.as_ref().map(Secret::expose),
        "the refresh token rotated"
    );
    assert!(fresh.access_token.expose().starts_with("at-"));
    assert!(fresh.expires_at.expect("expiry") > now + MINUTE * 35);
    assert_eq!(fresh.issued_at, now);
    assert_eq!(fresh.client_id, grant.client_id);
    assert!(!fresh.lapsed);
    let sent = &fixture.requests("/token")[0].form;
    assert_eq!(sent["grant_type"], "refresh_token");
    assert_eq!(
        sent["refresh_token"],
        grant.refresh_token.as_ref().expect("one").expose()
    );
}

#[tokio::test]
async fn leaves_a_fresh_token_alone() {
    let fixture = Fixture::start().await;
    let now = Utc::now();
    let grant = kept(&fixture, now, Some(MINUTE * 120));
    let outcome = refreshed(&grant, now, Duration::from_mins(35), Duration::from_secs(3)).await;
    assert_eq!(outcome, Ok(None));
    assert_eq!(fixture.count("/token"), 0);
    // With no expiry known, a token is trusted for fifty minutes.
    let unknown = kept(&fixture, now, None);
    let soon = refreshed(
        &unknown,
        now + MINUTE * 49,
        Duration::from_secs(60),
        Duration::from_secs(3),
    )
    .await;
    assert_eq!(soon, Ok(None));
    let later = refreshed(
        &unknown,
        now + MINUTE * 51,
        Duration::from_secs(60),
        Duration::from_secs(3),
    )
    .await;
    assert!(matches!(later, Ok(Some(_))), "{later:?}");
}

#[tokio::test]
async fn a_refused_refresh_lapses() {
    let now = Utc::now();
    for (status, error) in [
        (400, "invalid_grant"),
        (401, "invalid_client"),
        (400, "unauthorized_client"),
    ] {
        let fixture = Fixture::start().await;
        let grant = kept(&fixture, now, Some(MINUTE));
        fixture.set(|flags| flags.refresh_error = Some((status, error.to_string())));
        let outcome = refreshed(
            &grant,
            now,
            Duration::from_secs(600),
            Duration::from_secs(3),
        )
        .await;
        assert_eq!(outcome, Err(SignInError::Lapsed), "{error}");
    }
    let fixture = Fixture::start().await;
    let grant = kept(&fixture, now, Some(MINUTE));
    fixture.set(|flags| flags.refresh_error = Some((500, "server_error".to_string())));
    let outcome = refreshed(
        &grant,
        now,
        Duration::from_secs(600),
        Duration::from_secs(3),
    )
    .await;
    assert!(
        matches!(outcome, Err(SignInError::Failed(_))),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn a_refresh_that_does_not_finish_in_time_fails() {
    let fixture = Fixture::start().await;
    let now = Utc::now();
    let grant = kept(&fixture, now, Some(MINUTE));
    fixture.hold("token");
    let asked = tokio::spawn(async move {
        refreshed(
            &grant,
            now,
            Duration::from_secs(600),
            Duration::from_secs(3),
        )
        .await
    });
    for _ in 0..300 {
        if fixture.count("/token") > 0 || asked.is_finished() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        fixture.count("/token") > 0,
        "the refresh reached the server"
    );
    tokio::time::pause();
    let outcome = asked.await.expect("task");
    assert!(
        matches!(outcome, Err(SignInError::Failed(_))),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn a_grant_without_a_refresh_token_lapses_once_expired() {
    let fixture = Fixture::start().await;
    let now = Utc::now();
    let mut valid = kept(&fixture, now, Some(MINUTE * 2));
    valid.refresh_token = None;
    assert_eq!(
        refreshed(
            &valid,
            now,
            Duration::from_secs(600),
            Duration::from_secs(3)
        )
        .await,
        Ok(None),
        "still valid"
    );
    let mut expired = kept(&fixture, now, Some(-MINUTE));
    expired.refresh_token = None;
    assert_eq!(
        refreshed(
            &expired,
            now,
            Duration::from_secs(600),
            Duration::from_secs(3)
        )
        .await,
        Err(SignInError::Lapsed)
    );
    assert_eq!(fixture.count("/token"), 0);
}

#[tokio::test]
async fn refresh_sends_the_kept_resource() {
    let fixture = Fixture::start().await;
    let now = Utc::now();
    let grant = kept(&fixture, now, Some(MINUTE));
    fixture.set(|flags| flags.keep_refresh_token = true);
    let fresh = refreshed(
        &grant,
        now,
        Duration::from_secs(600),
        Duration::from_secs(3),
    )
    .await
    .expect("refreshed")
    .expect("due");
    let sent = &fixture.requests("/token")[0].form;
    assert_eq!(sent["resource"], grant.resource);
    assert_eq!(sent["client_id"], grant.client_id);
    assert_eq!(
        fresh.refresh_token.as_ref().map(Secret::expose),
        grant.refresh_token.as_ref().map(Secret::expose),
        "an answer without a refresh token keeps the old one"
    );
}

#[tokio::test]
async fn revokes_the_refresh_token() {
    let fixture = Fixture::start().await;
    let now = Utc::now();
    let grant = kept(&fixture, now, Some(MINUTE * 60));
    revoke(&grant).await;
    let sent = &fixture.requests("/revoke")[0].form;
    assert_eq!(
        sent["token"],
        grant.refresh_token.as_ref().expect("one").expose()
    );
    assert_eq!(sent["token_type_hint"], "refresh_token");
    assert_eq!(sent["client_id"], grant.client_id);

    let mut access_only = kept(&fixture, now, Some(MINUTE * 60));
    access_only.refresh_token = None;
    revoke(&access_only).await;
    let sent = &fixture.requests("/revoke")[1].form;
    assert_eq!(sent["token"], access_only.access_token.expose());
    assert_eq!(sent["token_type_hint"], "access_token");

    fixture.set(|flags| flags.revoke_status = 500);
    revoke(&grant).await;
    assert_eq!(fixture.count("/revoke"), 3, "a 500 is not an error");
}

#[tokio::test]
async fn a_kept_grant_is_only_sent_to_https_endpoints() {
    let fixture = Fixture::start().await;
    let now = Utc::now();
    let mut grant = kept(&fixture, now, Some(MINUTE));
    grant.token_endpoint = "http://auth.example/token".to_string();
    grant.revocation_endpoint = Some("http://auth.example/revoke".to_string());
    let outcome = refreshed(
        &grant,
        now,
        Duration::from_secs(600),
        Duration::from_secs(3),
    )
    .await;
    assert_eq!(
        outcome,
        Err(SignInError::Failed(
            "http://auth.example/token is not https".to_string()
        ))
    );
    revoke(&grant).await;
    assert_eq!(fixture.count("/token") + fixture.count("/revoke"), 0);
}

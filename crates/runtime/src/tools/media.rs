//! The pictures and clips of a post (ADR 0042, and the founder's decision of 2026-10-06: a post's
//! pictures are the ones the agent made, the owner's own, or any public address of theirs). A post
//! names up to four `https` addresses, and Farik checks each one twice, when the post is written and
//! again when it is handed to Buffer: that it is a public address, never a loopback or a private
//! one, and that it answers with an image or a video of a size Buffer will take. Farik asks the
//! address for its headers only, and never follows a redirect.
//!
//! Where an address leads is the agent's to choose, so the check stands between an agent and
//! whatever Farik's own computer can reach: an address that is an IP literal is judged by the
//! literal, and a name by every address it resolves to, at the moment the connection is made.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use reqwest::Url;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};

use super::ToolError;
use super::refusal::Refusal;

/// The most characters of an address.
const MOST_URL_CHARACTERS: usize = 2_000;
/// How long an address has to answer.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(30);
/// The most bytes an image may be.
const MOST_IMAGE_BYTES: u64 = 20 * 1024 * 1024;
/// The most bytes a clip may be.
const MOST_VIDEO_BYTES: u64 = 1024 * 1024 * 1024;

/// What the agent says an address holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MediaKind {
    /// A picture.
    Image,
    /// A clip.
    Video,
}

#[cfg(test)]
thread_local! {
    static LOOPBACK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// While it lives, the test's own thread may name an `http` address on its own computer, where a
/// test serves its pictures. Nothing else changes, and nothing outside a test can ask for it.
#[cfg(test)]
pub(crate) struct LoopbackAllowed;

#[cfg(test)]
impl LoopbackAllowed {
    pub(crate) fn new() -> Self {
        LOOPBACK.with(|allowed| allowed.set(true));
        Self
    }
}

#[cfg(test)]
impl Drop for LoopbackAllowed {
    fn drop(&mut self) {
        LOOPBACK.with(|allowed| allowed.set(false));
    }
}

/// Whether this thread may reach its own computer: never outside a test.
fn loopback_allowed() -> bool {
    #[cfg(test)]
    {
        LOOPBACK.with(std::cell::Cell::get)
    }
    #[cfg(not(test))]
    {
        false
    }
}

fn refused(detail: impl Into<String>) -> ToolError {
    Refusal::MarketingPlan {
        code: "media_url_refused",
        detail: detail.into(),
    }
    .into()
}

/// Whether `address` is one anyone on the internet could reach: not this computer, not a private
/// network, not a link-local, shared, documentation, benchmarking, multicast or reserved range,
/// and, for an IPv6 address that carries an IPv4 one, not a private IPv4.
pub(crate) fn is_public(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_public_v4(address),
        IpAddr::V6(address) => {
            if let Some(inside) = address.to_ipv4() {
                return is_public_v4(inside);
            }
            let segments = address.segments();
            if segments[0] == 0x2002 {
                // 6to4: the IPv4 address sits in the next two groups.
                let [high, low] = [segments[1].to_be_bytes(), segments[2].to_be_bytes()];
                return is_public_v4(Ipv4Addr::new(high[0], high[1], low[0], low[1]));
            }
            if segments[..6] == [0x0064, 0xff9b, 0, 0, 0, 0] {
                // NAT64: the IPv4 address is the last two groups.
                let [high, low] = [segments[6].to_be_bytes(), segments[7].to_be_bytes()];
                return is_public_v4(Ipv4Addr::new(high[0], high[1], low[0], low[1]));
            }
            !(address.is_unspecified()
                || address.is_loopback()
                || address.is_multicast()
                || is_unique_local(address)
                || (segments[0] & 0xffc0) == 0xfe80
                || (segments[0] == 0x2001 && segments[1] == 0x0db8))
        }
    }
}

fn is_unique_local(address: Ipv6Addr) -> bool {
    (address.segments()[0] & 0xfe00) == 0xfc00
}

fn is_public_v4(address: Ipv4Addr) -> bool {
    let [first, second, third, _] = address.octets();
    !(address.is_unspecified()
        || address.is_loopback()
        || address.is_private()
        || address.is_link_local()
        || address.is_broadcast()
        || address.is_documentation()
        || address.is_multicast()
        || first == 0
        // Shared address space (carrier-grade NAT), 100.64.0.0/10.
        || (first == 100 && (64..128).contains(&second))
        // IETF protocol assignments, 192.0.0.0/24.
        || (first == 192 && second == 0 && third == 0)
        // Benchmarking, 198.18.0.0/15.
        || (first == 198 && (second == 18 || second == 19))
        // Reserved, 240.0.0.0/4.
        || first >= 240)
}

/// The addresses of `found` a connection may be made to: the public ones, and, in a test that
/// serves its pictures from its own computer, loopback.
fn keep_public(found: Vec<SocketAddr>, allow_loopback: bool) -> Vec<SocketAddr> {
    found
        .into_iter()
        .filter(|address| is_public(address.ip()) || (allow_loopback && address.ip().is_loopback()))
        .collect()
}

/// Resolves a name as the system does, and keeps only the addresses a post's picture may be
/// fetched from, so that a name which resolves to a private address cannot be connected to.
struct PublicOnly {
    allow_loopback: bool,
}

impl Resolve for PublicOnly {
    fn resolve(&self, name: Name) -> Resolving {
        let allow_loopback = self.allow_loopback;
        Box::pin(async move {
            let found: Vec<SocketAddr> =
                tokio::net::lookup_host((name.as_str(), 0)).await?.collect();
            let kept = keep_public(found, allow_loopback);
            if kept.is_empty() {
                return Err("it resolves to no public address".into());
            }
            Ok(Box::new(kept.into_iter()) as Addrs)
        })
    }
}

/// `address` as a URL a post's picture may be at: at most 2,000 characters, `https` on its own
/// port with no user and no password, and a public host: an IP literal that is public, or a name
/// of at least two labels that does not end in `.local`, `.localhost`, `.internal`, `.lan` or
/// `.home.arpa`. A name is judged again by what it resolves to when it is connected to.
///
/// # Errors
///
/// `media_url_refused`, saying which rule the address breaks.
pub(crate) fn media_url_allowed(address: &str) -> Result<Url, ToolError> {
    if address.chars().count() > MOST_URL_CHARACTERS {
        return Err(refused(format!(
            "an address is at most {MOST_URL_CHARACTERS} characters"
        )));
    }
    let url =
        Url::parse(address).map_err(|_| refused(format!("{address:?} is not a web address")))?;
    let on_this_computer = loopback_allowed();
    let plain = url.scheme() == "http";
    match url.scheme() {
        "https" => {}
        "http" if on_this_computer => {}
        _ => {
            return Err(refused(format!(
                "{address:?} is not an https address; a post's pictures are at https addresses"
            )));
        }
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(refused(
            "an address holds no user name or password".to_string(),
        ));
    }
    if !on_this_computer && url.port().is_some_and(|port| port != 443) {
        return Err(refused(
            "an address is on the usual https port, 443".to_string(),
        ));
    }
    let Some(host) = url.host_str() else {
        return Err(refused(format!("{address:?} names no host")));
    };
    // An IPv6 literal is written in brackets; the URL reader has already turned every other way
    // of writing an IPv4 address (a decimal number, hexadecimal groups) into the dotted one.
    let literal = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<IpAddr>()
        .ok();
    // A test's plain `http` is for its own computer alone.
    if plain && !literal.is_some_and(|literal| literal.is_loopback()) {
        return Err(refused(format!(
            "{address:?} is not an https address; a post's pictures are at https addresses"
        )));
    }
    if let Some(literal) = literal {
        check_address(literal, on_this_computer)?;
    } else {
        let host = host.to_ascii_lowercase();
        let host = host.trim_end_matches('.');
        let private = ["local", "localhost", "internal", "lan"]
            .iter()
            .any(|suffix| host.ends_with(&format!(".{suffix}")))
            || host.ends_with(".home.arpa");
        if !host.contains('.') || private || host == "localhost" {
            return Err(refused(format!(
                "{host} is not a public host; a post's pictures are at public addresses"
            )));
        }
    }
    Ok(url)
}

fn check_address(address: IpAddr, on_this_computer: bool) -> Result<(), ToolError> {
    if is_public(address) || (on_this_computer && address.is_loopback()) {
        Ok(())
    } else {
        Err(refused(format!(
            "{address} is not a public address; a post's pictures are at public addresses"
        )))
    }
}

/// What the address answered, headers only: the status, the content type without its parameters,
/// and the length when it said.
struct Answer {
    status: reqwest::StatusCode,
    content_type: String,
    length: Option<u64>,
}

/// Asks `url` for its headers, with the client every check of a picture uses: no redirect, no proxy,
/// no cookie, thirty seconds, and a connection only to a public address.
async fn ask(url: &Url) -> Result<Answer, ToolError> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .timeout(ANSWER_TIMEOUT)
        .dns_resolver(Arc::new(PublicOnly {
            allow_loopback: loopback_allowed(),
        }))
        .build()
        .map_err(|_| ToolError::Failed {
            detail: "the web client could not be made".to_string(),
        })?;
    let host = url.host_str().unwrap_or_default().to_string();
    let response = client
        .get(url.clone())
        .send()
        .await
        .map_err(|_| refused(format!("{host} did not answer, or is not a public address")))?;
    Ok(Answer {
        status: response.status(),
        content_type: response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase(),
        length: response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok()),
    })
}

/// Whether the address `address` answers with a picture or a clip of the `kind` the agent said:
/// the rules of [`media_url_allowed`], then a request for its headers, which must be a success
/// with an image (PNG, JPEG, GIF or WebP, never SVG) or a video, of no more than 20 MiB for a
/// picture and a gigabyte for a clip when it says how big it is.
///
/// # Errors
///
/// `media_url_refused`, saying what is wrong with the address or what it answered.
pub(crate) async fn media_answers(address: &str, kind: MediaKind) -> Result<(), ToolError> {
    let url = media_url_allowed(address)?;
    let host = url.host_str().unwrap_or_default().to_string();
    let answer = ask(&url).await?;
    if !answer.status.is_success() {
        return Err(refused(format!("{host} did not give the file")));
    }
    let (fits, most) = match kind {
        MediaKind::Image => (
            matches!(
                answer.content_type.as_str(),
                "image/png" | "image/jpeg" | "image/gif" | "image/webp"
            ),
            MOST_IMAGE_BYTES,
        ),
        MediaKind::Video => (answer.content_type.starts_with("video/"), MOST_VIDEO_BYTES),
    };
    if !fits {
        return Err(refused(format!(
            "{host} answers {:?}, which is not {}",
            answer.content_type,
            match kind {
                MediaKind::Image => "a PNG, JPEG, GIF or WebP picture",
                MediaKind::Video => "a video",
            }
        )));
    }
    if answer.length.is_some_and(|length| length > most) {
        return Err(refused(format!(
            "the file at {host} is over {} MiB",
            most / (1024 * 1024)
        )));
    }
    Ok(())
}

/// A server of pictures for the tests of every module that checks a post's.
#[cfg(test)]
pub(crate) mod fixtures {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::extract::{Request, State};
    use axum::http::{StatusCode, header};
    use axum::middleware::Next;
    use axum::routing::get;

    /// A server of pictures on this computer: its address, and how many requests it has had.
    /// It answers `/ok.png`, `/ok.jpg`, `/clip.mp4` and, as pictures that are not, `/page`,
    /// `/pic.svg`, `/gone.png`, `/moved.png` and `/huge.png`.
    pub(crate) async fn serving() -> (String, Arc<AtomicUsize>) {
        let big = vec![0_u8; 21 * 1024 * 1024];
        let asked = Arc::new(AtomicUsize::new(0));
        let router = axum::Router::new()
            .route(
                "/ok.png",
                get(|| async {
                    (
                        [(header::CONTENT_TYPE, "image/png")],
                        vec![0x89, b'P', b'N', b'G'],
                    )
                }),
            )
            .route(
                "/ok.jpg",
                get(|| async {
                    (
                        [(header::CONTENT_TYPE, "image/jpeg; charset=binary")],
                        vec![0xff, 0xd8],
                    )
                }),
            )
            .route(
                "/clip.mp4",
                get(|| async { ([(header::CONTENT_TYPE, "video/mp4")], vec![0_u8; 16]) }),
            )
            .route(
                "/page",
                get(|| async { ([(header::CONTENT_TYPE, "text/html")], "<html></html>") }),
            )
            .route(
                "/pic.svg",
                get(|| async { ([(header::CONTENT_TYPE, "image/svg+xml")], "<svg/>") }),
            )
            .route("/gone.png", get(|| async { StatusCode::NOT_FOUND }))
            .route(
                "/moved.png",
                get(|| async { (StatusCode::FOUND, [(header::LOCATION, "/ok.png")]) }),
            )
            .route(
                "/huge.png",
                get(move || {
                    let big = big.clone();
                    async move { ([(header::CONTENT_TYPE, "image/png")], big) }
                }),
            )
            .layer(axum::middleware::from_fn_with_state(
                Arc::clone(&asked),
                |State(asked): State<Arc<AtomicUsize>>, request: Request, next: Next| async move {
                    asked.fetch_add(1, Ordering::SeqCst);
                    next.run(request).await
                },
            ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a local port");
        let origin = format!("http://{}", listener.local_addr().expect("its address"));
        tokio::spawn(async move { axum::serve(listener, router).await });
        (origin, asked)
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use super::fixtures::serving;
    use super::{LoopbackAllowed, MediaKind, ask, keep_public, media_answers, media_url_allowed};
    use crate::tools::ToolError;

    fn reason_of(error: ToolError) -> String {
        match error {
            ToolError::Refused { reason } => reason,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn refuses_an_address_that_is_not_a_public_https_one() {
        for refused in [
            "not an address",
            "http://cdn.example.com/a.png",
            "ftp://cdn.example.com/a.png",
            "https://user:secret@cdn.example.com/a.png",
            "https://cdn.example.com:8443/a.png",
            "https://127.0.0.1/a.png",
            "https://127.1/a.png",
            "https://2130706433/a.png",
            "https://0x7f.0.0.1/a.png",
            "https://0.0.0.0/a.png",
            "https://10.0.0.5/a.png",
            "https://172.16.4.4/a.png",
            "https://192.168.1.1/a.png",
            "https://169.254.169.254/latest/meta-data/",
            "https://100.64.0.1/a.png",
            "https://198.18.0.1/a.png",
            "https://240.0.0.1/a.png",
            "https://[::1]/a.png",
            "https://[::]/a.png",
            "https://[fd00::1]/a.png",
            "https://[fe80::1]/a.png",
            "https://[::ffff:10.0.0.1]/a.png",
            "https://[::ffff:127.0.0.1]/a.png",
            "https://[2002:0a00:0001::1]/a.png",
            "https://[64:ff9b::a00:1]/a.png",
            "https://localhost/a.png",
            "https://LOCALHOST./a.png",
            "https://app.localhost/a.png",
            "https://printer.local/a.png",
            "https://nas.internal/a.png",
            "https://router.lan/a.png",
            "https://intranet/a.png",
        ] {
            let error = media_url_allowed(refused).expect_err(refused);
            assert!(
                reason_of(error).starts_with("media_url_refused: "),
                "{refused}"
            );
        }
        let long = format!("https://cdn.example.com/{}", "a".repeat(2_000));
        assert!(media_url_allowed(&long).is_err(), "over 2,000 characters");

        for allowed in [
            "https://cdn.example.com/a.png",
            "https://cdn.example.com:443/a.png?x=1&y=2",
            "https://93.184.216.34/a.png",
            "https://[2606:4700::1111]/a.png",
        ] {
            media_url_allowed(allowed).expect(allowed);
        }
    }

    #[test]
    fn a_test_may_use_plain_http_for_its_own_computer_alone() {
        let _here = LoopbackAllowed::new();
        media_url_allowed("http://127.0.0.1:8080/a.png").expect("this computer");
        media_url_allowed("https://127.0.0.1:8080/a.png").expect("this computer, over https");
        for refused in [
            "http://cdn.example.com/a.png",
            "http://93.184.216.34/a.png",
            "http://10.0.0.5/a.png",
            "http://localhost/a.png",
            "https://10.0.0.5/a.png",
            "https://localhost/a.png",
        ] {
            assert!(media_url_allowed(refused).is_err(), "{refused}");
        }
    }

    #[test]
    fn keeps_only_the_public_addresses_a_name_resolves_to() {
        let at = |text: &str| -> SocketAddr { format!("{text}:443").parse().expect("an address") };
        let found = vec![at("10.0.0.5"), at("93.184.216.34"), at("127.0.0.1")];
        assert_eq!(keep_public(found.clone(), false), [at("93.184.216.34")]);
        assert_eq!(
            keep_public(found, true),
            [at("93.184.216.34"), at("127.0.0.1")],
            "a test may reach its own computer, and no other private address"
        );
        assert!(keep_public(vec![at("192.168.0.9")], false).is_empty());
    }

    #[tokio::test]
    async fn accepts_what_answers_with_a_picture_or_a_clip() {
        let _here = LoopbackAllowed::new();
        let (origin, _) = serving().await;
        for path in ["ok.png", "ok.jpg"] {
            media_answers(&format!("{origin}/{path}"), MediaKind::Image)
                .await
                .expect(path);
        }
        media_answers(&format!("{origin}/clip.mp4"), MediaKind::Video)
            .await
            .expect("a clip");
    }

    #[tokio::test]
    async fn refuses_what_does_not_answer_with_the_kind_it_is_said_to_be() {
        let _here = LoopbackAllowed::new();
        let (origin, _) = serving().await;
        let asked = |path: &'static str, kind| {
            let address = format!("{origin}/{path}");
            async move { media_answers(&address, kind).await.map_err(reason_of) }
        };
        for (path, kind, said) in [
            ("page", MediaKind::Image, "text/html"),
            ("pic.svg", MediaKind::Image, "svg"),
            ("clip.mp4", MediaKind::Image, "video/mp4"),
            ("ok.png", MediaKind::Video, "image/png"),
            ("gone.png", MediaKind::Image, "did not give the file"),
            ("moved.png", MediaKind::Image, "did not give the file"),
            ("huge.png", MediaKind::Image, "over 20 MiB"),
        ] {
            let reason = asked(path, kind).await.expect_err(path);
            assert!(
                reason.starts_with("media_url_refused: "),
                "{path}: {reason}"
            );
            assert!(reason.contains(said), "{path}: {reason}");
        }
    }

    #[tokio::test]
    async fn will_not_connect_to_a_name_that_resolves_to_this_computer() {
        // `localhost` is a name that resolves to loopback: asked for with no allowance, the
        // resolver keeps no address and the request fails; with it, it is answered.
        let (origin, _) = {
            let _here = LoopbackAllowed::new();
            serving().await
        };
        let port = origin.rsplit(':').next().expect("a port");
        let url = reqwest::Url::parse(&format!("http://localhost:{port}/ok.png")).expect("a url");
        assert!(ask(&url).await.is_err(), "no public address, no connection");
        let _here = LoopbackAllowed::new();
        let answer = ask(&url)
            .await
            .expect("answered when this computer is allowed");
        assert!(answer.status.is_success());
    }
}

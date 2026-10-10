//! The apps Catervas has registered with a service (ADR 0035, route 2): a public client Catervas owns,
//! used only for the servers whose address the table names, so that a token from Catervas's app is
//! never sent to a host the app does not serve.

use std::fmt;

use url::{Host, Url};

/// How a registered app signs a user in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppFlow {
    /// The device flow (RFC 8628): Catervas asks the service for a code, the user types it on the
    /// service's page, and Catervas polls until the service says yes.
    Device {
        /// Where the device code is asked for.
        device_endpoint: &'static str,
        /// The one page the user types the code on; an answer naming another is refused.
        verification_uri: &'static str,
    },
    /// The authorization-code flow with PKCE, back to a listener on this computer, for an app
    /// that signs in for one of Catervas's own connectors: Catervas makes the requests itself, since
    /// there is no MCP server whose metadata to discover.
    Loopback {
        /// The page the user is sent to, which asks them to say yes.
        authorization_endpoint: &'static str,
    },
}

/// One app Catervas has registered with a service.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RegisteredApp {
    /// What a kept grant names it by.
    pub id: &'static str,
    /// What the screens call it.
    pub name: &'static str,
    /// The one host it serves, by the server's address and never by what the server's metadata
    /// says; none for an app that signs in for one of Catervas's own connectors, whose server is no
    /// address.
    pub host: Option<&'static str>,
    /// The one of Catervas's own connectors (`catervas connector <name>`) it signs in for, if it does.
    pub catervas_connector: Option<&'static str>,
    /// How a user signs in to it.
    pub flow: AppFlow,
    /// The app's client id, which is public.
    pub client_id: &'static str,
    /// The client secret a service insists on for a client it calls public, sent on the exchange
    /// and on each refresh and never kept in a grant (ADR 0035). It protects nothing.
    pub client_secret: Option<&'static str>,
    /// The only scopes the app asks for.
    pub scopes: &'static [&'static str],
    /// Who the service says it is.
    pub issuer: &'static str,
    /// Where tokens are asked for and refreshed.
    pub token_endpoint: &'static str,
    /// Where a grant is asked to be forgotten, when the app can do that without a secret.
    pub revocation_endpoint: Option<&'static str>,
    /// Where the user installs the app on what the agent is to read, when it must be.
    pub install_url: Option<&'static str>,
    /// The page where the user removes the app at the service.
    pub settings_url: &'static str,
}

impl fmt::Debug for RegisteredApp {
    /// Everything but the client secret, which no assertion message or log may print.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RegisteredApp")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("host", &self.host)
            .field("catervas_connector", &self.catervas_connector)
            .field("flow", &self.flow)
            .field("client_id", &self.client_id)
            .field("client_secret", &self.client_secret.map(|_| "***"))
            .field("scopes", &self.scopes)
            .field("issuer", &self.issuer)
            .field("token_endpoint", &self.token_endpoint)
            .field("revocation_endpoint", &self.revocation_endpoint)
            .field("install_url", &self.install_url)
            .field("settings_url", &self.settings_url)
            .finish()
    }
}

/// Catervas's Google app, which signs in for Catervas's `google-ads` connector alone (ADR 0042) with
/// the one scope it needs, `id` and `secret` being its client id and client secret. No build holds
/// either (ADR 0044): the tests make an entry from ids of their own, and phase 8's sign-in
/// through Catervas Cloud, which holds Catervas's, builds on these facts.
#[cfg(test)]
const fn google(id: &'static str, secret: &'static str) -> RegisteredApp {
    RegisteredApp {
        id: "google",
        name: "Google",
        host: None,
        catervas_connector: Some("google-ads"),
        flow: AppFlow::Loopback {
            authorization_endpoint: "https://accounts.google.com/o/oauth2/v2/auth",
        },
        client_id: id,
        client_secret: Some(secret),
        scopes: &["https://www.googleapis.com/auth/adwords"],
        issuer: "https://accounts.google.com",
        token_endpoint: "https://oauth2.googleapis.com/token",
        // Google's revocation ends every grant of the project for that account, every agent's at
        // once, so Remove deletes the local grant only and points to Google's settings.
        revocation_endpoint: None,
        install_url: None,
        settings_url: "https://myaccount.google.com/connections",
    }
}

/// Every app Catervas has registered: none. No build carries an app of Catervas's, and from phase 8
/// Catervas Cloud serves Catervas's apps (ADR 0044); the tests pass tables of their own.
pub static REGISTERED_APPS: &[RegisteredApp] = &[];

/// The app of `apps` that serves the server at `url`, if one does.
#[must_use]
pub fn app_for<'a>(apps: &'a [RegisteredApp], url: &str) -> Option<&'a RegisteredApp> {
    let url = Url::parse(url).ok()?;
    if !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    let served = match url.scheme() {
        "https" => url.port_or_known_default() == Some(443),
        "http" => is_loopback(&url),
        _ => false,
    };
    let host = url.host_str().filter(|_| served)?;
    apps.iter().find(|app| {
        app.host
            .is_some_and(|served| served.eq_ignore_ascii_case(host))
    })
}

/// The app of `apps` that signs in for the connector `command` and `args` start, when they are,
/// exactly, `catervas connector <name>` for one of Catervas's own connectors and the app's is `<name>`.
#[must_use]
pub fn app_for_catervas_connector<'a>(
    apps: &'a [RegisteredApp],
    command: &str,
    args: &[String],
) -> Option<&'a RegisteredApp> {
    if !catervas_roles::is_catervas_connector(command, args) {
        return None;
    }
    let connector = args.get(1)?;
    apps.iter()
        .find(|app| app.catervas_connector == Some(connector.as_str()))
}

/// Whether `url` names this computer, which is the only place Catervas talks to over `http`.
fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

/// What the tests of other modules need: a table whose one entry signs in for Catervas's connector
/// `osv` at the OAuth fixture.
#[cfg(all(test, unix))]
pub(crate) mod fixtures {
    use super::{AppFlow, RegisteredApp};
    use crate::oauth_fixture::Fixture;

    /// The one scope the entry asks for.
    pub(crate) const SCOPE: &str = "https://example.test/auth/ads";
    /// The client secret the entry has, which a fixture that sets `Flags::client_secret` to it
    /// insists on.
    pub(crate) const SECRET: &str = "the-test-secret";

    /// A table of one Loopback entry, `Google test`, which signs in for `osv` and serves no
    /// address, whose endpoints are `fixture`'s. Leaked: a table is `'static`, and a test's leak is
    /// small.
    pub(crate) fn google_apps(fixture: &Fixture) -> &'static [RegisteredApp] {
        fn leaked(text: String) -> &'static str {
            Box::leak(text.into_boxed_str())
        }
        let origin = &fixture.origin;
        Box::leak(Box::new([RegisteredApp {
            id: "google-test",
            name: "Google test",
            host: None,
            catervas_connector: Some("osv"),
            flow: AppFlow::Loopback {
                authorization_endpoint: leaked(format!("{origin}/o/oauth2/v2/auth")),
            },
            client_id: "google-test-client",
            client_secret: Some(SECRET),
            scopes: &[SCOPE],
            issuer: leaked(origin.clone()),
            token_endpoint: leaked(format!("{origin}/token")),
            revocation_endpoint: None,
            install_url: None,
            settings_url: "https://myaccount.google.com/connections",
        }]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A GitHub-shaped entry: the host the real one will serve, and nothing else real.
    const GITHUB_SHAPED: RegisteredApp = RegisteredApp {
        id: "github",
        name: "GitHub",
        host: Some("api.githubcopilot.com"),
        catervas_connector: None,
        flow: AppFlow::Device {
            device_endpoint: "https://auth.example/device/code",
            verification_uri: "https://auth.example/device",
        },
        client_id: "the-apps-client-id",
        client_secret: None,
        scopes: &[],
        issuer: "https://auth.example/oauth",
        token_endpoint: "https://auth.example/oauth/access_token",
        revocation_endpoint: None,
        install_url: Some("https://auth.example/apps/catervas/installations/new"),
        settings_url: "https://auth.example/settings/apps",
    };

    fn serving(host: &'static str) -> RegisteredApp {
        RegisteredApp {
            host: Some(host),
            ..GITHUB_SHAPED
        }
    }

    /// An entry that signs in for one of Catervas's own connectors, whose server answers no address.
    fn signing_in_for(connector: &'static str, id: &'static str) -> RegisteredApp {
        RegisteredApp {
            id,
            host: None,
            catervas_connector: Some(connector),
            client_secret: Some("a-secret"),
            scopes: &["a-scope"],
            issuer: "https://accounts.example",
            ..GITHUB_SHAPED
        }
    }

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|word| (*word).to_string()).collect()
    }

    #[test]
    fn matches_github_by_its_exact_host() {
        let table = [GITHUB_SHAPED];
        for url in [
            "https://api.githubcopilot.com/mcp/",
            "https://API.githubcopilot.com/mcp/",
            "https://api.githubcopilot.com:443/mcp/",
        ] {
            assert_eq!(
                app_for(&table, url).map(|app| app.id),
                Some("github"),
                "{url}"
            );
        }
        // A table entry written in capitals matches the address all the same.
        let capitals = [serving("API.GitHubCopilot.com")];
        assert!(app_for(&capitals, "https://api.githubcopilot.com/mcp/").is_some());
        for url in [
            "https://api.githubcopilot.com.evil.example/mcp/",
            "http://api.githubcopilot.com/mcp/",
            "https://api.githubcopilot.com:8443/mcp/",
            "https://api.githubcopilot.com./mcp/",
            "https://api.githubcopilot.com@evil.example/mcp/",
            "https://user@api.githubcopilot.com/mcp/",
            "https://user:secret@api.githubcopilot.com/mcp/",
            "https://:secret@api.githubcopilot.com/mcp/",
            "not a url",
        ] {
            assert!(app_for(&table, url).is_none(), "{url}");
        }
    }

    #[test]
    fn matches_a_loopback_fixture_over_http() {
        let table = [serving("127.0.0.1")];
        assert_eq!(
            app_for(&table, "http://127.0.0.1:4000/mcp").map(|app| app.id),
            Some("github")
        );
        let table = [serving("10.0.0.1")];
        assert!(app_for(&table, "http://10.0.0.1:4000/mcp").is_none());
    }

    /// No build carries an app of Catervas's (ADR 0044): the table is empty, so it holds no Google
    /// entry and no entry for a connector of Catervas's or for an address, whatever the build's
    /// environment. A guard, proved by a mutation: a table built from `google(..)` fails it.
    #[test]
    fn the_shipped_table_is_empty_in_every_build() {
        assert_eq!(REGISTERED_APPS.len(), 0, "{REGISTERED_APPS:?}");
        assert!(REGISTERED_APPS.iter().all(|app| app.id != "google"));
        assert!(REGISTERED_APPS.iter().all(|app| app.host.is_none()));
        assert!(
            REGISTERED_APPS
                .iter()
                .all(|app| app.catervas_connector.is_none())
        );
    }

    #[test]
    fn google_s_entry_is_for_google_ads_alone() {
        let google = google("an-id.apps.googleusercontent.com", "a-secret");
        assert_eq!(google.id, "google");
        assert_eq!(google.name, "Google");
        assert_eq!(google.host, None, "it serves no address");
        assert_eq!(google.catervas_connector, Some("google-ads"));
        assert_eq!(
            google.flow,
            AppFlow::Loopback {
                authorization_endpoint: "https://accounts.google.com/o/oauth2/v2/auth"
            }
        );
        assert_eq!(google.client_id, "an-id.apps.googleusercontent.com");
        assert_eq!(google.client_secret, Some("a-secret"));
        assert_eq!(google.scopes, ["https://www.googleapis.com/auth/adwords"]);
        assert_eq!(google.issuer, "https://accounts.google.com");
        assert_eq!(google.token_endpoint, "https://oauth2.googleapis.com/token");
        assert_eq!(
            google.revocation_endpoint, None,
            "Google's revocation ends every grant"
        );
        assert_eq!(google.install_url, None);
        assert_eq!(
            google.settings_url,
            "https://myaccount.google.com/connections"
        );
    }

    /// A guard: a failing assertion must not print the secret.
    #[test]
    fn a_registered_app_does_not_print_its_secret() {
        let shown = format!("{:?}", google("an-id", "the-secret-never-shown"));
        assert!(!shown.contains("the-secret-never-shown"), "{shown}");
        assert!(shown.contains("google-ads"), "the rest is shown: {shown}");
    }

    #[test]
    fn matches_a_catervas_connector_by_its_exact_pair() {
        // The entry for the word, not the table's first, nor one whose word it begins.
        let table = [
            signing_in_for("ads", "for-ads"),
            signing_in_for("osv-2", "for-osv-2"),
            signing_in_for("osv", "for-osv"),
        ];
        let found = |command: &str, args: &[&str]| {
            app_for_catervas_connector(&table, command, &words(args)).map(|app| app.id)
        };
        assert_eq!(found("catervas", &["connector", "osv"]), Some("for-osv"));
        for (command, args) in [
            ("catervas", vec!["connector", "osv", "x"]),
            // `ads` is not one of Catervas's own connectors, so its entry is never answered.
            ("catervas", vec!["connector", "ads"]),
            ("catervas-osv", vec!["connector", "osv"]),
            ("/usr/bin/catervas", vec!["connector", "osv"]),
            ("CATERVAS", vec!["connector", "osv"]),
            ("npx", vec!["connector", "osv"]),
            ("catervas", vec!["osv"]),
        ] {
            assert_eq!(found(command, &args), None, "{command} {args:?}");
        }
    }

    #[test]
    fn an_entry_without_a_host_matches_no_address() {
        let table = [signing_in_for("osv", "for-osv")];
        for url in [
            "https://api.githubcopilot.com/mcp/",
            "https://accounts.example/mcp",
            "https://accounts.example:443/",
            "http://127.0.0.1:4000/mcp",
            "http://localhost:4000/mcp",
        ] {
            assert!(app_for(&table, url).is_none(), "{url}");
        }
    }
}

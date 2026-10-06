//! The apps Farik has registered with a service (ADR 0035, route 2): a public client Farik owns,
//! used only for the servers whose address the table names, so that a token from Farik's app is
//! never sent to a host the app does not serve.

use url::{Host, Url};

/// How a registered app signs a user in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppFlow {
    /// The device flow (RFC 8628): Farik asks the service for a code, the user types it on the
    /// service's page, and Farik polls until the service says yes.
    Device {
        /// Where the device code is asked for.
        device_endpoint: &'static str,
        /// The one page the user types the code on; an answer naming another is refused.
        verification_uri: &'static str,
    },
}

/// One app Farik has registered with a service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisteredApp {
    /// What a kept grant names it by.
    pub id: &'static str,
    /// What the screens call it.
    pub name: &'static str,
    /// The one host it serves, by the server's address and never by what the server's metadata
    /// says; none for an app that signs in for one of Farik's own connectors, whose server is no
    /// address.
    pub host: Option<&'static str>,
    /// The one of Farik's own connectors (`farik connector <name>`) it signs in for, if it does.
    pub farik_connector: Option<&'static str>,
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

/// Every app Farik has registered. Empty until the founder registers the first.
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
/// exactly, `farik connector <name>` for one of Farik's own connectors and the app's is `<name>`.
#[must_use]
pub fn app_for_farik_connector<'a>(
    apps: &'a [RegisteredApp],
    command: &str,
    args: &[String],
) -> Option<&'a RegisteredApp> {
    if !farik_roles::is_farik_connector(command, args) {
        return None;
    }
    let connector = args.get(1)?;
    apps.iter()
        .find(|app| app.farik_connector == Some(connector.as_str()))
}

/// Whether `url` names this computer, which is the only place Farik talks to over `http`.
fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
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
        farik_connector: None,
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
        install_url: Some("https://auth.example/apps/farik/installations/new"),
        settings_url: "https://auth.example/settings/apps",
    };

    fn serving(host: &'static str) -> RegisteredApp {
        RegisteredApp {
            host: Some(host),
            ..GITHUB_SHAPED
        }
    }

    /// An entry that signs in for one of Farik's own connectors, whose server answers no address.
    fn signing_in_for(connector: &'static str, id: &'static str) -> RegisteredApp {
        RegisteredApp {
            id,
            host: None,
            farik_connector: Some(connector),
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

    #[test]
    fn the_shipped_table_is_empty_until_the_founder_registers() {
        assert!(REGISTERED_APPS.is_empty());
    }

    #[test]
    fn matches_a_farik_connector_by_its_exact_pair() {
        // The entry for the word, not the table's first, nor one whose word it begins.
        let table = [
            signing_in_for("ads", "for-ads"),
            signing_in_for("osv-2", "for-osv-2"),
            signing_in_for("osv", "for-osv"),
        ];
        let found = |command: &str, args: &[&str]| {
            app_for_farik_connector(&table, command, &words(args)).map(|app| app.id)
        };
        assert_eq!(found("farik", &["connector", "osv"]), Some("for-osv"));
        for (command, args) in [
            ("farik", vec!["connector", "osv", "x"]),
            // `ads` is not one of Farik's own connectors, so its entry is never answered.
            ("farik", vec!["connector", "ads"]),
            ("farik-osv", vec!["connector", "osv"]),
            ("/usr/bin/farik", vec!["connector", "osv"]),
            ("FARIK", vec!["connector", "osv"]),
            ("npx", vec!["connector", "osv"]),
            ("farik", vec!["osv"]),
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

    /// A guard: the table holds no entry for a Farik connector until step 08e's Task 6.
    #[test]
    fn the_shipped_table_has_no_google_yet() {
        assert!(
            REGISTERED_APPS
                .iter()
                .all(|app| app.farik_connector.is_none())
        );
    }
}

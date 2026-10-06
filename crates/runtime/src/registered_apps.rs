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
    /// The one host it serves, by the server's address and never by what the server's metadata says.
    pub host: &'static str,
    /// How a user signs in to it.
    pub flow: AppFlow,
    /// The app's public client id; no secret is shipped.
    pub client_id: &'static str,
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
    apps.iter().find(|app| app.host.eq_ignore_ascii_case(host))
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
        host: "api.githubcopilot.com",
        flow: AppFlow::Device {
            device_endpoint: "https://auth.example/device/code",
            verification_uri: "https://auth.example/device",
        },
        client_id: "the-apps-client-id",
        issuer: "https://auth.example/oauth",
        token_endpoint: "https://auth.example/oauth/access_token",
        revocation_endpoint: None,
        install_url: Some("https://auth.example/apps/farik/installations/new"),
        settings_url: "https://auth.example/settings/apps",
    };

    fn serving(host: &'static str) -> RegisteredApp {
        RegisteredApp {
            host,
            ..GITHUB_SHAPED
        }
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
}

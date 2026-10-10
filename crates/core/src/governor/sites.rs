//! Which sites a role may read on the web (`docs/SPEC.md` sections 5.6, 6.10 and 8.6).
//!
//! A site is a host reached over `https`, in the ASCII form the `url` crate gives, with a host and
//! its `www.` twin counted as one. The Procurement Specialist reads only the sites the owner
//! allowed and the ones Catervas ships (ADR 0039); every other role's web reading is open. Nothing
//! here reads the world: the approved set is passed in.

use std::collections::BTreeSet;
use std::fmt;

use serde_json::Value;
use url::{Host, Url};

use crate::contract::Role;

/// The most characters a site's host has, once one trailing dot is removed.
const MOST_HOST: usize = 253;
/// The label a host and its twin differ by.
const WWW: &str = "www.";

/// How far a role's web reading reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebAccess {
    /// Any address the role's tiers allow.
    Open,
    /// Only the approved sites: a `WebFetch`, and every `url` or `urls` field and every string
    /// that is an address in a connector's call, must name one.
    ApprovedSites,
}

/// Whether a role's web reading is held to the approved sites: the Procurement Specialist's is,
/// for it reads sellers' pages while it holds the business's quotes and prices (ADR 0039), and
/// every other role's is open. It is fixed for a session when the session starts.
#[must_use]
pub fn web_access(role: Role) -> WebAccess {
    match role {
        Role::ProcurementSpecialist => WebAccess::ApprovedSites,
        Role::ProductManager
        | Role::ScrumMaster
        | Role::Architect
        | Role::SoftwareDeveloper
        | Role::MarketingSpecialist
        | Role::UiUxDesigner
        | Role::FinanceSpecialist
        | Role::Human => WebAccess::Open,
    }
}

/// Why an address names no site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteFault {
    /// It is not an address that stands alone: `a.com` has no scheme.
    NotUrl,
    /// Its scheme is not `https`.
    NotHttps,
    /// It carries a user name or a password.
    HasUserInfo,
    /// It names a port other than 443.
    HasPort,
    /// Its host is an IP address, or a name with no dot.
    NotADomain,
    /// Its host is longer than 253 characters.
    TooLong,
}

impl fmt::Display for SiteFault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotUrl => "it is not an address: write it with https:// before the name",
            Self::NotHttps => "only addresses that start with https:// are sites",
            Self::HasUserInfo => "an address with a user name or a password is not a site",
            Self::HasPort => "an address with a port other than 443 is not a site",
            Self::NotADomain => "its host is not a name with a dot, and an IP address is no site",
            Self::TooLong => "its host is longer than 253 characters",
        })
    }
}

impl std::error::Error for SiteFault {}

/// The site an address is on: its host in ASCII (lower case, an international name in its
/// `xn--` form), without one trailing dot and without one leading `www.` while a dot is left, so
/// that a host and its `www.` twin are one site. Path, query and fragment are not looked at; a
/// subdomain is another site.
///
/// # Errors
///
/// `NotUrl` for text that does not parse as an address, then `NotHttps`, `HasUserInfo` for a user
/// name or a password, `HasPort` for any port but 443, `NotADomain` for an IP address or a host
/// with no dot, and `TooLong` past 253 characters.
pub fn site_of(address: &str) -> Result<String, SiteFault> {
    let url = Url::parse(address).map_err(|_| SiteFault::NotUrl)?;
    if url.scheme() != "https" {
        return Err(SiteFault::NotHttps);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(SiteFault::HasUserInfo);
    }
    // The crate drops the scheme's own port, so `:443` is none.
    if url.port().is_some() {
        return Err(SiteFault::HasPort);
    }
    let Some(Host::Domain(domain)) = url.host() else {
        return Err(SiteFault::NotADomain);
    };
    let host = domain.strip_suffix('.').unwrap_or(domain);
    if host.len() > MOST_HOST {
        return Err(SiteFault::TooLong);
    }
    if !host.contains('.') {
        return Err(SiteFault::NotADomain);
    }
    Ok(match host.strip_prefix(WWW) {
        Some(rest) if rest.contains('.') => rest.to_string(),
        _ => host.to_string(),
    })
}

/// An address a held call named that is on no approved site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteRefusal {
    /// The field's value: the string, or the JSON of a value that is not one.
    pub address: String,
}

/// Holds a call's addresses to the approved sites: every field named `url`, at any depth of
/// `input`, must be a string whose site is in `approved`, and so must each item of every field
/// named `urls`, which must be an array of strings. Besides, whatever the name of its field and
/// however deep, a string that is itself an address, one that parses as a URL with a host, must
/// name an approved site too, so that `webhook_url`, `href` or `image_url` is held as `url` is. A
/// string that is no address as a whole (a sentence that holds one, a name with no scheme, a
/// `mailto:` or a `data:` URL, which have no host) is not judged (spec 5.6, 8.6).
///
/// # Errors
///
/// `SiteRefusal` with the first address found that names no approved site, or the JSON of a
/// `url` that is not a string, of a `urls` that is not an array, or of an item that is not a
/// string.
pub fn check_site_urls(input: &Value, approved: &BTreeSet<String>) -> Result<(), SiteRefusal> {
    match input {
        Value::Object(fields) => fields.iter().try_for_each(|(key, field)| {
            match key.as_str() {
                "url" => check_address(field, approved)?,
                "urls" => match field {
                    Value::Array(items) => items
                        .iter()
                        .try_for_each(|item| check_address(item, approved))?,
                    other => {
                        return Err(SiteRefusal {
                            address: other.to_string(),
                        });
                    }
                },
                _ => {}
            }
            check_site_urls(field, approved)
        }),
        Value::Array(items) => items
            .iter()
            .try_for_each(|item| check_site_urls(item, approved)),
        Value::String(text) if names_a_host(text) => check_address(input, approved),
        _ => Ok(()),
    }
}

/// Whether `text` is an address as a whole that has a host, whatever its scheme: `mailto:` and
/// `data:` URLs have none, and a word, a path or a sentence is no URL.
fn names_a_host(text: &str) -> bool {
    Url::parse(text).is_ok_and(|url| url.host().is_some())
}

/// One address: a string on an approved site.
fn check_address(value: &Value, approved: &BTreeSet<String>) -> Result<(), SiteRefusal> {
    let on_an_approved_site = value
        .as_str()
        .and_then(|address| site_of(address).ok())
        .is_some_and(|site| approved.contains(&site));
    if on_an_approved_site {
        Ok(())
    } else {
        Err(refused(value))
    }
}

fn refused(value: &Value) -> SiteRefusal {
    SiteRefusal {
        address: value
            .as_str()
            .map_or_else(|| value.to_string(), str::to_string),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::json;

    use super::{SiteFault, WebAccess, check_site_urls, site_of, web_access};
    use crate::contract::Role;

    #[test]
    fn a_site_is_an_https_named_host() {
        for address in [
            "https://Shop.Example.com/p?q=1#f",
            "https://shop.example.com:443/",
            "https://shop.example.com./",
            "https://www.Shop.Example.com/",
        ] {
            assert_eq!(
                site_of(address).as_deref(),
                Ok("shop.example.com"),
                "{address}"
            );
        }
    }

    #[test]
    fn refuses_what_is_not_a_site() {
        for (address, fault) in [
            ("http://a.com", SiteFault::NotHttps),
            ("ftp://a.com/", SiteFault::NotHttps),
            ("https://a.com:8443/", SiteFault::HasPort),
            ("https://u:p@a.com/", SiteFault::HasUserInfo),
            ("https://u@a.com/", SiteFault::HasUserInfo),
            ("https://127.0.0.1/", SiteFault::NotADomain),
            ("https://[::1]/", SiteFault::NotADomain),
            ("https://localhost/", SiteFault::NotADomain),
            ("a.com", SiteFault::NotUrl),
            ("", SiteFault::NotUrl),
        ] {
            assert_eq!(site_of(address), Err(fault), "{address:?}");
        }
    }

    #[test]
    fn a_site_is_at_most_253_characters() {
        let longest = format!(
            "{}.{}.{}.{}",
            "a".repeat(63),
            "b".repeat(63),
            "c".repeat(63),
            "d".repeat(61)
        );
        assert_eq!(longest.len(), 253);
        assert_eq!(
            site_of(&format!("https://{longest}/")).as_deref(),
            Ok(longest.as_str())
        );
        assert_eq!(
            site_of(&format!("https://{longest}./")).as_deref(),
            Ok(longest.as_str()),
            "the trailing dot does not count"
        );
        let too_long = format!(
            "{}.{}.{}.{}",
            "a".repeat(63),
            "b".repeat(63),
            "c".repeat(63),
            "d".repeat(62)
        );
        assert_eq!(too_long.len(), 254);
        assert_eq!(
            site_of(&format!("https://{too_long}/")),
            Err(SiteFault::TooLong)
        );
    }

    #[test]
    fn an_international_name_is_its_ascii_form() {
        assert_eq!(
            site_of("https://bücher.example/").as_deref(),
            Ok("xn--bcher-kva.example")
        );
    }

    #[test]
    fn www_is_the_same_site_and_nothing_else_is() {
        assert_eq!(
            site_of("https://www.example.com/").as_deref(),
            Ok("example.com")
        );
        assert_eq!(
            site_of("https://example.com/").as_deref(),
            Ok("example.com")
        );
        for own in [
            "shop.example.com",
            "example.com.evil.net",
            "wwwexample.com",
            "www.com",
        ] {
            assert_eq!(
                site_of(&format!("https://{own}/")).as_deref(),
                Ok(own),
                "{own}"
            );
        }
        assert_eq!(site_of("https://www.com/").as_deref(), Ok("www.com"));
        // Only one `www.` is the twin's: another is part of the name.
        assert_eq!(
            site_of("https://www.www.example.com/").as_deref(),
            Ok("www.example.com")
        );
    }

    fn approved(hosts: &[&str]) -> BTreeSet<String> {
        hosts.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn holds_every_url_field_to_the_sites() {
        let sites = approved(&["a.com"]);
        for input in [
            json!({ "url": "https://a.com/x" }),
            json!({ "q": { "urls": ["https://www.a.com/"] } }),
            json!({ "query": "no address here", "limit": 3 }),
        ] {
            assert_eq!(check_site_urls(&input, &sites), Ok(()), "{input}");
        }
        for (input, address) in [
            (json!({ "url": "https://b.com/" }), "https://b.com/"),
            (
                json!({ "deep": [{ "url": "https://b.com/" }] }),
                "https://b.com/",
            ),
            (
                json!({ "urls": ["https://a.com/", "https://b.com/"] }),
                "https://b.com/",
            ),
            (json!({ "url": 7 }), "7"),
            (json!({ "urls": "https://a.com/" }), "\"https://a.com/\""),
            (json!({ "urls": ["https://a.com/", 7] }), "7"),
            (json!({ "url": "http://a.com/" }), "http://a.com/"),
            (json!({ "url": "a.com" }), "a.com"),
        ] {
            let refused = check_site_urls(&input, &sites).expect_err(&input.to_string());
            assert_eq!(refused.address, address, "{input}");
        }
    }

    #[test]
    fn holds_an_address_in_any_field() {
        let sites = approved(&["a.com"]);
        for input in [
            json!({ "URL": "https://b.com/" }),
            json!({ "webhook_url": "https://b.com/hook" }),
            json!({ "link": "https://b.com/" }),
            json!({ "requests": [{ "href": "https://b.com/" }] }),
            json!({ "uri": "https://b.com/" }),
            json!({ "params": { "engine": "google_reverse_image", "image_url": "https://b.com/i.png" } }),
            json!({ "list": ["https://a.com/", "https://b.com/"] }),
            json!({ "link": "http://a.com/" }),
            json!({ "link": "https://127.0.0.1/" }),
            json!({ "link": "https://a.com:8443/" }),
            json!({ "link": " https://b.com/ " }),
        ] {
            assert!(check_site_urls(&input, &sites).is_err(), "{input}");
        }
        let refused = check_site_urls(
            &json!({ "params": { "image_url": "https://b.com/i.png" } }),
            &sites,
        )
        .expect_err("an unapproved address");
        assert_eq!(refused.address, "https://b.com/i.png");

        for input in [
            json!({ "link": "https://a.com/x" }),
            json!({ "requests": [{ "href": "https://www.a.com/" }] }),
            json!({ "note": "see https://b.com/ for prices", "limit": 3 }),
            json!({ "query": "boxes", "sku": "ABC:123", "email": "mailto:me@b.com" }),
            json!({ "name": "a.com", "path": "/etc/hosts", "file": "file:///etc/hosts" }),
            json!({ "blob": "data:text/plain;base64,aGk=", "empty": "", "n": 7, "ok": true }),
        ] {
            assert_eq!(check_site_urls(&input, &sites), Ok(()), "{input}");
        }
    }

    #[test]
    fn only_procurement_is_held() {
        for role in [
            Role::ProductManager,
            Role::ScrumMaster,
            Role::Architect,
            Role::SoftwareDeveloper,
            Role::MarketingSpecialist,
            Role::UiUxDesigner,
            Role::FinanceSpecialist,
            Role::Human,
        ] {
            assert_eq!(web_access(role), WebAccess::Open, "{role}");
        }
        assert_eq!(
            web_access(Role::ProcurementSpecialist),
            WebAccess::ApprovedSites
        );
    }
}

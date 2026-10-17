//! The site an address is on (`docs/SPEC.md` sections 6.10 and 8.6).
//!
//! A site is a host reached over `https`, in the ASCII form the `url` crate gives, with a host and
//! its `www.` twin counted as one. No role is held to a list of sites any more (ADR 0061): this
//! module only reads an address's host, which seller messages, data pipeline requests and the
//! waiting rows show.

use std::fmt;

use url::{Host, Url};

/// The most characters a site's host has, once one trailing dot is removed.
const MOST_HOST: usize = 253;
/// The label a host and its twin differ by.
const WWW: &str = "www.";

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

#[cfg(test)]
mod tests {
    use super::{SiteFault, site_of};

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
}

//! The catalog that sorts a connector into a kind of service.

/// What kind of work a service is for, in the order services are listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ServiceKind {
    /// Books and invoices.
    Bookkeeping,
    /// Spreadsheets.
    Spreadsheets,
    /// Taking payments.
    Payments,
    /// Selling goods.
    Selling,
    /// Marketing and advertising.
    Marketing,
    /// Code.
    Code,
    /// Anything the catalog does not know.
    Other,
}

impl ServiceKind {
    /// The kind's value in `--json`.
    #[must_use]
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Bookkeeping => "bookkeeping",
            Self::Spreadsheets => "spreadsheets",
            Self::Payments => "payments",
            Self::Selling => "selling",
            Self::Marketing => "marketing",
            Self::Code => "code",
            Self::Other => "other",
        }
    }
}

/// One service the catalog knows.
#[derive(Debug, PartialEq, Eq)]
pub struct CatalogEntry {
    /// The service's id, its record's key.
    pub id: &'static str,
    /// The name it is shown under.
    pub name: &'static str,
    /// The kind of work it is for.
    pub kind: ServiceKind,
    /// The normalised names a connector for it goes by.
    pub names: &'static [&'static str],
    /// The hosts its connectors live on, and their subdomains.
    pub hosts: &'static [&'static str],
}

const fn entry(
    id: &'static str,
    name: &'static str,
    kind: ServiceKind,
    names: &'static [&'static str],
    hosts: &'static [&'static str],
) -> CatalogEntry {
    CatalogEntry {
        id,
        name,
        kind,
        names,
        hosts,
    }
}

/// Every service the catalog knows. A table the compiler checks, not a data file.
pub const CATALOG: [CatalogEntry; 17] = [
    entry(
        "zoho_books",
        "Zoho Books",
        ServiceKind::Bookkeeping,
        &["zohobooks"],
        &[
            "books.zoho.com",
            "books.zoho.eu",
            "books.zoho.in",
            "books.zoho.com.au",
        ],
    ),
    entry(
        "quickbooks",
        "QuickBooks",
        ServiceKind::Bookkeeping,
        &["quickbooks", "qbo", "intuitquickbooks"],
        &["quickbooks.api.intuit.com"],
    ),
    entry(
        "xero",
        "Xero",
        ServiceKind::Bookkeeping,
        &["xero"],
        &["xero.com"],
    ),
    entry(
        "freshbooks",
        "FreshBooks",
        ServiceKind::Bookkeeping,
        &["freshbooks"],
        &["freshbooks.com"],
    ),
    entry(
        "wave",
        "Wave",
        ServiceKind::Bookkeeping,
        &["wave", "waveapps", "waveaccounting"],
        &["waveapps.com"],
    ),
    entry(
        "sage",
        "Sage",
        ServiceKind::Bookkeeping,
        &["sage", "sageaccounting", "sageintacct"],
        &["sage.com", "intacct.com"],
    ),
    entry(
        "microsoft_365",
        "Microsoft 365 and Excel",
        ServiceKind::Spreadsheets,
        &[
            "excel",
            "microsoft365",
            "m365",
            "office365",
            "microsoftgraph",
            "msgraph",
        ],
        &["graph.microsoft.com", "office.com", "office365.com"],
    ),
    entry(
        "google_sheets",
        "Google Sheets",
        ServiceKind::Spreadsheets,
        &["googlesheets", "gsheets", "sheets"],
        &["sheets.googleapis.com"],
    ),
    entry(
        "stripe",
        "Stripe",
        ServiceKind::Payments,
        &["stripe"],
        &["stripe.com"],
    ),
    entry(
        "paypal",
        "PayPal",
        ServiceKind::Payments,
        &["paypal"],
        &["paypal.com"],
    ),
    entry(
        "shopify",
        "Shopify",
        ServiceKind::Selling,
        &["shopify"],
        &["shopify.com", "myshopify.com"],
    ),
    entry(
        "amazon_seller_central",
        "Amazon Seller Central",
        ServiceKind::Selling,
        &[
            "amazonsellercentral",
            "sellercentral",
            "amazonsellingpartner",
            "spapi",
        ],
        &[
            "sellercentral.amazon.com",
            "sellingpartnerapi-na.amazon.com",
            "sellingpartnerapi-eu.amazon.com",
            "sellingpartnerapi-fe.amazon.com",
        ],
    ),
    entry(
        "buffer",
        "Buffer",
        ServiceKind::Marketing,
        &["buffer"],
        &["buffer.com"],
    ),
    entry(
        "google_ads",
        "Google Ads",
        ServiceKind::Marketing,
        &["googleads", "adwords"],
        &["googleads.googleapis.com"],
    ),
    entry(
        "github",
        "GitHub",
        ServiceKind::Code,
        &["github"],
        &["github.com", "githubcopilot.com"],
    ),
    entry(
        "vercel",
        "Vercel",
        ServiceKind::Code,
        &["vercel"],
        &["vercel.com"],
    ),
    entry(
        "sentry",
        "Sentry",
        ServiceKind::Code,
        &["sentry"],
        &["sentry.io"],
    ),
];

/// Servers that are never a service of the owner's: Catervas's own (before and after its rename),
/// Claude Code's session and remote-control servers and IDE bridge, the Claude app's browser,
/// preview, device and Chrome surfaces, and Claude Docs. Compared normalised.
pub const NOT_SERVICES: [&str; 11] = [
    "catervas",
    "farik",
    "ccd_session",
    "claude-code-remote",
    "ide",
    "Claude_Browser",
    "Claude_Preview",
    "remote-devices",
    "computer-use",
    "claude-in-chrome",
    "Claude_Docs",
];

/// A name as the catalog compares it: lower case, ASCII letters and digits only, then one leading
/// `mcp` and one trailing `mcpserver`, `mcp` or `server` removed.
#[must_use]
pub fn normalise(name: &str) -> String {
    let plain: String = name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let plain = plain.strip_prefix("mcp").unwrap_or(&plain);
    for suffix in ["mcpserver", "mcp", "server"] {
        if let Some(stripped) = plain.strip_suffix(suffix) {
            return stripped.to_string();
        }
    }
    plain.to_string()
}

/// Whether a server counts as a service of the owner's at all.
#[must_use]
pub fn is_a_service(name: &str) -> bool {
    let name = normalise(name);
    !NOT_SERVICES.iter().any(|own| normalise(own) == name)
}

fn is_on(host: &str, listed: &str) -> bool {
    host == listed
        || host
            .strip_suffix(listed)
            .is_some_and(|front| front.ends_with('.'))
}

/// The service a connector belongs to: by host first (equal to a listed host or on a subdomain of
/// it), then by its normalised name, then by its program's normalised file stem.
#[must_use]
pub fn match_service(
    name: &str,
    host: Option<&str>,
    program: Option<&str>,
) -> Option<&'static CatalogEntry> {
    let by_names = |wanted: &str| CATALOG.iter().find(|e| e.names.contains(&wanted));
    host.map(str::to_ascii_lowercase)
        .and_then(|host| {
            CATALOG
                .iter()
                .find(|e| e.hosts.iter().any(|listed| is_on(&host, listed)))
        })
        .or_else(|| by_names(&normalise(name)))
        .or_else(|| {
            let program = program?;
            let stem = program.rsplit_once('.').map_or(program, |(stem, _)| stem);
            by_names(&normalise(stem))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_each_catalog_entry() {
        let mut ids = std::collections::BTreeSet::new();
        assert_eq!(CATALOG.len(), 17);
        for entry in &CATALOG {
            assert!(ids.insert(entry.id), "{} twice", entry.id);
            for name in entry.names {
                assert_eq!(
                    match_service(name, None, None).map(|e| e.id),
                    Some(entry.id),
                    "{name}"
                );
            }
            for host in entry.hosts {
                let on_a_subdomain = format!("mcp.{host}");
                for tried in [*host, on_a_subdomain.as_str()] {
                    let found = match_service("whatever", Some(tried), None);
                    assert_eq!(found.map(|e| e.id), Some(entry.id), "{tried}");
                }
            }
        }
        for name in [
            "Zoho Books",
            "zoho_books",
            "zoho-books-mcp",
            "mcp-zoho-books",
        ] {
            assert_eq!(
                match_service(name, None, None).map(|e| e.id),
                Some("zoho_books"),
                "{name}"
            );
        }
        assert!(match_service("Unknown Thing", None, None).is_none());
        assert_eq!(normalise("Zoho Books"), "zohobooks");
    }

    #[test]
    fn matches_by_host_then_name_then_program() {
        let id = |name, host, program| match_service(name, host, program).map(|e| e.id);
        assert_eq!(id("github", Some("mcp.stripe.com"), None), Some("stripe"));
        assert_eq!(
            id("my-books", None, Some("quickbooks-mcp.exe")),
            Some("quickbooks")
        );
        assert_eq!(id("github", None, Some("stripe-mcp")), Some("github"));
        assert_eq!(id("x", Some("notstripe.com"), None), None);
    }

    #[test]
    fn leaves_out_catervas_and_claude_codes_own_servers() {
        assert_eq!(NOT_SERVICES.len(), 11);
        for name in NOT_SERVICES {
            assert!(!is_a_service(name), "{name}");
        }
        for name in ["Catervas", "FARIK", "farik-mcp", "Claude Docs"] {
            assert!(!is_a_service(name), "{name}");
        }
        assert!(is_a_service("zoho-books"));
        assert_eq!(ServiceKind::Bookkeeping.as_wire(), "bookkeeping");
        assert!(ServiceKind::Code < ServiceKind::Other);
    }
}

//! Farik's approved sites (`docs/SPEC.md` 6.10, ADR 0039): the long-established shops the
//! Procurement Specialist may read from its first task, shipped in its folder as public data.
//! The file is held to `docs/schemas/approved-sites.schema.json`, and then to the rule the schema
//! cannot say: each host is a bare site, and none is listed twice.

use std::collections::BTreeSet;
use std::fmt;
use std::sync::LazyLock;

use farik_core::governor::sites::site_of;
use jsonschema::Validator;
use serde_json::Value;

use crate::generated::approved_sites::FarikApprovedSites;
pub use crate::generated::approved_sites::SiteCategory;

const SCHEMA_JSON: &str = include_str!("../../../docs/schemas/approved-sites.schema.json");
const SITES_YAML: &str = include_str!("../roles/procurement_specialist/approved_sites.yaml");

static VALIDATOR: LazyLock<Validator> = LazyLock::new(|| {
    let schema: Value = serde_json::from_str(SCHEMA_JSON).expect(
        "the embedded approved-sites schema is valid JSON: it is the file in docs/schemas/ that \
         typify generated this module's types from at compile time",
    );
    jsonschema::validator_for(&schema).expect(
        "the embedded approved-sites schema compiles: it is JSON Schema 2020-12 with no external \
         references, and the generator already parsed it",
    )
});

/// One shop on Farik's list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FarikSite {
    /// The shop's primary domain, as `site_of` gives it back.
    pub host: String,
    /// The shop's name as the owner reads it.
    pub shop: String,
    /// What the shop sells.
    pub category: SiteCategory,
}

/// Why a list of sites could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SitesError {
    /// The file is not Farik's list of sites.
    Invalid {
        /// Each refusal as `<json pointer>: <code>: <words>`, joined by `; `.
        detail: String,
    },
}

impl fmt::Display for SitesError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid { detail } => {
                write!(
                    formatter,
                    "the list of approved sites is not valid: {detail}"
                )
            }
        }
    }
}

impl std::error::Error for SitesError {}

/// Reads a list of sites: `yaml` held to the schema, then each host held to being a bare site, a
/// lower-case ASCII host with a dot and no scheme, path, port, IP address or leading `www.`, and
/// listed once.
///
/// # Errors
///
/// `Invalid`, naming each refusal at its entry as `<json pointer>: <code>: <words>`.
pub fn parse_farik_sites(yaml: &str) -> Result<Vec<FarikSite>, SitesError> {
    let invalid = |detail: String| SitesError::Invalid { detail };
    let value = crate::yaml_value(yaml, "approved_sites.yaml").map_err(invalid)?;
    let schema: Vec<String> = VALIDATOR
        .iter_errors(&value)
        .map(|error| format!("{}: schema: {error}", error.instance_path()))
        .collect();
    if !schema.is_empty() {
        return Err(invalid(schema.join("; ")));
    }
    let file: FarikApprovedSites = serde_json::from_value(value).map_err(|error| {
        invalid(format!(
            "the schema passed but the typed list could not be built: {error}"
        ))
    })?;
    let mut refused = Vec::new();
    let mut seen = BTreeSet::new();
    let mut sites = Vec::new();
    for (index, site) in file.sites.into_iter().enumerate() {
        let at = format!("/sites/{index}/host");
        let bare = site_of(&format!("https://{}/", site.host)).is_ok_and(|back| back == site.host);
        if !bare {
            refused.push(format!(
                "{at}: host_not_bare: {:?} is not a bare lower-case ASCII host with a dot; write \
                 no scheme, path, port, IP address or leading www., and an international name \
                 as its xn-- form",
                site.host
            ));
        } else if !seen.insert(site.host.clone()) {
            refused.push(format!(
                "{at}: host_twice: {} is on the list already",
                site.host
            ));
        }
        sites.push(FarikSite {
            host: site.host,
            shop: site.shop.to_string(),
            category: site.category,
        });
    }
    if refused.is_empty() {
        Ok(sites)
    } else {
        Err(invalid(refused.join("; ")))
    }
}

static FARIK_SITES: LazyLock<Vec<FarikSite>> = LazyLock::new(|| {
    parse_farik_sites(SITES_YAML).expect(
        "the shipped approved_sites.yaml is a valid list: the test \
         farik_s_approved_sites_are_well_formed rules the failure out",
    )
});

/// Farik's approved sites in the order the file lists them, read once: every team's Procurement
/// Specialist may read these from its first task, unless its owner turned one off.
#[must_use]
pub fn farik_sites() -> &'static [FarikSite] {
    &FARIK_SITES
}

#[cfg(test)]
mod tests {
    use farik_core::governor::sites::site_of;

    use super::{FarikSite, SiteCategory, SitesError, farik_sites, parse_farik_sites};

    /// One entry of a hand-written list.
    fn entry(host: &str, shop: &str, category: &str) -> String {
        format!("  - host: \"{host}\"\n    shop: \"{shop}\"\n    category: {category}\n")
    }

    fn list(entries: &[String]) -> String {
        format!("sites:\n{}", entries.concat())
    }

    fn refusal_of(yaml: &str) -> String {
        match parse_farik_sites(yaml) {
            Err(SitesError::Invalid { detail }) => detail,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn refuses_a_malformed_site_list() {
        let good = entry("good.example", "Good", "general_marketplace");
        for (bad, at) in [
            (entry("https://a.com", "A", "furniture"), "/sites/1/host"),
            (entry("A.com", "A", "furniture"), "/sites/1/host"),
            (entry("a.com:443", "A", "furniture"), "/sites/1/host"),
            (entry("a.com/x", "A", "furniture"), "/sites/1/host"),
            (entry("10.0.0.1", "A", "furniture"), "/sites/1/host"),
            (entry("bücher.example", "A", "furniture"), "/sites/1/host"),
            (entry("www.a.com", "A", "furniture"), "/sites/1/host"),
            (entry("good.example", "Again", "furniture"), "/sites/1/host"),
            (entry("a.com", "A", "gadgets"), "/sites/1/category"),
            (entry("a.com", "", "furniture"), "/sites/1/shop"),
            (entry("a.com", "A\\nB", "furniture"), "/sites/1/shop"),
            (
                entry("a.com", &"A".repeat(61), "furniture"),
                "/sites/1/shop",
            ),
            (
                "  - host: a.com\n    shop: A\n    category: furniture\n    price: 1\n".to_string(),
                "/sites/1",
            ),
        ] {
            let detail = refusal_of(&list(&[good.clone(), bad.clone()]));
            assert!(detail.contains(at), "{bad}: {detail}");
        }
        assert!(refusal_of("sites: nope\n").contains("/sites"));
        assert!(refusal_of("sites: []\nextra: 1\n").contains("extra"));
    }

    #[test]
    fn parses_a_good_list_in_order() {
        let yaml = list(&[
            entry("b.example", "Bee", "furniture"),
            entry("a.example", "Ay", "software"),
        ]);
        assert_eq!(
            parse_farik_sites(&yaml),
            Ok(vec![
                FarikSite {
                    host: "b.example".to_string(),
                    shop: "Bee".to_string(),
                    category: SiteCategory::Furniture,
                },
                FarikSite {
                    host: "a.example".to_string(),
                    shop: "Ay".to_string(),
                    category: SiteCategory::Software,
                },
            ])
        );
    }

    #[test]
    fn farik_s_approved_sites_are_well_formed() {
        let sites = farik_sites();
        assert!(!sites.is_empty());
        let mut seen = std::collections::BTreeSet::new();
        for site in sites {
            assert_eq!(
                site_of(&format!("https://{}/", site.host)).as_deref(),
                Ok(site.host.as_str()),
                "{} is a bare host",
                site.host
            );
            assert!(seen.insert(site.host.clone()), "{} twice", site.host);
            assert!(!site.shop.is_empty(), "{} has no shop", site.host);
        }
        for category in [
            "general_marketplace",
            "office_supplies",
            "industrial_supplies",
            "packaging_and_shipping",
            "electronic_components",
            "computers_and_it",
            "furniture",
            "food_service",
            "printing",
            "software",
        ] {
            assert!(
                sites
                    .iter()
                    .any(|site| site.category.to_string() == category),
                "no shop in {category}"
            );
        }
    }
}

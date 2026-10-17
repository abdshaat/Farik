//! An address cut for an answer or a refusal. Catervas keeps no list of sites any more (ADR 0061):
//! a role may open any page, so what was here before (`catervas_request_sites`,
//! `catervas_read_sites`) is gone.

/// The most characters of an address an answer or a refusal shows.
const SHOWN_URL: usize = 200;

/// An address cut for an answer or a refusal, so that what an agent wrote never fills the log.
pub(crate) fn shown(url: &str) -> String {
    if url.chars().count() > SHOWN_URL {
        format!("{}...", url.chars().take(SHOWN_URL).collect::<String>())
    } else {
        url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{SHOWN_URL, shown};

    #[test]
    fn cuts_a_long_address_and_leaves_a_short_one() {
        assert_eq!(shown("https://shop.example/"), "https://shop.example/");
        let long = format!("https://shop.example/{}", "a".repeat(SHOWN_URL));
        let cut = shown(&long);
        assert!(cut.ends_with("..."), "{cut}");
        assert_eq!(cut.chars().count(), SHOWN_URL + 3);
    }
}

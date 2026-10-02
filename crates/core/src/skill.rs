//! A skill's hash (`docs/SPEC.md` 6.7, ADR 0034): what a team file pins and a person confirms.

use std::collections::BTreeMap;

use serde_json::json;

use crate::team::{canonical_json, sha256_bytes_hex, sha256_hex};

/// The sha256, in lower-case hex, of the canonical JSON of `{ <relative path>: <sha256 hex of the
/// file's bytes> }` over every file in a skill's folder.
#[must_use]
pub fn skill_sha256(files: &BTreeMap<String, Vec<u8>>) -> String {
    let listed: BTreeMap<&str, String> = files
        .iter()
        .map(|(path, bytes)| (path.as_str(), sha256_bytes_hex(bytes)))
        .collect();
    sha256_hex(&canonical_json(&json!(listed)))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::skill_sha256;

    fn files(entries: &[(&str, &str)]) -> BTreeMap<String, Vec<u8>> {
        entries
            .iter()
            .map(|(path, text)| ((*path).to_string(), text.as_bytes().to_vec()))
            .collect()
    }

    #[test]
    fn the_skill_hash_sees_every_file_and_ignores_order() {
        let mut forward = BTreeMap::new();
        forward.insert("SKILL.md".to_string(), b"one".to_vec());
        forward.insert("references/a.md".to_string(), b"two".to_vec());
        let mut backward = BTreeMap::new();
        backward.insert("references/a.md".to_string(), b"two".to_vec());
        backward.insert("SKILL.md".to_string(), b"one".to_vec());
        let hash = skill_sha256(&forward);
        assert_eq!(hash.len(), 64);
        assert!(
            hash.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert_eq!(
            skill_sha256(&backward),
            hash,
            "insertion order does not matter"
        );

        let base = files(&[("SKILL.md", "one"), ("references/a.md", "two")]);
        assert_eq!(skill_sha256(&base), hash);
        let changed_byte = files(&[("SKILL.md", "onf"), ("references/a.md", "two")]);
        let renamed = files(&[("SKILL.md", "one"), ("references/b.md", "two")]);
        let added = files(&[
            ("SKILL.md", "one"),
            ("references/a.md", "two"),
            ("c.md", ""),
        ]);
        for (what, other) in [
            ("a byte", changed_byte),
            ("a name", renamed),
            ("a file", added),
        ] {
            assert_ne!(skill_sha256(&other), hash, "{what} changes the hash");
        }
    }
}

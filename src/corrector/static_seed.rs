//! Loads the curated static correction seed embedded at compile time.

use super::dictionary::{Correction, Source};

/// The seed JSON, compiled into the binary.
const SEED_JSON: &str = include_str!("tech_terms.json");

#[derive(serde::Deserialize)]
struct SeedEntry {
    heard: String,
    canonical: String,
}

/// Parse the embedded seed into corrections. Panics only if the shipped JSON is
/// malformed, which a test guarantees it is not.
pub fn load() -> Vec<Correction> {
    let raw: Vec<SeedEntry> =
        serde_json::from_str(SEED_JSON).expect("embedded tech_terms.json is valid");
    raw.into_iter()
        .map(|e| Correction {
            heard: e.heard.to_lowercase(),
            canonical: e.canonical,
            source: Source::Static,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_parses_and_is_nonempty() {
        let seed = load();
        assert!(seed.len() >= 20, "expected a real seed, got {}", seed.len());
    }

    #[test]
    fn seed_heard_keys_are_lowercase() {
        for c in load() {
            assert_eq!(
                c.heard,
                c.heard.to_lowercase(),
                "heard key must be lowercase: {:?}",
                c.heard
            );
        }
    }
}

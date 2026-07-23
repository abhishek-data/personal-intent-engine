use super::{AppliedFix, CorrectionOutcome, Tier};

/// Origin of a correction entry; user entries override static ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Static,
    User,
}

/// One heard->canonical mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Correction {
    pub heard: String,
    pub canonical: String,
    pub source: Source,
}

/// A compiled set of corrections with an exact longest-phrase matcher.
pub struct CorrectionDict {
    entries: Vec<Correction>,
    /// Entry indices sorted by descending heard-word-count (longest first),
    /// so multi-word phrases match before single words.
    by_len: Vec<usize>,
}

/// Lowercase and trim surrounding ASCII punctuation for matching only.
/// Returns (normalized, trailing_punctuation_of_token).
fn normalize(token: &str) -> (String, &str) {
    let trimmed = token.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    let trailing = &token[trimmed.len()
        + (token.len()
            - token
                .trim_start_matches(|c: char| !c.is_ascii_alphanumeric())
                .len())..];
    (trimmed.to_lowercase(), trailing)
}

impl CorrectionDict {
    pub fn from_entries(entries: Vec<Correction>) -> Self {
        let mut by_len: Vec<usize> = (0..entries.len()).collect();
        by_len.sort_by_key(|&i| std::cmp::Reverse(entries[i].heard.split_whitespace().count()));
        Self { entries, by_len }
    }

    /// Replace exact (case-insensitive, whitespace-tokenized) phrase matches,
    /// longest phrase first. Trailing punctuation on the last matched token is
    /// preserved. Non-matching tokens are emitted verbatim.
    pub fn apply_exact(&self, text: &str) -> CorrectionOutcome {
        let tokens: Vec<&str> = text.split_whitespace().collect();
        let norm: Vec<String> = tokens.iter().map(|t| normalize(t).0).collect();

        let mut out_tokens: Vec<String> = Vec::with_capacity(tokens.len());
        let mut applied = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            let mut matched = None;
            for &idx in &self.by_len {
                let phrase: Vec<&str> = self.entries[idx].heard.split_whitespace().collect();
                let n = phrase.len();
                if n == 0 || i + n > norm.len() {
                    continue;
                }
                if (0..n).all(|k| norm[i + k] == phrase[k]) {
                    matched = Some((idx, n));
                    break;
                }
            }
            if let Some((idx, n)) = matched {
                // Preserve trailing punctuation of the final matched token.
                let trailing = normalize(tokens[i + n - 1]).1;
                out_tokens.push(format!("{}{}", self.entries[idx].canonical, trailing));
                applied.push(AppliedFix {
                    from: (0..n)
                        .map(|k| norm[i + k].as_str())
                        .collect::<Vec<_>>()
                        .join(" "),
                    to: self.entries[idx].canonical.clone(),
                    tier: Tier::Exact,
                });
                i += n;
            } else {
                out_tokens.push(tokens[i].to_string());
                i += 1;
            }
        }

        if applied.is_empty() {
            return CorrectionOutcome {
                text: text.to_string(),
                applied,
            };
        }
        CorrectionOutcome {
            text: out_tokens.join(" "),
            applied,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict() -> CorrectionDict {
        CorrectionDict::from_entries(vec![
            Correction {
                heard: "next jazz".into(),
                canonical: "Next.js".into(),
                source: Source::Static,
            },
            Correction {
                heard: "next".into(),
                canonical: "NEXT-SHOULD-NOT-FIRE".into(),
                source: Source::Static,
            },
            Correction {
                heard: "coober net ease".into(),
                canonical: "Kubernetes".into(),
                source: Source::Static,
            },
        ])
    }

    #[test]
    fn exact_multiword_replaces_and_records() {
        let out = dict().apply_exact("build a next jazz app");
        assert_eq!(out.text, "build a Next.js app");
        assert_eq!(
            out.applied,
            vec![AppliedFix {
                from: "next jazz".into(),
                to: "Next.js".into(),
                tier: Tier::Exact,
            }]
        );
    }

    #[test]
    fn longest_phrase_wins_over_shorter() {
        // "next jazz" must win; the bare "next" entry must not pre-empt it.
        let out = dict().apply_exact("next jazz");
        assert_eq!(out.text, "Next.js");
    }

    #[test]
    fn case_insensitive_match_preserves_canonical_casing() {
        let out = dict().apply_exact("Next Jazz rocks");
        assert_eq!(out.text, "Next.js rocks");
    }

    #[test]
    fn word_boundary_safety_no_substring_match() {
        // "nextel" is one token, normalizes to "nextel" != "next": no match.
        let out = dict().apply_exact("my nextel phone");
        assert_eq!(out.text, "my nextel phone");
        assert!(out.applied.is_empty());
    }

    #[test]
    fn trailing_punctuation_preserved_on_match() {
        let out = dict().apply_exact("i love next jazz!");
        assert_eq!(out.text, "i love Next.js!");
    }

    #[test]
    fn no_match_returns_input_unchanged() {
        let out = dict().apply_exact("hello world");
        assert_eq!(out.text, "hello world");
        assert!(out.applied.is_empty());
    }

    #[test]
    fn no_match_preserves_original_whitespace_byte_for_byte() {
        let out = dict().apply_exact("hi\nthere   world  ");
        assert_eq!(out.text, "hi\nthere   world  ");
        assert!(out.applied.is_empty());
    }
}

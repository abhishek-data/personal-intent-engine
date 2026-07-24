//! A small, deterministic phonetic key for single-word fuzzy matching.
//!
//! Not Double Metaphone — it maps consonants to sound classes and drops vowels
//! so different spellings of the same term collide (e.g. "kubernetes" and
//! "coobernetes"). Multi-word garbles are handled by exact seed phrases, not
//! here.

/// Compute the phonetic key for a single word.
pub fn phonetic_key(word: &str) -> String {
    let lower: String = word
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .collect();
    // Digraph normalization before per-char classing.
    let s = lower
        .replace("sch", "sk")
        .replace("tch", "ch")
        .replace("ph", "f")
        .replace("ck", "k")
        .replace("gh", "")
        .replace('x', "ks");

    let mut out = String::new();
    for c in s.chars() {
        let cls = match c {
            'a' | 'e' | 'i' | 'o' | 'u' => continue, // drop vowels
            'c' | 'k' | 'q' => 'k',
            's' | 'z' => 's',
            'f' | 'v' => 'f',
            'g' | 'j' => 'j',
            'd' | 't' => 't',
            'b' | 'p' => 'p',
            'm' | 'n' => 'n',
            other => other, // l, r, w, h, y
        };
        if out.chars().last() != Some(cls) {
            out.push(cls);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spelling_variants_of_same_term_collide() {
        assert_eq!(phonetic_key("kubernetes"), phonetic_key("coobernetes"));
        assert_eq!(phonetic_key("Next"), phonetic_key("next"));
    }

    #[test]
    fn distinct_terms_do_not_collide() {
        assert_ne!(phonetic_key("kubernetes"), phonetic_key("postgres"));
    }

    #[test]
    fn empty_or_symbol_input_is_empty_key() {
        assert_eq!(phonetic_key(""), "");
        assert_eq!(phonetic_key("123"), "");
    }
}

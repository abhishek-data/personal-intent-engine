//! Code-aware post-processing: translate spoken code patterns into syntax.
//! Runs ONLY in code mode (opt-in), after pronunciation correction and before
//! intent extraction, so ordinary dictation is never affected.

/// Built-in spoken->syntax pairs. Ordering here doesn't matter — application
/// sorts by phrase length (longest first) so multi-word phrases win.
pub fn builtin_map() -> Vec<(String, String)> {
    [
        ("console dot log", "console.log("),
        ("dot log", ".log("),
        ("dot map", ".map("),
        ("dot filter", ".filter("),
        ("dot for each", ".forEach("),
        ("dot find", ".find("),
        ("dot push", ".push("),
        ("arrow function", "() => "),
        ("fat arrow", "() => "),
        ("triple equals", "==="),
        ("not strictly equal", "!=="),
        ("double equals", "=="),
        ("not equal", "!="),
        ("open brace", "{"),
        ("close brace", "}"),
        ("open bracket", "["),
        ("close bracket", "]"),
        ("open paren", "("),
        ("close paren", ")"),
        ("semi colon", ";"),
        ("single quote", "'"),
        ("double quote", "\""),
        ("back tick", "`"),
        ("hash tag", "#"),
        ("async function", "async function"),
        ("export default", "export default"),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect()
}

/// Apply the built-in code-phrase map.
#[must_use]
pub fn apply_code_phrases(text: &str) -> String {
    apply_with_map(text, &builtin_map())
}

/// Apply `map` to `text`: longest phrase first (by char count), case-insensitive,
/// replacing ALL occurrences of each phrase. Longest-first ordering ensures a
/// multi-word phrase (e.g. "console dot log") wins over a shorter phrase that
/// is also a suffix of it (e.g. "dot log").
#[must_use]
pub fn apply_with_map(text: &str, map: &[(String, String)]) -> String {
    let mut pairs: Vec<&(String, String)> = map.iter().collect();
    pairs.sort_by_key(|(spoken, _)| std::cmp::Reverse(spoken.chars().count()));

    let mut result = text.to_string();
    for (spoken, syntax) in pairs {
        if spoken.is_empty() {
            continue;
        }
        result = replace_all_ci(&result, spoken, syntax);
    }
    result
}

/// Case-insensitive (ASCII-fold) replace-all of `needle` with `replacement`
/// in `haystack`.
///
/// Implementation note: rather than lowercasing `haystack` into a second
/// `String` and indexing into it with byte offsets from the original (the
/// approach sketched in the design doc), this walks `Vec<char>` built from
/// `haystack.chars()` directly. That sidesteps two failure modes of the
/// byte-slice approach:
///   - **Panics**: `.to_lowercase()` can change a character's UTF-8 byte
///     length (e.g. `İ` -> `i̇`), so a byte index that's a valid boundary in
///     `haystack` need not be one in a separately-lowercased copy once the
///     two strings' byte layouts diverge — slicing it can panic.
///   - **Misalignment**: even without a panic, such a divergence would shift
///     all subsequent byte offsets, silently corrupting matches after the
///     first affected character.
///
/// Comparing per-char (via `eq_ignore_ascii_case`, since the built-in and
/// documented user-supplied phrases are ASCII) makes correctness independent
/// of how any given character lowercases, and the loop always advances `i`
/// by at least one char per iteration, so it terminates on any input,
/// including empty strings and multibyte text.
fn replace_all_ci(haystack: &str, needle: &str, replacement: &str) -> String {
    let needle_chars: Vec<char> = needle.chars().collect();
    if needle_chars.is_empty() {
        return haystack.to_string();
    }
    let hay_chars: Vec<char> = haystack.chars().collect();
    let mut out = String::with_capacity(haystack.len());
    let mut i = 0;
    while i < hay_chars.len() {
        let end = i + needle_chars.len();
        let is_match = end <= hay_chars.len()
            && hay_chars[i..end]
                .iter()
                .zip(needle_chars.iter())
                .all(|(h, n)| h.eq_ignore_ascii_case(n));
        if is_match {
            out.push_str(replacement);
            i = end;
        } else {
            out.push(hay_chars[i]);
            i += 1;
        }
    }
    out
}

/// Parse a user-supplied map from JSON: either an object
/// `{"spoken":"syntax"}` or an array of pairs `[["spoken","syntax"]]`.
/// Phrases should be kept ASCII — matching is case-folded on ASCII only.
pub fn map_from_json(json: &str) -> anyhow::Result<Vec<(String, String)>> {
    use serde_json::Value;
    let v: Value = serde_json::from_str(json)?;
    match v {
        Value::Object(o) => Ok(o
            .into_iter()
            .filter_map(|(k, val)| val.as_str().map(|s| (k, s.to_string())))
            .collect()),
        Value::Array(a) => Ok(a
            .into_iter()
            .filter_map(|pair| {
                let arr = pair.as_array()?;
                let spoken = arr.first()?.as_str()?.to_string();
                let syntax = arr.get(1)?.as_str()?.to_string();
                Some((spoken, syntax))
            })
            .collect()),
        _ => anyhow::bail!("code-phrase map must be a JSON object or array of pairs"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_all_occurrences_longest_first() {
        let map = vec![
            ("console dot log".to_string(), "console.log(".to_string()),
            ("dot log".to_string(), ".log(".to_string()),
            ("triple equals".to_string(), "===".to_string()),
        ];
        // "console dot log" must win over "dot log" (longest first),
        // and BOTH "triple equals" occurrences must be replaced.
        let out = apply_with_map("console dot log then triple equals and triple equals", &map);
        assert_eq!(out, "console.log( then === and ===");
    }

    #[test]
    fn case_insensitive_match() {
        let map = vec![("triple equals".to_string(), "===".to_string())];
        assert_eq!(apply_with_map("Triple Equals", &map), "===");
    }

    #[test]
    fn no_match_returns_input_unchanged() {
        let map = vec![("triple equals".to_string(), "===".to_string())];
        assert_eq!(apply_with_map("hello world", &map), "hello world");
    }

    #[test]
    fn builtin_map_translates_common_patterns() {
        let out = apply_code_phrases("console dot log hello");
        assert!(out.starts_with("console.log("), "got: {out}");
    }

    #[test]
    fn map_from_json_object_and_array() {
        let obj = map_from_json(r#"{"dot map":".map("}"#).unwrap();
        assert_eq!(obj, vec![("dot map".to_string(), ".map(".to_string())]);
        let arr = map_from_json(r#"[["dot map",".map("]]"#).unwrap();
        assert_eq!(arr, vec![("dot map".to_string(), ".map(".to_string())]);
    }

    // Extra coverage beyond the brief's five tests: UTF-8 safety and
    // termination are explicit correctness requirements for this module
    // (arbitrary transcribed text may contain multibyte characters even
    // though the phrase map itself is ASCII), so they get their own tests.

    #[test]
    fn does_not_panic_on_multibyte_haystack() {
        let map = vec![("triple equals".to_string(), "===".to_string())];
        // Includes a char whose lowercase form changes byte length (İ -> i̇),
        // emoji, and CJK, interspersed with a matchable ASCII phrase.
        let input = "İ 日本語 triple equals 😀 café naïve résumé";
        let out = apply_with_map(input, &map);
        assert_eq!(out, "İ 日本語 === 😀 café naïve résumé");
    }

    #[test]
    fn terminates_and_is_unchanged_on_empty_and_non_ascii_only_input() {
        let map = vec![("triple equals".to_string(), "===".to_string())];
        assert_eq!(apply_with_map("", &map), "");
        assert_eq!(apply_with_map("日本語のテキスト", &map), "日本語のテキスト");
    }

    #[test]
    fn empty_needle_in_map_is_skipped_without_hanging() {
        let map = vec![
            (String::new(), "SHOULD_NOT_APPEAR".to_string()),
            ("triple equals".to_string(), "===".to_string()),
        ];
        assert_eq!(apply_with_map("triple equals", &map), "===");
    }
}

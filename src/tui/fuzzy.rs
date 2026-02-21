use nucleo_matcher::pattern::{Atom, AtomKind, CaseMatching, Normalization};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// Returns a list of `(original_index, score)` pairs for items matching the query.
///
/// Results are sorted by score descending (best match first).
/// If `query` is empty, returns all items with score 0.
pub(crate) fn fuzzy_filter(query: &str, items: &[String]) -> Vec<(usize, u16)> {
    if query.is_empty() {
        return items.iter().enumerate().map(|(i, _)| (i, 0)).collect();
    }

    let atom = Atom::new(
        query,
        CaseMatching::Smart,
        Normalization::Smart,
        AtomKind::Fuzzy,
        false,
    );
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut buf = Vec::new();

    let mut results: Vec<(usize, u16)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| {
            let haystack = Utf32Str::new(item, &mut buf);
            atom.score(haystack, &mut matcher).map(|score| (i, score))
        })
        .collect();

    results.sort_by(|a, b| b.1.cmp(&a.1));
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<String> {
        vec![
            "ApexClass".to_string(),
            "ApexTrigger".to_string(),
            "CustomObject".to_string(),
            "ApexComponent".to_string(),
            "LightningComponentBundle".to_string(),
        ]
    }

    #[test]
    fn empty_query_returns_all() {
        let results = fuzzy_filter("", &items());
        assert_eq!(results.len(), 5);
        // All indices present
        let indices: Vec<usize> = results.iter().map(|r| r.0).collect();
        assert_eq!(indices, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn matching_query_returns_subset() {
        let all = items();
        let results = fuzzy_filter("Apex", &all);
        assert!(!results.is_empty());
        // All results should be Apex-related items
        for (idx, _) in &results {
            assert!(all[*idx].contains("Apex") || all[*idx].to_lowercase().contains("apex"));
        }
    }

    #[test]
    fn non_matching_query_returns_empty() {
        let results = fuzzy_filter("zzzzzzz", &items());
        assert!(results.is_empty());
    }

    #[test]
    fn results_sorted_by_score_descending() {
        let results = fuzzy_filter("comp", &items());
        if results.len() > 1 {
            for window in results.windows(2) {
                assert!(window[0].1 >= window[1].1);
            }
        }
    }

    #[test]
    fn fuzzy_matching_works() {
        // "lcb" should match "LightningComponentBundle"
        let all = items();
        let results = fuzzy_filter("lcb", &all);
        let matched_names: Vec<&str> = results.iter().map(|(i, _)| all[*i].as_str()).collect();
        assert!(
            matched_names.contains(&"LightningComponentBundle"),
            "Expected LightningComponentBundle in fuzzy results for 'lcb', got: {matched_names:?}"
        );
    }

    #[test]
    fn empty_items_returns_empty() {
        let results = fuzzy_filter("test", &[]);
        assert!(results.is_empty());
    }
}

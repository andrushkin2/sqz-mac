pub const CAP_ERRORS: usize = 30;
pub const CAP_WARNINGS: usize = 15;
pub const CAP_LIST: usize = 30;

pub fn truncate_lines(lines: &[&str], cap: usize) -> String {
    if lines.len() <= cap {
        return lines.join("\n");
    }
    let mut result: Vec<&str> = lines[..cap].to_vec();
    result.push(&"");
    let omitted = lines.len() - cap;
    let suffix = format!("...+{} more", omitted);
    let mut out = result.join("\n");
    out.push_str(&suffix);
    out
}

pub fn truncate_items(items: &[String], cap: usize) -> String {
    if items.len() <= cap {
        return items.join("\n");
    }
    let visible: Vec<&str> = items[..cap].iter().map(|s| s.as_str()).collect();
    let omitted = items.len() - cap;
    format!("{}\n...+{} more", visible.join("\n"), omitted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_truncation_under_cap() {
        let lines = vec!["a", "b", "c"];
        assert_eq!(truncate_lines(&lines, 5), "a\nb\nc");
    }

    #[test]
    fn truncates_at_cap() {
        let lines = vec!["a", "b", "c", "d", "e"];
        let result = truncate_lines(&lines, 3);
        assert!(result.contains("a\nb\nc"));
        assert!(result.contains("...+2 more"));
    }

    #[test]
    fn truncate_items_works() {
        let items: Vec<String> = (0..10).map(|i| format!("item_{}", i)).collect();
        let result = truncate_items(&items, 3);
        assert!(result.contains("item_0"));
        assert!(result.contains("item_2"));
        assert!(result.contains("...+7 more"));
    }
}

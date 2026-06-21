pub fn format_lint(output: &str) -> String {
    let mut by_rule: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut total = 0;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.contains("error") || trimmed.contains("warning") {
            total += 1;
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if let Some(rule) = parts.last() {
                *by_rule.entry(rule.to_string()).or_insert(0) += 1;
            }
        }
    }

    if total == 0 {
        return "ok: 0 issues".to_string();
    }

    let mut result = Vec::new();
    result.push(format!("{} issues:", total));
    let mut sorted: Vec<_> = by_rule.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));
    for (rule, count) in sorted.iter().take(10) {
        result.push(format!("  {} ({}x)", rule, count));
    }
    if sorted.len() > 10 {
        result.push(format!("  ...+{} more rules", sorted.len() - 10));
    }
    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lint_no_issues() {
        let result = format_lint("All checks passed.\n");
        assert_eq!(result, "ok: 0 issues");
    }

    #[test]
    fn test_lint_with_issues() {
        let output = "  10:5  error  Unexpected var  no-var\n  12:1  warning  Missing return  consistent-return\n";
        let result = format_lint(output);
        assert!(result.contains("2 issues:"));
    }
}

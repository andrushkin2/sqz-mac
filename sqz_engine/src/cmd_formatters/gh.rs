use super::truncate::CAP_LIST;

pub fn format_gh(subcmd: Option<&str>, output: &str) -> Option<String> {
    match subcmd? {
        "pr" => Some(format_gh_pr(output)),
        "issue" => Some(format_gh_issue(output)),
        "run" => Some(format_gh_run(output)),
        _ => None,
    }
}

fn format_gh_pr(output: &str) -> String {
    // gh often outputs JSON — detect and parse
    if output.trim_start().starts_with('[') {
        if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(output) {
            return format_pr_list_json(&arr);
        }
    }
    if output.trim_start().starts_with('{') {
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(output) {
            return format_pr_detail_json(&obj);
        }
    }
    // Table format: keep header + truncate rows
    format_table(output)
}

fn format_gh_issue(output: &str) -> String {
    if output.trim_start().starts_with('[') {
        if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(output) {
            return format_issue_list_json(&arr);
        }
    }
    if output.trim_start().starts_with('{') {
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(output) {
            return format_issue_detail_json(&obj);
        }
    }
    format_table(output)
}

fn format_gh_run(output: &str) -> String {
    if output.trim_start().starts_with('[') {
        if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(output) {
            return format_run_list_json(&arr);
        }
    }
    if output.trim_start().starts_with('{') {
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(output) {
            return format_run_detail_json(&obj);
        }
    }
    format_table(output)
}

fn format_pr_list_json(prs: &[serde_json::Value]) -> String {
    if prs.is_empty() {
        return "0 PRs".to_string();
    }
    let mut result = format!("{} PRs:\n", prs.len());
    for pr in prs.iter().take(CAP_LIST) {
        let number = pr.get("number").and_then(|v| v.as_u64()).unwrap_or(0);
        let title = pr.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let state = pr.get("state").and_then(|v| v.as_str()).unwrap_or("OPEN");
        let author = pr
            .get("author")
            .and_then(|a| a.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        result.push_str(&format!(
            "  #{} {} [{}] @{}\n",
            number, title, state, author
        ));
    }
    if prs.len() > CAP_LIST {
        result.push_str(&format!("  ...+{} more\n", prs.len() - CAP_LIST));
    }
    result.trim().to_string()
}

fn format_pr_detail_json(pr: &serde_json::Value) -> String {
    let number = pr.get("number").and_then(|v| v.as_u64()).unwrap_or(0);
    let title = pr.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let state = pr.get("state").and_then(|v| v.as_str()).unwrap_or("");
    let author = pr
        .get("author")
        .and_then(|a| a.get("login"))
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let additions = pr.get("additions").and_then(|v| v.as_u64()).unwrap_or(0);
    let deletions = pr.get("deletions").and_then(|v| v.as_u64()).unwrap_or(0);
    let mergeable = pr.get("mergeable").and_then(|v| v.as_str()).unwrap_or("?");

    format!(
        "PR #{} {} [{}] @{}\n+{} -{} mergeable:{}",
        number, title, state, author, additions, deletions, mergeable
    )
}

fn format_issue_list_json(issues: &[serde_json::Value]) -> String {
    if issues.is_empty() {
        return "0 issues".to_string();
    }
    let mut result = format!("{} issues:\n", issues.len());
    for issue in issues.iter().take(CAP_LIST) {
        let number = issue.get("number").and_then(|v| v.as_u64()).unwrap_or(0);
        let title = issue.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let state = issue
            .get("state")
            .and_then(|v| v.as_str())
            .unwrap_or("OPEN");
        result.push_str(&format!("  #{} {} [{}]\n", number, title, state));
    }
    if issues.len() > CAP_LIST {
        result.push_str(&format!("  ...+{} more\n", issues.len() - CAP_LIST));
    }
    result.trim().to_string()
}

fn format_issue_detail_json(issue: &serde_json::Value) -> String {
    let number = issue.get("number").and_then(|v| v.as_u64()).unwrap_or(0);
    let title = issue.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let state = issue.get("state").and_then(|v| v.as_str()).unwrap_or("");
    let labels: Vec<&str> = issue
        .get("labels")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|l| l.get("name").and_then(|n| n.as_str()))
                .collect()
        })
        .unwrap_or_default();

    let mut out = format!("Issue #{} {} [{}]", number, title, state);
    if !labels.is_empty() {
        out.push_str(&format!("\nlabels: {}", labels.join(", ")));
    }
    out
}

fn format_run_list_json(runs: &[serde_json::Value]) -> String {
    if runs.is_empty() {
        return "0 workflow runs".to_string();
    }
    let mut result = format!("{} workflow runs:\n", runs.len());
    for run in runs.iter().take(CAP_LIST) {
        let status = run.get("status").and_then(|v| v.as_str()).unwrap_or("?");
        let conclusion = run.get("conclusion").and_then(|v| v.as_str()).unwrap_or("");
        let name = run.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        let branch = run
            .get("headBranch")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let display_status = if conclusion.is_empty() {
            status
        } else {
            conclusion
        };
        result.push_str(&format!("  {} [{}] {}\n", name, display_status, branch));
    }
    if runs.len() > CAP_LIST {
        result.push_str(&format!("  ...+{} more\n", runs.len() - CAP_LIST));
    }
    result.trim().to_string()
}

fn format_run_detail_json(run: &serde_json::Value) -> String {
    let name = run.get("name").and_then(|v| v.as_str()).unwrap_or("?");
    let status = run.get("status").and_then(|v| v.as_str()).unwrap_or("?");
    let conclusion = run.get("conclusion").and_then(|v| v.as_str()).unwrap_or("");
    let branch = run
        .get("headBranch")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let display_status = if conclusion.is_empty() {
        status
    } else {
        conclusion
    };
    format!("Run: {} [{}] branch:{}", name, display_status, branch)
}

fn format_table(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= CAP_LIST {
        return output.to_string();
    }
    let mut result: Vec<&str> = lines[..CAP_LIST].to_vec();
    result.push("");
    let out = result.join("\n");
    format!("{}...+{} more rows", out, lines.len() - CAP_LIST)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pr_list_json() {
        let json = r#"[{"number":123,"title":"Fix bug","state":"OPEN","author":{"login":"user"}}]"#;
        let result = format_gh_pr(json);
        assert!(result.contains("#123"));
        assert!(result.contains("Fix bug"));
        assert!(result.contains("@user"));
    }

    #[test]
    fn test_pr_detail_json() {
        let json = r#"{"number":42,"title":"Add feature","state":"MERGED","author":{"login":"dev"},"additions":50,"deletions":10,"mergeable":"UNKNOWN"}"#;
        let result = format_gh_pr(json);
        assert!(result.contains("PR #42"));
        assert!(result.contains("+50 -10"));
    }

    #[test]
    fn test_run_list_json() {
        let json =
            r#"[{"name":"CI","status":"completed","conclusion":"success","headBranch":"main"}]"#;
        let result = format_gh_run(json);
        assert!(result.contains("CI"));
        assert!(result.contains("success"));
    }

    #[test]
    fn test_table_passthrough_short() {
        let output = "ID\tTITLE\tSTATE\n1\tBug\tOPEN\n";
        assert_eq!(format_table(output), output);
    }
}

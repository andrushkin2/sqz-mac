use super::truncate::{CAP_ERRORS, CAP_WARNINGS};

pub fn format_python(cmd: &str, subcmd: Option<&str>, output: &str) -> Option<String> {
    let base = cmd.split_whitespace().next().unwrap_or("").rsplit('/').next().unwrap_or("");

    match base {
        "pytest" => Some(super::test_output::format_test_failures(output)),
        "python" | "python3" if cmd.contains("pytest") || cmd.contains("-m pytest") => {
            Some(super::test_output::format_test_failures(output))
        }
        "ruff" => Some(format_ruff(subcmd, output)),
        "mypy" => Some(format_mypy(output)),
        "pip" | "pip3" => format_pip(subcmd, output),
        _ => None,
    }
}

fn format_ruff(subcmd: Option<&str>, output: &str) -> String {
    // Try JSON first (ruff check --output-format=json)
    if output.trim_start().starts_with('[') {
        if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(output) {
            return format_ruff_json(&arr);
        }
    }

    let is_format = subcmd == Some("format");
    if is_format {
        return format_ruff_format(output);
    }

    // Human-readable: "file.py:10:5: E501 Line too long"
    let mut by_file: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    let mut total = 0;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }
        // Pattern: "path:line:col: CODE message"
        if let Some(colon1) = trimmed.find(':') {
            let after = &trimmed[colon1 + 1..];
            if after.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                let file = trimmed[..colon1].to_string();
                by_file.entry(file).or_default().push(trimmed.to_string());
                total += 1;
            }
        }
    }

    if total == 0 {
        if output.contains("All checks passed") || output.trim().is_empty() {
            return "ok: 0 issues".to_string();
        }
        return output.to_string();
    }

    let mut result = format!("{} issues in {} files:\n", total, by_file.len());
    let mut shown = 0;
    for (file, issues) in &by_file {
        if shown >= CAP_ERRORS { break; }
        result.push_str(&format!("  {} ({}):\n", file, issues.len()));
        for issue in issues.iter().take(5) {
            result.push_str(&format!("    {}\n", issue));
            shown += 1;
        }
        if issues.len() > 5 {
            result.push_str(&format!("    ...+{} more\n", issues.len() - 5));
        }
    }
    if total > CAP_ERRORS {
        result.push_str(&format!("...+{} more issues\n", total - CAP_ERRORS));
    }
    result.trim().to_string()
}

fn format_ruff_json(diagnostics: &[serde_json::Value]) -> String {
    if diagnostics.is_empty() {
        return "ok: 0 issues".to_string();
    }

    // Group by code
    let mut by_code: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for d in diagnostics {
        let code = d.get("code").and_then(|v| v.as_str()).unwrap_or("?");
        *by_code.entry(code.to_string()).or_insert(0) += 1;
    }

    let mut result = format!("{} issues:\n", diagnostics.len());
    let mut sorted: Vec<_> = by_code.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));
    for (code, count) in sorted.iter().take(CAP_WARNINGS) {
        result.push_str(&format!("  {} ({}x)\n", code, count));
    }
    if sorted.len() > CAP_WARNINGS {
        result.push_str(&format!("  ...+{} more rules\n", sorted.len() - CAP_WARNINGS));
    }
    result.trim().to_string()
}

fn format_ruff_format(output: &str) -> String {
    let changed: Vec<&str> = output.lines()
        .filter(|l| !l.trim().is_empty() && !l.contains("file") && !l.contains("left unchanged"))
        .collect();

    if changed.is_empty() || output.contains("left unchanged") {
        if let Some(line) = output.lines().find(|l| l.contains("file")) {
            return line.trim().to_string();
        }
        return "ok: no changes".to_string();
    }

    format!("{} files reformatted", changed.len())
}

fn format_mypy(output: &str) -> String {
    let mut by_code: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    let mut total = 0;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.contains(": error") || trimmed.contains(": note") {
            total += 1;
            // Extract error code from brackets [code]
            let code = if let Some(bracket_start) = trimmed.rfind('[') {
                if let Some(bracket_end) = trimmed.rfind(']') {
                    trimmed[bracket_start + 1..bracket_end].to_string()
                } else {
                    "other".to_string()
                }
            } else {
                "other".to_string()
            };
            by_code.entry(code).or_default().push(trimmed.to_string());
        }
    }

    // Check for summary line
    if let Some(summary) = output.lines().find(|l| l.contains("Found") && l.contains("error")) {
        if total == 0 {
            return summary.trim().to_string();
        }
    }

    if total == 0 {
        return "ok: no errors".to_string();
    }

    let mut result = format!("mypy: {} errors\n", total);
    let mut sorted: Vec<_> = by_code.iter().collect();
    sorted.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
    for (code, locations) in sorted.iter().take(CAP_WARNINGS) {
        result.push_str(&format!("  [{}] ({}x)\n", code, locations.len()));
        for loc in locations.iter().take(3) {
            result.push_str(&format!("    {}\n", loc));
        }
        if locations.len() > 3 {
            result.push_str(&format!("    ...+{} more\n", locations.len() - 3));
        }
    }
    result.trim().to_string()
}

fn format_pip(subcmd: Option<&str>, output: &str) -> Option<String> {
    match subcmd? {
        "install" => Some(format_pip_install(output)),
        "freeze" | "list" => Some(format_pip_list(output)),
        _ => None,
    }
}

fn format_pip_install(output: &str) -> String {
    // Keep only "Successfully installed" or "Requirement already satisfied" summary
    for line in output.lines().rev() {
        if line.starts_with("Successfully installed") {
            let packages: Vec<&str> = line.strip_prefix("Successfully installed ")
                .unwrap_or(line)
                .split_whitespace()
                .collect();
            if packages.len() > 10 {
                return format!("installed {} packages: {}, ...+{}", packages.len(),
                    packages[..5].join(", "), packages.len() - 5);
            }
            return line.to_string();
        }
    }
    if output.contains("already satisfied") {
        return "ok: already satisfied".to_string();
    }
    "ok".to_string()
}

fn format_pip_list(output: &str) -> String {
    let lines: Vec<&str> = output.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() <= 30 {
        return output.to_string();
    }
    format!("{} packages installed\n{}\n...+{} more",
        lines.len() - 2,  // subtract header lines
        lines[..12].join("\n"),
        lines.len() - 12)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ruff_clean() {
        let result = format_ruff(Some("check"), "All checks passed!\n");
        assert_eq!(result, "ok: 0 issues");
    }

    #[test]
    fn test_ruff_human_readable() {
        let output = "src/main.py:10:5: E501 Line too long (120 > 100)\nsrc/main.py:15:1: F401 Unused import\nsrc/util.py:3:1: E302 Expected 2 blank lines\n";
        let result = format_ruff(Some("check"), output);
        assert!(result.contains("3 issues in 2 files"));
    }

    #[test]
    fn test_ruff_json() {
        let json = r#"[{"code":"E501","message":"Line too long","filename":"a.py","location":{"row":1,"column":1}},{"code":"E501","message":"Line too long","filename":"b.py","location":{"row":2,"column":1}}]"#;
        let result = format_ruff(Some("check"), json);
        assert!(result.contains("2 issues"));
        assert!(result.contains("E501 (2x)"));
    }

    #[test]
    fn test_mypy_clean() {
        let result = format_mypy("Success: no issues found in 10 source files\n");
        assert_eq!(result, "ok: no errors");
    }

    #[test]
    fn test_mypy_errors() {
        let output = "src/main.py:10: error: Incompatible types [assignment]\nsrc/main.py:15: error: Missing return [return]\nFound 2 errors in 1 file\n";
        let result = format_mypy(output);
        assert!(result.contains("mypy: 2 errors"));
        assert!(result.contains("[assignment]"));
    }

    #[test]
    fn test_pip_install() {
        let output = "Collecting requests\nDownloading requests-2.28.0.tar.gz\nSuccessfully installed requests-2.28.0 urllib3-1.26.9\n";
        let result = format_pip_install(output);
        assert!(result.starts_with("Successfully installed"));
    }

    #[test]
    fn test_pip_already_satisfied() {
        let output = "Requirement already satisfied: requests in ./venv/lib\n";
        let result = format_pip_install(output);
        assert_eq!(result, "ok: already satisfied");
    }
}

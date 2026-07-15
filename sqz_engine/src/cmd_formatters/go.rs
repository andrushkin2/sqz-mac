use super::truncate::CAP_ERRORS;

pub fn format_go(subcmd: Option<&str>, output: &str) -> Option<String> {
    match subcmd? {
        "test" => Some(format_go_test(output)),
        "build" => Some(format_go_build(output)),
        "vet" => Some(format_go_vet(output)),
        _ => None,
    }
}

fn format_go_test(output: &str) -> String {
    // Detect JSON event stream (go test -json)
    if output
        .lines()
        .next()
        .map(|l| l.trim_start().starts_with('{'))
        .unwrap_or(false)
    {
        if let Some(result) = parse_go_test_json(output) {
            return result;
        }
    }

    // Human-readable: extract pass/fail summary
    super::test_output::format_test_failures(output)
}

fn parse_go_test_json(output: &str) -> Option<String> {
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut failures: Vec<(String, Vec<String>)> = Vec::new();
    let mut current_failure: Option<(String, Vec<String>)> = None;
    let mut packages_passed = 0usize;
    let mut _packages_failed = 0usize;

    for line in output.lines() {
        let event: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let action = event.get("Action").and_then(|v| v.as_str()).unwrap_or("");
        let test = event.get("Test").and_then(|v| v.as_str());
        let event_output = event.get("Output").and_then(|v| v.as_str());

        match action {
            "pass" => {
                if test.is_some() {
                    passed += 1;
                    // If we were tracking a failure for this test, discard it (it passed on retry?)
                } else {
                    packages_passed += 1;
                }
            }
            "fail" => {
                if let Some(test_name) = test {
                    failed += 1;
                    if let Some((name, lines)) = current_failure.take() {
                        if name == test_name {
                            failures.push((name, lines));
                        } else {
                            failures.push((name, lines));
                            failures.push((test_name.to_string(), Vec::new()));
                        }
                    } else {
                        failures.push((test_name.to_string(), Vec::new()));
                    }
                } else {
                    _packages_failed += 1;
                }
            }
            "skip" => {
                if test.is_some() {
                    skipped += 1;
                }
            }
            "output" => {
                if let (Some(test_name), Some(out)) = (test, event_output) {
                    // Track output for potential failures
                    if let Some((ref name, ref mut lines)) = current_failure {
                        if name == test_name && lines.len() < 20 {
                            lines.push(out.trim_end().to_string());
                        }
                    } else if out.contains("FAIL") || out.contains("Error") || out.contains("panic")
                    {
                        current_failure =
                            Some((test_name.to_string(), vec![out.trim_end().to_string()]));
                    }
                }
            }
            _ => {}
        }
    }

    if passed == 0 && failed == 0 && skipped == 0 {
        return None;
    }

    let total = passed + failed + skipped;

    if failed == 0 {
        let mut result = format!("ok: {} passed", passed);
        if skipped > 0 {
            result.push_str(&format!(", {} skipped", skipped));
        }
        if packages_passed > 1 {
            result.push_str(&format!(" ({} packages)", packages_passed));
        }
        return Some(result);
    }

    let mut result = format!("FAILED: {} passed, {} failed", passed, failed);
    if skipped > 0 {
        result.push_str(&format!(", {} skipped", skipped));
    }
    result.push_str(&format!(" ({} total)\n", total));

    for (name, lines) in failures.iter().take(CAP_ERRORS) {
        result.push_str(&format!("  --- FAIL: {}\n", name));
        for line in lines.iter().take(10) {
            result.push_str(&format!("    {}\n", line));
        }
    }
    if failures.len() > CAP_ERRORS {
        result.push_str(&format!(
            "  ...+{} more failures\n",
            failures.len() - CAP_ERRORS
        ));
    }

    Some(result.trim().to_string())
}

fn format_go_build(output: &str) -> String {
    if output.trim().is_empty() {
        return "ok".to_string();
    }

    let errors: Vec<&str> = output.lines().filter(|l| !l.trim().is_empty()).collect();

    if errors.is_empty() {
        return "ok".to_string();
    }

    // Group by file
    let mut by_file: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for line in &errors {
        // Go errors: "./file.go:10:5: error message"
        if let Some(colon) = line.find(':') {
            let file = line[..colon].to_string();
            by_file.entry(file).or_default().push(line.to_string());
        } else {
            by_file
                .entry("other".to_string())
                .or_default()
                .push(line.to_string());
        }
    }

    let mut result = format!("{} errors:\n", errors.len());
    for (file, errs) in by_file.iter().take(CAP_ERRORS) {
        result.push_str(&format!("  {} ({}):\n", file, errs.len()));
        for e in errs.iter().take(5) {
            result.push_str(&format!("    {}\n", e));
        }
        if errs.len() > 5 {
            result.push_str(&format!("    ...+{} more\n", errs.len() - 5));
        }
    }
    result.trim().to_string()
}

fn format_go_vet(output: &str) -> String {
    if output.trim().is_empty() {
        return "ok: no issues".to_string();
    }

    let issues: Vec<&str> = output
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .collect();

    if issues.is_empty() {
        return "ok: no issues".to_string();
    }

    format!(
        "go vet: {} issues\n{}",
        issues.len(),
        issues
            .iter()
            .take(CAP_ERRORS)
            .copied()
            .collect::<Vec<_>>()
            .join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_go_test_json_all_pass() {
        let output = r#"{"Action":"pass","Test":"TestFoo","Package":"pkg"}
{"Action":"pass","Test":"TestBar","Package":"pkg"}
{"Action":"pass","Package":"pkg"}
"#;
        let result = format_go_test(output);
        assert!(result.contains("ok: 2 passed"));
    }

    #[test]
    fn test_go_test_json_with_failure() {
        let output = r#"{"Action":"pass","Test":"TestFoo","Package":"pkg"}
{"Action":"output","Test":"TestBar","Output":"    bar_test.go:10: expected 1 got 2\n"}
{"Action":"fail","Test":"TestBar","Package":"pkg"}
{"Action":"fail","Package":"pkg"}
"#;
        let result = format_go_test(output);
        assert!(result.contains("FAILED"));
        assert!(result.contains("TestBar"));
    }

    #[test]
    fn test_go_build_clean() {
        assert_eq!(format_go_build(""), "ok");
    }

    #[test]
    fn test_go_build_errors() {
        let output = "./main.go:10:5: undefined: foo\n./main.go:15:3: cannot use x\n";
        let result = format_go_build(output);
        assert!(result.contains("2 errors"));
    }

    #[test]
    fn test_go_vet_clean() {
        assert_eq!(format_go_vet(""), "ok: no issues");
    }
}

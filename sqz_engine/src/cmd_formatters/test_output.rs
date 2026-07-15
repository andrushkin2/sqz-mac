use super::truncate::CAP_ERRORS;

pub fn format_test_failures(output: &str) -> String {
    let mut failures = Vec::new();
    let mut summary_line = String::new();
    let mut in_failure = false;
    let mut failure_buf: Vec<String> = Vec::new();

    for line in output.lines() {
        // Summary lines
        if line.starts_with("test result:") || line.starts_with("Tests:") {
            summary_line = line.to_string();
            continue;
        }

        // Rust: "---- test_name stdout ----" marks failure start
        if line.starts_with("---- ") && line.ends_with(" ----") {
            if !failure_buf.is_empty() {
                failures.push(failure_buf.join("\n"));
                failure_buf.clear();
            }
            in_failure = true;
            failure_buf.push(line.to_string());
            continue;
        }

        // Rust: "failures:" section
        if line == "failures:" {
            in_failure = true;
            continue;
        }

        // go test: "--- FAIL:"
        if line.starts_with("--- FAIL:") {
            if !failure_buf.is_empty() {
                failures.push(failure_buf.join("\n"));
                failure_buf.clear();
            }
            in_failure = true;
            failure_buf.push(line.to_string());
            continue;
        }

        // pytest: "FAILED" in line
        if line.contains("FAILED") && !in_failure {
            failures.push(line.to_string());
            continue;
        }

        // Collect failure details
        if in_failure {
            if line.trim().is_empty() {
                if !failure_buf.is_empty() {
                    failures.push(failure_buf.join("\n"));
                    failure_buf.clear();
                }
                in_failure = false;
            } else {
                failure_buf.push(line.to_string());
            }
        }
    }
    if !failure_buf.is_empty() {
        failures.push(failure_buf.join("\n"));
    }

    // All tests passed
    if failures.is_empty() {
        if !summary_line.is_empty() {
            return summary_line;
        }
        let total = output
            .lines()
            .filter(|l| l.contains("... ok") || l.contains("PASSED") || l.contains("passed"))
            .count();
        if total > 0 {
            return format!("ok: {} tests passed", total);
        }
        return output.to_string();
    }

    // Truncate excessive failures
    let mut result = Vec::new();
    if !summary_line.is_empty() {
        result.push(summary_line);
    }
    result.push(format!("FAILURES ({}):", failures.len()));
    for f in failures.iter().take(CAP_ERRORS) {
        result.push(f.clone());
    }
    if failures.len() > CAP_ERRORS {
        result.push(format!("...+{} more failures", failures.len() - CAP_ERRORS));
    }
    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_pass() {
        let output = "running 15 tests\ntest a ... ok\ntest b ... ok\ntest result: ok. 15 passed; 0 failed; 0 ignored\n";
        let result = format_test_failures(output);
        assert!(result.contains("15 passed"));
        assert!(!result.contains("FAILURES"));
    }

    #[test]
    fn test_with_failure() {
        let output = "running 3 tests\ntest a ... ok\ntest b ... FAILED\ntest c ... ok\n\nfailures:\n\n---- b stdout ----\nassertion failed\n\ntest result: FAILED. 2 passed; 1 failed\n";
        let result = format_test_failures(output);
        assert!(result.contains("FAIL"));
    }

    #[test]
    fn test_go_failure() {
        let output = "--- FAIL: TestFoo (0.00s)\n    foo_test.go:15: expected 1, got 2\nFAIL\n";
        let result = format_test_failures(output);
        assert!(result.contains("FAILURES"));
        assert!(result.contains("TestFoo"));
    }

    #[test]
    fn test_pytest_failure() {
        let output = "FAILED tests/test_foo.py::test_bar - AssertionError\n1 failed, 5 passed\n";
        let result = format_test_failures(output);
        assert!(result.contains("FAILURES"));
        assert!(result.contains("test_bar"));
    }
}

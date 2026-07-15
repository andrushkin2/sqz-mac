//! rake test / rails test — Minitest output.

use super::super::truncate::CAP_WARNINGS;
use super::truncate;

const MAX_RAKE_FAILURES: usize = CAP_WARNINGS;

pub fn format_rake(subcmd: Option<&str>, output: &str) -> Option<String> {
    // Only the test task produces parseable Minitest output. Other rake tasks
    // (db:migrate, assets:precompile, …) fall through to generic compression.
    match subcmd {
        Some("test") => Some(filter_minitest_output(output)),
        _ => None,
    }
}

#[derive(Debug, PartialEq)]
enum MinitestState {
    Header,
    Running,
    Failures,
}

fn filter_minitest_output(output: &str) -> String {
    let mut state = MinitestState::Header;
    let mut failures: Vec<String> = Vec::new();
    let mut current_failure: Vec<String> = Vec::new();
    let mut summary_line = String::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // Summary line, e.g. "8 runs, 9 assertions, 1 failures, 0 errors, 0 skips".
        if (trimmed.contains(" runs,") || trimmed.contains(" tests,"))
            && trimmed.contains(" assertions,")
        {
            summary_line = trimmed.to_string();
            continue;
        }

        if trimmed == "# Running:" || trimmed.starts_with("Started with run options") {
            state = MinitestState::Running;
            continue;
        }
        if trimmed.starts_with("Finished in ") {
            state = MinitestState::Failures;
            continue;
        }

        match state {
            MinitestState::Header | MinitestState::Running => continue,
            MinitestState::Failures => {
                if is_minitest_failure_header(trimmed) {
                    if !current_failure.is_empty() {
                        failures.push(current_failure.join("\n"));
                        current_failure.clear();
                    }
                    current_failure.push(trimmed.to_string());
                } else if trimmed.is_empty() && !current_failure.is_empty() {
                    failures.push(current_failure.join("\n"));
                    current_failure.clear();
                } else if !trimmed.is_empty() {
                    current_failure.push(line.to_string());
                }
            }
        }
    }

    if !current_failure.is_empty() {
        failures.push(current_failure.join("\n"));
    }

    build_minitest_summary(&summary_line, &failures)
}

/// "1) Failure:" or "1) Error:".
fn is_minitest_failure_header(line: &str) -> bool {
    let line = line.trim();
    if let Some(pos) = line.find(')') {
        let prefix = &line[..pos];
        if prefix.is_empty() || !prefix.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
        let rest = line[pos + 1..].trim();
        rest == "Failure:" || rest == "Error:"
    } else {
        false
    }
}

fn build_minitest_summary(summary: &str, failures: &[String]) -> String {
    let (runs, fail_count, error_count, skips) = parse_minitest_summary(summary);

    if runs == 0 && summary.is_empty() {
        return "rake test: no tests ran".to_string();
    }

    if fail_count == 0 && error_count == 0 {
        let mut msg = format!("ok rake test: {} runs, 0 failures", runs);
        if skips > 0 {
            msg.push_str(&format!(", {} skips", skips));
        }
        return msg;
    }

    let mut result = format!(
        "rake test: {} runs, {} failures, {} errors",
        runs, fail_count, error_count
    );
    if skips > 0 {
        result.push_str(&format!(", {} skips", skips));
    }
    result.push('\n');

    if failures.is_empty() {
        return result.trim().to_string();
    }

    result.push('\n');

    for (i, failure) in failures.iter().take(MAX_RAKE_FAILURES).enumerate() {
        let lines: Vec<&str> = failure.lines().collect();
        if let Some(header) = lines.first() {
            result.push_str(&format!("{}. {}\n", i + 1, header.trim()));
        }
        for line in lines.iter().skip(1).take(4) {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                result.push_str(&format!("   {}\n", truncate(trimmed, 120)));
            }
        }
        if i < failures.len().min(MAX_RAKE_FAILURES) - 1 {
            result.push('\n');
        }
    }

    if failures.len() > MAX_RAKE_FAILURES {
        result.push_str(&format!(
            "\n... +{} more failures\n",
            failures.len() - MAX_RAKE_FAILURES
        ));
    }

    result.trim().to_string()
}

/// Returns (runs, failures, errors, skips).
fn parse_minitest_summary(summary: &str) -> (usize, usize, usize, usize) {
    let mut runs = 0;
    let mut failures = 0;
    let mut errors = 0;
    let mut skips = 0;

    for part in summary.split(',') {
        let words: Vec<&str> = part.split_whitespace().collect();
        if words.len() >= 2 {
            if let Ok(n) = words[0].parse::<usize>() {
                match words[1] {
                    "runs" | "run" | "tests" | "test" => runs = n,
                    "failures" | "failure" => failures = n,
                    "errors" | "error" => errors = n,
                    "skips" | "skip" => skips = n,
                    _ => {}
                }
            }
        }
    }

    (runs, failures, errors, skips)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rake_non_test_returns_none() {
        assert!(format_rake(Some("db:migrate"), "anything").is_none());
        assert!(format_rake(None, "anything").is_none());
    }

    #[test]
    fn minitest_all_pass() {
        let output = "Run options: --seed 12345\n\n# Running:\n\n........\n\nFinished in 0.123456s, 64.8 runs/s\n\n8 runs, 9 assertions, 0 failures, 0 errors, 0 skips";
        let r = format_rake(Some("test"), output).unwrap();
        assert!(r.contains("ok rake test"));
        assert!(r.contains("8 runs"));
        assert!(r.contains("0 failures"));
    }

    #[test]
    fn minitest_with_failures() {
        let output = "Run options: --seed 54321\n\n# Running:\n\n..F....\n\nFinished in 0.234567s, 29.8 runs/s\n\n  1) Failure:\nTestSomething#test_that_fails [/path/to/test.rb:15]:\nExpected: true\n  Actual: false\n\n7 runs, 7 assertions, 1 failures, 0 errors, 0 skips";
        let r = format_rake(Some("test"), output).unwrap();
        assert!(r.contains("1 failures"));
        assert!(r.contains("test_that_fails"));
        assert!(r.contains("Expected: true"));
    }

    #[test]
    fn minitest_empty() {
        let r = format_rake(Some("test"), "").unwrap();
        assert!(r.contains("no tests ran"));
    }

    #[test]
    fn minitest_reporters_format() {
        let output = "Started with run options --seed 37764\n\nProgress: |====|\n\nFinished in 5.79938s\n57 tests, 378 assertions, 0 failures, 0 errors, 0 skips";
        let r = format_rake(Some("test"), output).unwrap();
        assert!(r.contains("ok rake test"));
        assert!(r.contains("57 runs"));
    }

    #[test]
    fn minitest_skip() {
        let output = "# Running:\n\n..S..\n\nFinished in 0.1s, 50.0 runs/s\n\n5 runs, 4 assertions, 0 failures, 0 errors, 1 skips";
        let r = format_rake(Some("test"), output).unwrap();
        assert!(r.contains("ok rake test"));
        assert!(r.contains("1 skips"));
    }

    #[test]
    fn parse_minitest_summary_variants() {
        assert_eq!(
            parse_minitest_summary("8 runs, 9 assertions, 0 failures, 0 errors, 0 skips"),
            (8, 0, 0, 0)
        );
        assert_eq!(
            parse_minitest_summary("5 runs, 4 assertions, 1 failures, 1 errors, 2 skips"),
            (5, 1, 1, 2)
        );
        assert_eq!(
            parse_minitest_summary("57 tests, 378 assertions, 0 failures, 0 errors, 0 skips"),
            (57, 0, 0, 0)
        );
    }
}

//! RSpec: JSON formatter (`--format json`) with a documentation/progress text
//! fallback.

use super::truncate;
use serde::Deserialize;

// rspec failures carry full backtraces — show fewer than a generic warning list.
const MAX_RSPEC_FAILURES: usize = 5;

/// Backtrace line from gems/rspec/ruby internals — not user code.
fn is_gem_backtrace(line: &str) -> bool {
    line.contains("/gems/")
        || line.contains("lib/rspec")
        || line.contains("lib/ruby/")
        || line.contains("vendor/bundle")
}

#[derive(Deserialize)]
struct RspecOutput {
    examples: Vec<RspecExample>,
    summary: RspecSummary,
}

#[derive(Deserialize)]
struct RspecExample {
    full_description: String,
    status: String,
    file_path: String,
    line_number: u32,
    exception: Option<RspecException>,
}

#[derive(Deserialize)]
struct RspecException {
    class: String,
    message: String,
    #[serde(default)]
    backtrace: Vec<String>,
}

#[derive(Deserialize)]
struct RspecSummary {
    duration: f64,
    example_count: usize,
    failure_count: usize,
    pending_count: usize,
    #[serde(default)]
    errors_outside_of_examples_count: usize,
}

pub fn format_rspec(output: &str) -> String {
    if output.trim().is_empty() {
        return "RSpec: No output".to_string();
    }

    // Happy path: user passed --format json.
    if let Ok(rspec) = serde_json::from_str::<RspecOutput>(output) {
        return build_rspec_summary(&rspec);
    }

    let stripped = strip_rspec_noise(output);
    if let Ok(rspec) = serde_json::from_str::<RspecOutput>(&stripped) {
        return build_rspec_summary(&rspec);
    }

    filter_rspec_text(&stripped)
}

/// Drop Spring preloader, SimpleCov coverage blocks, DEPRECATION warnings, the
/// "Finished in" timing line, and Capybara screenshot noise (keep path only).
fn strip_rspec_noise(output: &str) -> String {
    let mut result = Vec::new();
    let mut in_simplecov_block = false;

    for line in output.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_lowercase();

        if lower.contains("running via spring preloader") {
            continue;
        }
        if trimmed.starts_with("DEPRECATION WARNING:") {
            continue;
        }
        if trimmed.starts_with("Finished in ") {
            continue;
        }

        let is_simplecov = lower.contains("coverage report")
            || lower.contains("simplecov")
            || lower.contains("coverage/")
            || lower.contains(".simplecov")
            || (lower.contains("all files") && lower.contains("lines"));
        if is_simplecov {
            in_simplecov_block = true;
            continue;
        }
        if in_simplecov_block {
            if trimmed.is_empty() {
                in_simplecov_block = false;
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("saved screenshot to ") {
            result.push(format!("[screenshot: {}]", rest.trim()));
            continue;
        }
        if let Some(idx) = trimmed.find("saved screenshot to ") {
            let path = &trimmed[idx + "saved screenshot to ".len()..];
            result.push(format!("[screenshot: {}]", path.trim()));
            continue;
        }

        result.push(line.to_string());
    }

    result.join("\n")
}

fn build_rspec_summary(rspec: &RspecOutput) -> String {
    let s = &rspec.summary;

    if s.example_count == 0 && s.errors_outside_of_examples_count == 0 {
        return "RSpec: No examples found".to_string();
    }
    if s.example_count == 0 && s.errors_outside_of_examples_count > 0 {
        return format!(
            "RSpec: {} errors outside of examples ({:.2}s)",
            s.errors_outside_of_examples_count, s.duration
        );
    }

    if s.failure_count == 0 && s.errors_outside_of_examples_count == 0 {
        let passed = s.example_count.saturating_sub(s.pending_count);
        let mut result = format!("✓ RSpec: {} passed", passed);
        if s.pending_count > 0 {
            result.push_str(&format!(", {} pending", s.pending_count));
        }
        result.push_str(&format!(" ({:.2}s)", s.duration));
        return result;
    }

    let passed = s
        .example_count
        .saturating_sub(s.failure_count + s.pending_count);
    let mut result = format!("RSpec: {} passed, {} failed", passed, s.failure_count);
    if s.pending_count > 0 {
        result.push_str(&format!(", {} pending", s.pending_count));
    }
    result.push_str(&format!(" ({:.2}s)\n", s.duration));

    let failures: Vec<&RspecExample> = rspec
        .examples
        .iter()
        .filter(|e| e.status == "failed")
        .collect();

    if failures.is_empty() {
        return result.trim().to_string();
    }

    result.push_str("\nFailures:\n");

    for (i, example) in failures.iter().take(MAX_RSPEC_FAILURES).enumerate() {
        result.push_str(&format!(
            "{}. ✗ {}\n   {}:{}\n",
            i + 1,
            example.full_description,
            example.file_path,
            example.line_number
        ));

        if let Some(exc) = &example.exception {
            let short_class = exc.class.rsplit("::").next().unwrap_or(&exc.class);
            let first_msg = exc.message.lines().next().unwrap_or("");
            result.push_str(&format!(
                "   {}: {}\n",
                short_class,
                truncate(first_msg, 120)
            ));

            for bt in &exc.backtrace {
                if !bt.contains("/gems/") && !bt.contains("lib/rspec") {
                    result.push_str(&format!("   {}\n", truncate(bt, 120)));
                    break;
                }
            }
        }

        if i < failures.len().min(MAX_RSPEC_FAILURES) - 1 {
            result.push('\n');
        }
    }

    if failures.len() > MAX_RSPEC_FAILURES {
        result.push_str(&format!(
            "\n... +{} more failures\n",
            failures.len() - MAX_RSPEC_FAILURES
        ));
    }

    result.trim().to_string()
}

/// Is this the rspec summary line, e.g. "9 examples, 2 failures"?
fn is_rspec_summary_line(line: &str) -> bool {
    line.contains("example") && (line.contains("failure") || line.contains("pending"))
}

/// State-machine text parser for documentation/progress format output.
fn filter_rspec_text(output: &str) -> String {
    #[derive(PartialEq)]
    enum State {
        Header,
        Failures,
        FailedExamples,
        Summary,
    }

    let mut state = State::Header;
    let mut failures: Vec<String> = Vec::new();
    let mut current_failure = String::new();
    let mut summary_line = String::new();

    for line in output.lines() {
        let trimmed = line.trim();

        match state {
            State::Header => {
                if trimmed == "Failures:" {
                    state = State::Failures;
                } else if trimmed == "Failed examples:" {
                    state = State::FailedExamples;
                } else if is_rspec_summary_line(trimmed) {
                    summary_line = trimmed.to_string();
                    state = State::Summary;
                }
            }
            State::Failures => {
                if is_numbered_failure(trimmed) {
                    if !current_failure.trim().is_empty() {
                        failures.push(compact_failure_block(&current_failure));
                    }
                    current_failure = trimmed.to_string();
                    current_failure.push('\n');
                } else if trimmed == "Failed examples:" {
                    if !current_failure.trim().is_empty() {
                        failures.push(compact_failure_block(&current_failure));
                    }
                    current_failure.clear();
                    state = State::FailedExamples;
                } else if is_rspec_summary_line(trimmed) {
                    if !current_failure.trim().is_empty() {
                        failures.push(compact_failure_block(&current_failure));
                    }
                    current_failure.clear();
                    summary_line = trimmed.to_string();
                    state = State::Summary;
                } else if !trimmed.is_empty() {
                    if is_gem_backtrace(trimmed) {
                        continue;
                    }
                    current_failure.push_str(trimmed);
                    current_failure.push('\n');
                }
            }
            State::FailedExamples => {
                if is_rspec_summary_line(trimmed) {
                    summary_line = trimmed.to_string();
                    state = State::Summary;
                }
            }
            State::Summary => break,
        }
    }

    if !current_failure.trim().is_empty() && state == State::Failures {
        failures.push(compact_failure_block(&current_failure));
    }

    if !summary_line.is_empty() {
        if failures.is_empty() {
            return format!("RSpec: {}", summary_line);
        }
        let mut result = format!("RSpec: {}\n", summary_line);
        for (i, failure) in failures.iter().take(MAX_RSPEC_FAILURES).enumerate() {
            result.push_str(&format!("{}. ✗ {}\n", i + 1, failure));
            if i < failures.len().min(MAX_RSPEC_FAILURES) - 1 {
                result.push('\n');
            }
        }
        if failures.len() > MAX_RSPEC_FAILURES {
            result.push_str(&format!(
                "\n... +{} more failures\n",
                failures.len() - MAX_RSPEC_FAILURES
            ));
        }
        return result.trim().to_string();
    }

    // No summary found anywhere — scan from the end.
    for line in output.lines().rev() {
        let t = line.trim();
        if is_rspec_summary_line(t) {
            return format!("RSpec: {}", t);
        }
    }

    // Last resort: last 5 non-empty lines.
    let tail: Vec<&str> = output.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = tail.len().saturating_sub(5);
    tail[start..].join("\n")
}

/// "1) User#full_name..." — leading digits then ')'.
fn is_numbered_failure(line: &str) -> bool {
    let trimmed = line.trim();
    if let Some(pos) = trimmed.find(')') {
        let prefix = &trimmed[..pos];
        !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit())
    } else {
        false
    }
}

/// Compact a failure block: keep the message, the spec file:line, drop gem
/// backtrace.
fn compact_failure_block(block: &str) -> String {
    let mut spec_file = String::new();
    let mut kept_lines: Vec<String> = Vec::new();

    for line in block.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("# ./spec/") || t.starts_with("# ./test/") {
            spec_file = t.trim_start_matches("# ").to_string();
        } else if t.starts_with('#') && (t.contains("/gems/") || t.contains("lib/rspec")) {
            continue;
        } else {
            kept_lines.push(t.to_string());
        }
    }

    let mut result = kept_lines.join("\n   ");
    if !spec_file.is_empty() {
        result.push_str(&format!("\n   {}", spec_file));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rspec_with_failures_json() -> &'static str {
        r#"{
          "examples": [
            {"full_description":"User is valid","status":"passed","file_path":"./spec/models/user_spec.rb","line_number":5,"exception":null},
            {"full_description":"User saves to database","status":"failed","file_path":"./spec/models/user_spec.rb","line_number":10,"exception":{
                "class":"RSpec::Expectations::ExpectationNotMetError",
                "message":"expected true but got false",
                "backtrace":[
                  "/usr/local/lib/ruby/gems/3.2.0/gems/rspec-expectations-3.12.0/lib/rspec/expectations/fail_with.rb:37:in `fail_with'",
                  "./spec/models/user_spec.rb:11:in `block (2 levels) in <top (required)>'"
                ]}}
          ],
          "summary": {"duration":0.123,"example_count":2,"failure_count":1,"pending_count":0,"errors_outside_of_examples_count":0}
        }"#
    }

    #[test]
    fn rspec_all_pass() {
        let json = r#"{"examples":[{"full_description":"a","status":"passed","file_path":"./spec/a_spec.rb","line_number":1,"exception":null}],"summary":{"duration":0.01,"example_count":1,"failure_count":0,"pending_count":0,"errors_outside_of_examples_count":0}}"#;
        let r = format_rspec(json);
        assert!(r.starts_with("✓ RSpec:"));
        assert!(r.contains("1 passed"));
    }

    #[test]
    fn rspec_failures_shortens_class_and_filters_gems() {
        let r = format_rspec(rspec_with_failures_json());
        assert!(r.contains("1 passed, 1 failed"));
        assert!(r.contains("✗ User saves to database"));
        assert!(r.contains("user_spec.rb:10"));
        assert!(r.contains("ExpectationNotMetError"));
        assert!(!r.contains("RSpec::Expectations::ExpectationNotMetError"));
        assert!(!r.contains("gems/rspec-expectations"));
        assert!(r.contains("user_spec.rb:11"));
    }

    #[test]
    fn rspec_empty() {
        assert_eq!(format_rspec(""), "RSpec: No output");
    }

    #[test]
    fn rspec_no_examples() {
        let json = r#"{"examples":[],"summary":{"duration":0.001,"example_count":0,"failure_count":0,"pending_count":0,"errors_outside_of_examples_count":0}}"#;
        assert_eq!(format_rspec(json), "RSpec: No examples found");
    }

    #[test]
    fn rspec_errors_outside_examples() {
        let json = r#"{"examples":[],"summary":{"duration":0.01,"example_count":0,"failure_count":0,"pending_count":0,"errors_outside_of_examples_count":1}}"#;
        let r = format_rspec(json);
        assert!(!r.contains("No examples found"));
        assert!(r.contains("errors outside"));
    }

    #[test]
    fn rspec_many_failures_caps_at_five() {
        let mut examples = Vec::new();
        for i in 0..6 {
            examples.push(format!(
                r#"{{"full_description":"t{}","status":"failed","file_path":"./spec/a_spec.rb","line_number":{},"exception":{{"class":"RuntimeError","message":"boom","backtrace":["./spec/a_spec.rb:{}:in `block'"]}}}}"#,
                i, i, i
            ));
        }
        let json = format!(
            r#"{{"examples":[{}],"summary":{{"duration":0.05,"example_count":6,"failure_count":6,"pending_count":0,"errors_outside_of_examples_count":0}}}}"#,
            examples.join(",")
        );
        let r = format_rspec(&json);
        assert!(r.contains("1. ✗"));
        assert!(r.contains("5. ✗"));
        assert!(!r.contains("6. ✗"));
        assert!(r.contains("+1 more"));
    }

    #[test]
    fn rspec_text_fallback() {
        let text = "..F.\n\nFailures:\n\n  1) User is valid\n     Failure/Error: expect(user).to be_valid\n       expected true got false\n     # ./spec/models/user_spec.rb:5\n\n4 examples, 1 failure\n";
        let r = format_rspec(text);
        assert!(r.contains("RSpec:"));
        assert!(r.contains("4 examples, 1 failure"));
        assert!(r.contains("✗"));
        assert!(r.contains("spec/models/user_spec.rb:5"));
    }

    #[test]
    fn rspec_text_strips_spring_and_simplecov() {
        let text = "Running via Spring preloader in process 123\n....\n\nCoverage report generated for RSpec to /app/coverage.\n142 / 200 LOC (71.0%) covered.\n\n5 examples, 0 failures\n";
        let r = format_rspec(text);
        assert!(!r.contains("Spring"));
        assert!(!r.contains("Coverage"));
        assert!(r.contains("5 examples, 0 failures"));
    }

    #[test]
    fn rspec_text_screenshot_kept_as_path() {
        let text = "     saved screenshot to /tmp/capybara/failed.png\n3 examples, 1 failure\n";
        let stripped = strip_rspec_noise(text);
        assert!(stripped.contains("[screenshot:"));
        assert!(stripped.contains("failed.png"));
        assert!(!stripped.contains("saved screenshot to"));
    }

    #[test]
    fn rspec_invalid_json_no_panic() {
        let r = format_rspec("not json at all { broken");
        assert!(!r.is_empty());
    }
}

//! RuboCop: JSON formatter (`--format json`) with a text-summary fallback.

use serde::Deserialize;

#[derive(Deserialize)]
struct RubocopOutput {
    files: Vec<RubocopFile>,
    summary: RubocopSummary,
}

#[derive(Deserialize)]
struct RubocopFile {
    path: String,
    offenses: Vec<RubocopOffense>,
}

#[derive(Deserialize)]
struct RubocopOffense {
    cop_name: String,
    severity: String,
    message: String,
    #[serde(default)]
    correctable: bool,
    location: RubocopLocation,
}

#[derive(Deserialize)]
struct RubocopLocation {
    start_line: usize,
}

#[derive(Deserialize)]
struct RubocopSummary {
    offense_count: usize,
    inspected_file_count: usize,
    #[serde(default)]
    correctable_offense_count: usize,
}

pub fn format_rubocop(output: &str) -> String {
    if output.trim().is_empty() {
        return "RuboCop: No output".to_string();
    }
    if let Ok(rubocop) = serde_json::from_str::<RubocopOutput>(output) {
        return filter_rubocop_json(&rubocop);
    }
    filter_rubocop_text(output)
}

/// Rank severity for ordering: lower = more severe.
fn severity_rank(severity: &str) -> u8 {
    match severity {
        "fatal" | "error" => 0,
        "warning" => 1,
        "convention" | "refactor" | "info" => 2,
        _ => 3,
    }
}

fn filter_rubocop_json(rubocop: &RubocopOutput) -> String {
    let s = &rubocop.summary;

    if s.offense_count == 0 {
        return format!("ok ✓ rubocop ({} files)", s.inspected_file_count);
    }

    let correctable_count = if s.correctable_offense_count > 0 {
        s.correctable_offense_count
    } else {
        rubocop
            .files
            .iter()
            .flat_map(|f| &f.offenses)
            .filter(|o| o.correctable)
            .count()
    };

    let mut result = format!(
        "rubocop: {} offenses ({} files)\n",
        s.offense_count, s.inspected_file_count
    );

    let mut files_with_offenses: Vec<&RubocopFile> = rubocop
        .files
        .iter()
        .filter(|f| !f.offenses.is_empty())
        .collect();

    files_with_offenses.sort_by(|a, b| {
        let a_worst = a
            .offenses
            .iter()
            .map(|o| severity_rank(&o.severity))
            .min()
            .unwrap_or(3);
        let b_worst = b
            .offenses
            .iter()
            .map(|o| severity_rank(&o.severity))
            .min()
            .unwrap_or(3);
        a_worst.cmp(&b_worst).then(a.path.cmp(&b.path))
    });

    let max_files = 10;
    let max_offenses_per_file = 5;

    for file in files_with_offenses.iter().take(max_files) {
        result.push_str(&format!("\n{}\n", compact_ruby_path(&file.path)));

        let mut sorted_offenses: Vec<&RubocopOffense> = file.offenses.iter().collect();
        sorted_offenses.sort_by(|a, b| {
            severity_rank(&a.severity)
                .cmp(&severity_rank(&b.severity))
                .then(a.location.start_line.cmp(&b.location.start_line))
        });

        for offense in sorted_offenses.iter().take(max_offenses_per_file) {
            let first_msg_line = offense.message.lines().next().unwrap_or("");
            result.push_str(&format!(
                "  :{} {} — {}\n",
                offense.location.start_line, offense.cop_name, first_msg_line
            ));
        }
        if sorted_offenses.len() > max_offenses_per_file {
            result.push_str(&format!(
                "  … +{} more\n",
                sorted_offenses.len() - max_offenses_per_file
            ));
        }
    }

    if files_with_offenses.len() > max_files {
        result.push_str(&format!(
            "\n… +{} more files\n",
            files_with_offenses.len() - max_files
        ));
    }

    if correctable_count > 0 {
        result.push_str(&format!(
            "\n({} correctable, run `rubocop -A`)",
            correctable_count
        ));
    }

    result.trim().to_string()
}

fn filter_rubocop_text(output: &str) -> String {
    // Ruby/Bundler load errors first.
    for line in output.lines() {
        let t = line.trim();
        if t.contains("cannot load such file")
            || t.contains("Bundler::GemNotFound")
            || t.contains("Gem::MissingSpecError")
            || t.starts_with("rubocop: command not found")
            || t.starts_with("rubocop: No such file")
        {
            let lines: Vec<&str> = output.trim().lines().take(20).collect();
            let total = output.trim().lines().count();
            if total > 20 {
                return format!(
                    "RuboCop error:\n{}\n... ({} more lines)",
                    lines.join("\n"),
                    total - 20
                );
            }
            return format!("RuboCop error:\n{}", lines.join("\n"));
        }
    }

    for line in output.lines().rev() {
        let t = line.trim();
        if t.contains("inspected") && t.contains("autocorrected") {
            let files = extract_leading_number(t);
            let corrected = extract_autocorrect_count(t);
            if files > 0 && corrected > 0 {
                return format!(
                    "ok ✓ rubocop -A ({} files, {} autocorrected)",
                    files, corrected
                );
            }
            return format!("RuboCop: {}", t);
        }
        if t.contains("inspected") && (t.contains("offense") || t.contains("no offenses")) {
            if t.contains("no offenses") {
                let files = extract_leading_number(t);
                if files > 0 {
                    return format!("ok ✓ rubocop ({} files)", files);
                }
                return "ok ✓ rubocop (no offenses)".to_string();
            }
            return format!("RuboCop: {}", t);
        }
    }

    let tail: Vec<&str> = output.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = tail.len().saturating_sub(5);
    if tail.is_empty() {
        return "RuboCop: No output".to_string();
    }
    format!("RuboCop: {}", tail[start..].join("\n"))
}

fn extract_leading_number(s: &str) -> usize {
    s.split_whitespace()
        .next()
        .and_then(|w| w.parse().ok())
        .unwrap_or(0)
}

fn extract_autocorrect_count(s: &str) -> usize {
    for part in s.split(',').rev() {
        let t = part.trim();
        if t.contains("autocorrected") {
            return extract_leading_number(t);
        }
    }
    0
}

/// Compact a Ruby file path to the nearest Rails-convention directory.
fn compact_ruby_path(path: &str) -> String {
    let path = path.replace('\\', "/");

    for prefix in &[
        "app/models/",
        "app/controllers/",
        "app/views/",
        "app/helpers/",
        "app/services/",
        "app/jobs/",
        "app/mailers/",
        "lib/",
        "spec/",
        "test/",
        "config/",
    ] {
        if let Some(pos) = path.find(prefix) {
            return path[pos..].to_string();
        }
    }

    if let Some(pos) = path.rfind("/app/") {
        return path[pos + 1..].to_string();
    }
    if let Some(pos) = path.rfind('/') {
        return path[pos + 1..].to_string();
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rubocop_with_offenses_json() -> &'static str {
        r#"{
          "files": [
            {"path":"app/models/user.rb","offenses":[
              {"severity":"convention","message":"Trailing whitespace detected.","cop_name":"Layout/TrailingWhitespace","correctable":true,"location":{"start_line":10}},
              {"severity":"warning","message":"Useless assignment to variable - `x`.","cop_name":"Lint/UselessAssignment","correctable":false,"location":{"start_line":25}}
            ]},
            {"path":"app/controllers/users_controller.rb","offenses":[
              {"severity":"error","message":"Syntax error.","cop_name":"Lint/Syntax","correctable":false,"location":{"start_line":30}}
            ]}
          ],
          "summary":{"offense_count":3,"target_file_count":2,"inspected_file_count":20,"correctable_offense_count":1}
        }"#
    }

    #[test]
    fn rubocop_no_offenses() {
        let json = r#"{"files":[],"summary":{"offense_count":0,"target_file_count":0,"inspected_file_count":15}}"#;
        assert_eq!(format_rubocop(json), "ok ✓ rubocop (15 files)");
    }

    #[test]
    fn rubocop_offenses_grouped_and_sorted() {
        let r = format_rubocop(rubocop_with_offenses_json());
        assert!(r.contains("3 offenses (20 files)"));
        // error-severity file sorts before convention/warning file
        let ctrl = r.find("users_controller.rb").unwrap();
        let model = r.find("app/models/user.rb").unwrap();
        assert!(ctrl < model);
        assert!(r.contains(":30 Lint/Syntax — Syntax error"));
        assert!(r.contains("1 correctable"));
    }

    #[test]
    fn rubocop_empty() {
        assert_eq!(format_rubocop(""), "RuboCop: No output");
    }

    #[test]
    fn rubocop_text_no_offenses() {
        let text = "Inspecting 10 files\n..........\n\n10 files inspected, no offenses detected";
        assert_eq!(format_rubocop(text), "ok ✓ rubocop (10 files)");
    }

    #[test]
    fn rubocop_text_autocorrect() {
        let text = "Inspecting 15 files\n...C..CC.......\n\n15 files inspected, 3 offenses detected, 3 offenses autocorrected";
        assert_eq!(
            format_rubocop(text),
            "ok ✓ rubocop -A (15 files, 3 autocorrected)"
        );
    }

    #[test]
    fn rubocop_text_bundler_error() {
        let text = "Bundler::GemNotFound: Could not find gem 'rubocop' in any sources.";
        let r = format_rubocop(text);
        assert!(r.starts_with("RuboCop error:"));
        assert!(r.contains("GemNotFound"));
    }

    #[test]
    fn rubocop_caps_files_at_ten() {
        let mut files = Vec::new();
        for i in 1..=12 {
            files.push(format!(
                r#"{{"path":"app/models/m_{}.rb","offenses":[{{"severity":"convention","message":"msg","cop_name":"Cop/X","correctable":false,"location":{{"start_line":1}}}}]}}"#,
                i
            ));
        }
        let json = format!(
            r#"{{"files":[{}],"summary":{{"offense_count":12,"target_file_count":12,"inspected_file_count":12}}}}"#,
            files.join(",")
        );
        let r = format_rubocop(&json);
        assert!(r.contains("… +2 more files"));
    }

    #[test]
    fn compact_ruby_path_works() {
        assert_eq!(
            compact_ruby_path("/home/user/project/app/models/user.rb"),
            "app/models/user.rb"
        );
        assert_eq!(
            compact_ruby_path("/project/spec/models/user_spec.rb"),
            "spec/models/user_spec.rb"
        );
        assert_eq!(
            compact_ruby_path("lib/tasks/deploy.rake"),
            "lib/tasks/deploy.rake"
        );
    }

    #[test]
    fn severity_rank_ordering() {
        assert!(severity_rank("error") < severity_rank("warning"));
        assert!(severity_rank("warning") < severity_rank("convention"));
    }
}

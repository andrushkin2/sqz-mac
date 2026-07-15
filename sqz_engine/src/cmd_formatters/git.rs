pub fn format_git(subcmd: Option<&str>, output: &str) -> Option<String> {
    match subcmd? {
        "status" => Some(format_git_status(output)),
        "log" => Some(format_git_log(output)),
        "diff" => Some(format_git_diff(output)),
        "show" => Some(format_git_show(output)),
        "stash" => Some(format_git_stash(output)),
        "remote" => Some(format_git_remote(output)),
        "fetch" => Some(format_git_fetch(output)),
        "add" | "commit" | "push" | "pull" | "checkout" | "switch" | "branch" => {
            Some(format_git_short(subcmd.unwrap(), output))
        }
        _ => None,
    }
}

fn format_git_status(output: &str) -> String {
    let mut staged = Vec::new();
    let mut modified = Vec::new();
    let mut untracked = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("new file:") || trimmed.starts_with("modified:") && line.starts_with('\t') {
            staged.push(trimmed.to_string());
        } else if trimmed.starts_with("modified:") || trimmed.starts_with("deleted:") {
            modified.push(trimmed.to_string());
        } else if line.starts_with("\t") && !trimmed.starts_with("(use") {
            if output[..output.find(line).unwrap_or(0)].contains("Untracked files:") {
                untracked.push(trimmed.to_string());
            }
        }
    }

    // Also handle short-format status (git status -s)
    if staged.is_empty() && modified.is_empty() && untracked.is_empty() {
        let mut short_staged = Vec::new();
        let mut short_modified = Vec::new();
        let mut short_untracked = Vec::new();
        for line in output.lines() {
            if line.len() < 3 { continue; }
            let (idx, rest) = (line.get(..2), line.get(3..));
            if let (Some(idx), Some(rest)) = (idx, rest) {
                match idx.trim() {
                    "M" | "A" | "D" | "R" => short_staged.push(format!("{} {}", idx.trim(), rest)),
                    "??" => short_untracked.push(rest.to_string()),
                    _ if idx.contains('M') => short_modified.push(format!("M {}", rest)),
                    _ => {}
                }
            }
        }
        if !short_staged.is_empty() || !short_modified.is_empty() || !short_untracked.is_empty() {
            staged = short_staged;
            modified = short_modified;
            untracked = short_untracked;
        }
    }

    if staged.is_empty() && modified.is_empty() && untracked.is_empty() {
        if output.contains("nothing to commit") {
            return "clean".to_string();
        }
        return output.to_string();
    }

    let mut result = Vec::new();
    if !staged.is_empty() {
        result.push(format!("staged({}): {}", staged.len(), staged.join(", ")));
    }
    if !modified.is_empty() {
        result.push(format!("modified({}): {}", modified.len(), modified.join(", ")));
    }
    if !untracked.is_empty() {
        if untracked.len() > 5 {
            result.push(format!("untracked({}): {}, ...+{}", untracked.len(),
                untracked[..3].join(", "), untracked.len() - 3));
        } else {
            result.push(format!("untracked({}): {}", untracked.len(), untracked.join(", ")));
        }
    }
    result.join("\n")
}

fn format_git_log(output: &str) -> String {
    let mut commits = Vec::new();
    let mut current_hash = String::new();
    let mut current_subject = String::new();

    for line in output.lines() {
        if line.starts_with("commit ") {
            if !current_hash.is_empty() {
                commits.push(format!("{} {}", &current_hash[..current_hash.len().min(7)], current_subject.trim()));
            }
            current_hash = line.strip_prefix("commit ").unwrap_or("").trim().to_string();
            current_subject.clear();
        } else if line.starts_with("Author:") || line.starts_with("Date:") || line.starts_with("Merge:") {
            // Skip
        } else {
            let trimmed = line.trim();
            if !trimmed.is_empty() && current_subject.is_empty() {
                current_subject = trimmed.to_string();
            }
        }
    }
    if !current_hash.is_empty() {
        commits.push(format!("{} {}", &current_hash[..current_hash.len().min(7)], current_subject.trim()));
    }

    if commits.is_empty() {
        return output.to_string();
    }
    commits.join("\n")
}

fn format_git_diff(output: &str) -> String {
    // Build a file summary header
    let mut files: Vec<&str> = Vec::new();
    let mut additions = 0usize;
    let mut deletions = 0usize;

    for line in output.lines() {
        if line.starts_with("diff --git") {
            if let Some(b_path) = line.split(" b/").last() {
                files.push(b_path);
            }
        } else if line.starts_with('+') && !line.starts_with("+++") {
            additions += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            deletions += 1;
        }
    }

    let mut result = Vec::new();
    if !files.is_empty() {
        result.push(format!("{} files +{} -{}", files.len(), additions, deletions));
    }

    let mut context_count = 0;
    for line in output.lines() {
        if line.starts_with("diff --git") || line.starts_with("---") || line.starts_with("+++") {
            result.push(line.to_string());
            context_count = 0;
        } else if line.starts_with("@@") {
            result.push(line.to_string());
            context_count = 0;
        } else if line.starts_with('+') || line.starts_with('-') {
            result.push(line.to_string());
            context_count = 0;
        } else {
            context_count += 1;
            if context_count <= 1 {
                result.push(line.to_string());
            }
        }
    }
    result.join("\n")
}

fn format_git_show(output: &str) -> String {
    // git show is commit info + diff; compress the diff part
    let mut header_lines = Vec::new();
    let mut diff_started = false;
    let mut diff_output = String::new();

    for line in output.lines() {
        if line.starts_with("diff --git") {
            diff_started = true;
        }
        if diff_started {
            diff_output.push_str(line);
            diff_output.push('\n');
        } else {
            let trimmed = line.trim();
            // Keep commit, author (first line only), subject
            if line.starts_with("commit ") {
                header_lines.push(format!("{}", &line[7..line.len().min(14 + 7)]));
            } else if line.starts_with("Author:") {
                // skip
            } else if line.starts_with("Date:") {
                // skip
            } else if !trimmed.is_empty() && !trimmed.starts_with("Merge:") {
                header_lines.push(trimmed.to_string());
            }
        }
    }

    if diff_output.is_empty() {
        return header_lines.join("\n");
    }

    let compressed_diff = format_git_diff(&diff_output);
    format!("{}\n{}", header_lines.join("\n"), compressed_diff)
}

fn format_git_stash(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.is_empty() {
        return "ok: no stashes".to_string();
    }
    // git stash list — compact to count + first few
    if lines.len() > 5 {
        let first_three: Vec<&str> = lines[..3].to_vec();
        format!("{} stashes:\n{}\n...+{} more", lines.len(), first_three.join("\n"), lines.len() - 3)
    } else {
        output.to_string()
    }
}

fn format_git_remote(output: &str) -> String {
    // git remote -v has duplicate lines (fetch/push), deduplicate
    let mut seen = std::collections::BTreeMap::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }
        // "origin  git@github.com:foo/bar.git (fetch)"
        if let Some(name_end) = trimmed.find(|c: char| c.is_whitespace()) {
            let name = &trimmed[..name_end];
            let rest = trimmed[name_end..].trim();
            let url = rest.split_whitespace().next().unwrap_or(rest);
            seen.entry(name.to_string()).or_insert_with(|| url.to_string());
        }
    }
    if seen.is_empty() {
        return output.to_string();
    }
    seen.iter().map(|(name, url)| format!("{}\t{}", name, url)).collect::<Vec<_>>().join("\n")
}

fn format_git_fetch(output: &str) -> String {
    if output.trim().is_empty() {
        return "ok: up-to-date".to_string();
    }
    // Keep lines with branch updates, skip "remote: Counting objects" noise
    let meaningful: Vec<&str> = output.lines().filter(|l| {
        let t = l.trim();
        !t.starts_with("remote: Counting")
            && !t.starts_with("remote: Compressing")
            && !t.starts_with("remote: Total")
            && !t.starts_with("Receiving objects")
            && !t.starts_with("Resolving deltas")
            && !t.is_empty()
    }).collect();
    if meaningful.is_empty() {
        return "ok: up-to-date".to_string();
    }
    meaningful.join("\n")
}

fn format_git_short(subcmd: &str, output: &str) -> String {
    match subcmd {
        "add" => {
            if output.trim().is_empty() { return "ok".to_string(); }
            output.to_string()
        }
        "commit" => {
            for line in output.lines() {
                if line.contains(']') && line.contains('[') {
                    return format!("ok {}", line.trim());
                }
            }
            if output.trim().is_empty() { return "ok".to_string(); }
            output.lines().find(|l| !l.trim().is_empty()).unwrap_or("ok").to_string()
        }
        "push" => {
            for line in output.lines() {
                if line.contains("->") {
                    return format!("ok {}", line.trim());
                }
            }
            "ok".to_string()
        }
        "pull" => {
            let mut files_changed = 0;
            let mut insertions = 0;
            let mut deletions = 0;
            for line in output.lines() {
                if line.contains("files changed") || line.contains("file changed") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    for (i, p) in parts.iter().enumerate() {
                        if *p == "file" || p.starts_with("file") { files_changed = parts.get(i-1).and_then(|n| n.parse().ok()).unwrap_or(0); }
                        if p.starts_with("insertion") { insertions = parts.get(i-1).and_then(|n| n.parse().ok()).unwrap_or(0); }
                        if p.starts_with("deletion") { deletions = parts.get(i-1).and_then(|n| n.parse().ok()).unwrap_or(0); }
                    }
                }
            }
            if files_changed > 0 {
                format!("ok {} files +{} -{}", files_changed, insertions, deletions)
            } else if output.contains("Already up to date") {
                "ok up-to-date".to_string()
            } else {
                "ok".to_string()
            }
        }
        _ => output.lines().take(3).collect::<Vec<_>>().join("\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_status_clean() {
        let output = "On branch main\nnothing to commit, working tree clean\n";
        assert_eq!(format_git_status(output), "clean");
    }

    #[test]
    fn test_git_log_compact() {
        let output = "commit abc1234567890\nAuthor: Test <test@test.com>\nDate:   Mon Apr 13\n\n    feat: Add feature\n\ncommit def5678901234\nAuthor: Test <test@test.com>\nDate:   Sun Apr 12\n\n    fix: Bug fix\n";
        let result = format_git_log(output);
        assert!(result.contains("abc1234"));
        assert!(result.contains("feat: Add feature"));
        assert!(!result.contains("Author:"));
    }

    #[test]
    fn test_git_push_compact() {
        let output = "Enumerating objects: 5, done.\nCounting objects: 100% (5/5), done.\nDelta compression using up to 8 threads\n   abc1234..def5678  main -> main\n";
        let result = format_git_short("push", output);
        assert!(result.starts_with("ok"));
    }

    #[test]
    fn test_git_diff_summary_header() {
        let output = "diff --git a/src/main.rs b/src/main.rs\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,3 +1,4 @@\n fn main() {\n+    println!(\"hello\");\n }\n";
        let result = format_git_diff(output);
        assert!(result.starts_with("1 files +1 -0"));
    }

    #[test]
    fn test_git_stash_many() {
        let mut lines = Vec::new();
        for i in 0..10 {
            lines.push(format!("stash@{{{}}}: WIP on main: abc{} msg{}", i, i, i));
        }
        let output = lines.join("\n");
        let result = format_git_stash(&output);
        assert!(result.contains("10 stashes"));
        assert!(result.contains("...+7 more"));
    }

    #[test]
    fn test_git_remote_dedup() {
        let output = "origin\tgit@github.com:user/repo.git (fetch)\norigin\tgit@github.com:user/repo.git (push)\nupstream\tgit@github.com:org/repo.git (fetch)\nupstream\tgit@github.com:org/repo.git (push)\n";
        let result = format_git_remote(output);
        assert!(result.contains("origin"));
        assert!(result.contains("upstream"));
        // Should only appear once each
        assert_eq!(result.matches("origin").count(), 1);
    }

    #[test]
    fn test_git_fetch_empty() {
        assert_eq!(format_git_fetch(""), "ok: up-to-date");
    }

    /// Documents upstream issue #30: `format_git_status` keys off English
    /// labels ("modified:", "new file:", "Untracked files:", "nothing to
    /// commit"). Localized git output (e.g. `LANG=it_IT`) doesn't match any
    /// of those long-format labels, so the parser falls through to the
    /// `git status -s` short-format heuristic — which misreads arbitrary
    /// bytes at column 0-1 of each localized line as status codes and
    /// garbles the text. This test reproduces the exact corruption (a
    /// line like "Modifiche in stage:" gets mangled into "M ifiche in
    /// stage:") to document the bug this fixture exists to catch.
    ///
    /// The actual fix lives at the hook layer (`tool_hooks.rs` /
    /// `opencode_plugin.rs`): commands are executed with `LC_ALL=C` so this
    /// formatter only ever sees English git output in practice. This test
    /// is a tripwire — if it ever stops failing this way, double-check the
    /// hook-layer fix hasn't silently regressed and someone "fixed" this
    /// formatter to (wrongly) treat it as expected behavior.
    #[test]
    fn test_git_status_localized_output_is_corrupted_without_c_locale_issue_30() {
        // Italian locale git status --long output (modificato: = "modified:")
        let italian_output = "Sul branch main\nModifiche in stage:\n\
            \t(usa \"git restore --staged <file>...\" per rimuovere lo stage)\n\
            \tmodificato:  src/main.rs\n";
        let result = format_git_status(italian_output);
        // Must NOT be misreported as clean — that would be the worst
        // outcome (silently hiding real changes from the agent).
        assert_ne!(
            result, "clean",
            "localized git output must never be misclassified as clean: {result}"
        );
        // Demonstrates the actual corruption: the short-status fallback
        // misparses "Modifiche in stage:" as "M" + "ifiche in stage:".
        // This is the bug issue #30 reports — mitigated by forcing
        // LC_ALL=C before this formatter ever runs, not by this parser.
        assert!(
            result.contains("ifiche in stage"),
            "expected the known short-format misparse corruption, got: {result}"
        );
    }

    #[test]
    fn test_git_fetch_strips_noise() {
        let output = "remote: Counting objects: 5, done.\nremote: Compressing objects: 100% (3/3), done.\nremote: Total 5\nReceiving objects: 100%\nResolving deltas: 100%\nFrom github.com:user/repo\n   abc..def  main -> origin/main\n";
        let result = format_git_fetch(output);
        assert!(!result.contains("Counting objects"));
        assert!(result.contains("main -> origin/main"));
    }
}

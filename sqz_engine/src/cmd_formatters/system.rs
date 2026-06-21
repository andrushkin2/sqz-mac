use super::truncate::CAP_LIST;

pub fn format_ls(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 20 { return output.to_string(); }

    let mut dirs = Vec::new();
    let mut files = Vec::new();

    for line in &lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("total") { continue; }
        if trimmed.starts_with('d') || trimmed.ends_with('/') {
            dirs.push(trimmed.split_whitespace().last().unwrap_or(trimmed).to_string());
        } else {
            files.push(trimmed.split_whitespace().last().unwrap_or(trimmed).to_string());
        }
    }

    let mut result = Vec::new();
    if !dirs.is_empty() {
        result.push(format!("dirs({}): {}", dirs.len(), dirs.join(", ")));
    }
    if files.len() > 10 {
        result.push(format!("files({}): {}, ...+{}", files.len(),
            files[..5].join(", "), files.len() - 5));
    } else if !files.is_empty() {
        result.push(format!("files({}): {}", files.len(), files.join(", ")));
    }
    result.join("\n")
}

pub fn format_find(output: &str) -> String {
    let lines: Vec<&str> = output.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() <= 20 { return output.to_string(); }

    let mut by_dir: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for line in &lines {
        let path = std::path::Path::new(line.trim());
        let parent = path.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        by_dir.entry(parent).or_default().push(name);
    }

    let mut result = Vec::new();
    result.push(format!("{} files found:", lines.len()));
    for (dir, files) in &by_dir {
        if files.len() > 5 {
            result.push(format!("  {}/ ({} files)", dir, files.len()));
        } else {
            result.push(format!("  {}/ {}", dir, files.join(", ")));
        }
    }
    result.join("\n")
}

pub fn format_grep(output: &str) -> String {
    let lines: Vec<&str> = output.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() <= CAP_LIST { return output.to_string(); }

    // Group by file
    let mut by_file: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    let mut total = 0;

    for line in &lines {
        total += 1;
        // grep output: "file:line:content" or "file:content"
        if let Some(colon_pos) = line.find(':') {
            let file = line[..colon_pos].to_string();
            let content = line[colon_pos + 1..].to_string();
            by_file.entry(file).or_default().push(content);
        } else {
            by_file.entry("".to_string()).or_default().push(line.to_string());
        }
    }

    let mut result = format!("{} matches in {} files:\n", total, by_file.len());
    let mut shown = 0;
    for (file, matches) in &by_file {
        if shown >= CAP_LIST { break; }
        if file.is_empty() {
            for m in matches.iter().take(5) {
                result.push_str(&format!("  {}\n", m));
                shown += 1;
            }
        } else {
            result.push_str(&format!("  {} ({} matches):\n", file, matches.len()));
            for m in matches.iter().take(3) {
                result.push_str(&format!("    {}\n", m));
                shown += 1;
            }
            if matches.len() > 3 {
                result.push_str(&format!("    ...+{} more\n", matches.len() - 3));
            }
        }
    }
    if total > CAP_LIST {
        result.push_str(&format!("...+{} more matches\n", total - CAP_LIST));
    }
    result.trim().to_string()
}

pub fn format_tree(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 30 { return output.to_string(); }

    // Filter out noise directories and collapse deep trees
    let noise_dirs = ["node_modules", ".git", "target", "__pycache__",
                      ".next", "dist", "build", ".cache", "vendor",
                      "venv", ".venv", ".tox"];

    let mut result = Vec::new();
    let mut skipped = 0;
    let mut in_noise = false;
    let mut noise_depth = 0;

    for line in &lines {
        let content = line.trim_start_matches(|c: char| c == '│' || c == ' ' || c == '├' || c == '└' || c == '─' || c == '─');
        let depth = line.len() - content.len();

        if noise_dirs.iter().any(|d| content.trim() == *d || content.trim().ends_with(d)) {
            in_noise = true;
            noise_depth = depth;
            result.push(format!("{} [collapsed]", line));
            continue;
        }

        if in_noise && depth > noise_depth {
            skipped += 1;
            continue;
        }
        in_noise = false;

        result.push(line.to_string());
    }

    if skipped > 0 {
        result.push(format!("({} entries collapsed)", skipped));
    }

    // Still too long? Truncate
    if result.len() > 50 {
        let mut truncated: Vec<String> = result[..50].to_vec();
        truncated.push(format!("...+{} more entries", result.len() - 50));
        return truncated.join("\n");
    }

    result.join("\n")
}

pub fn format_curl(output: &str) -> String {
    // Strip progress bar lines (curl uses \r for progress)
    let lines: Vec<&str> = output.lines()
        .filter(|l| {
            let t = l.trim();
            !t.starts_with('%') && !t.contains("Dload") && !t.contains("Upload")
                && !t.contains("Xferd") && !t.is_empty()
        })
        .collect();

    if lines.is_empty() {
        return output.to_string();
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ls_short_passthrough() {
        let output = "file1.rs\nfile2.rs\n";
        assert_eq!(format_ls(output), output);
    }

    #[test]
    fn test_grep_groups_by_file() {
        let mut lines = Vec::new();
        for i in 0..40 {
            lines.push(format!("src/main.rs:{}:let x = {};", i + 1, i));
        }
        let output = lines.join("\n");
        let result = format_grep(&output);
        assert!(result.contains("40 matches"));
        assert!(result.contains("src/main.rs (40 matches)"));
    }

    #[test]
    fn test_grep_short_passthrough() {
        let output = "src/lib.rs:10:fn foo()\n";
        assert_eq!(format_grep(output), output);
    }

    #[test]
    fn test_tree_collapses_noise() {
        let mut lines = vec![
            ".".to_string(),
            "├── src".to_string(),
            "│   └── main.rs".to_string(),
            "├── node_modules".to_string(),
        ];
        // Add many node_modules entries
        for i in 0..50 {
            lines.push(format!("│   ├── package_{}", i));
        }
        lines.push("└── Cargo.toml".to_string());
        let output = lines.join("\n");
        let result = format_tree(&output);
        assert!(result.contains("[collapsed]"));
        assert!(result.contains("entries collapsed"));
        assert!(!result.contains("package_49"));
    }

    #[test]
    fn test_tree_short_passthrough() {
        let output = ".\n├── src\n│   └── main.rs\n└── Cargo.toml\n";
        assert_eq!(format_tree(output), output);
    }

    #[test]
    fn test_curl_strips_progress() {
        let output = "  % Total    % Received % Xferd\n  Dload  Upload   Total\n{\"key\": \"value\"}\n";
        let result = format_curl(output);
        assert_eq!(result, "{\"key\": \"value\"}");
    }
}

pub fn format_tsc(output: &str) -> String {
    let mut by_file: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    let mut error_count = 0;

    for line in output.lines() {
        if line.contains("): error TS") || line.contains("): warning TS") {
            error_count += 1;
            if let Some(paren_pos) = line.find('(') {
                let file = &line[..paren_pos];
                by_file
                    .entry(file.to_string())
                    .or_default()
                    .push(line.to_string());
            } else {
                by_file
                    .entry("unknown".to_string())
                    .or_default()
                    .push(line.to_string());
            }
        }
    }

    if error_count == 0 {
        if output.contains("Found 0 errors") || output.trim().is_empty() {
            return "ok: 0 errors".to_string();
        }
        return output.to_string();
    }

    let mut result = Vec::new();
    result.push(format!(
        "ERRORS: {} in {} files",
        error_count,
        by_file.len()
    ));
    for (file, errors) in &by_file {
        result.push(format!("  {} ({}):", file, errors.len()));
        for e in errors.iter().take(5) {
            result.push(format!("    {}", e));
        }
        if errors.len() > 5 {
            result.push(format!("    ...+{} more", errors.len() - 5));
        }
    }
    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tsc_no_errors() {
        let result = format_tsc("Found 0 errors.\n");
        assert_eq!(result, "ok: 0 errors");
    }
}

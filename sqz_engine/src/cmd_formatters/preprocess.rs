use crate::ansi_strip::strip_ansi;

pub fn clean(output: &str) -> String {
    let stripped = strip_ansi(output);
    strip_progress_lines(&stripped)
}

fn strip_progress_lines(input: &str) -> String {
    let mut result = Vec::new();
    let mut download_count = 0;

    for line in input.lines() {
        if line.contains('\r') {
            if let Some(last) = line.rsplit('\r').find(|s| !s.trim().is_empty()) {
                result.push(last.to_string());
            }
            continue;
        }

        if is_download_noise(line) {
            download_count += 1;
            if download_count <= 5 {
                result.push(line.to_string());
            }
            continue;
        }

        result.push(line.to_string());
    }

    if download_count > 5 {
        result.push(format!("...+{} more downloads", download_count - 5));
    }

    result.join("\n")
}

fn is_download_noise(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("Downloading")
        || trimmed.starts_with("Downloaded")
        || trimmed.starts_with("Fetching")
        || (trimmed.starts_with("Compiling") && !trimmed.contains("error"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_ansi_codes() {
        let input = "\x1b[32mgreen\x1b[0m text";
        assert_eq!(clean(input), "green text");
    }

    #[test]
    fn strips_carriage_return_progress() {
        let input = "Progress: 10%\rProgress: 50%\rProgress: 100%\nDone.";
        let result = clean(input);
        assert!(result.contains("Progress: 100%"));
        assert!(result.contains("Done."));
        assert!(!result.contains("10%"));
    }

    #[test]
    fn caps_download_noise() {
        let mut lines = Vec::new();
        for i in 0..20 {
            lines.push(format!("Downloading crate_{} v1.0.{}", i, i));
        }
        lines.push("Finished dev".to_string());
        let input = lines.join("\n");
        let result = clean(&input);
        assert!(result.contains("Downloading crate_0"));
        assert!(result.contains("...+15 more downloads"));
        assert!(result.contains("Finished dev"));
    }

    #[test]
    fn passthrough_normal_content() {
        let input = "line 1\nline 2\nline 3";
        assert_eq!(clean(input), input);
    }
}

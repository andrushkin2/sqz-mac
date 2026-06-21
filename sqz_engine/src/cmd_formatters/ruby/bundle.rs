//! bundle install / update.

pub fn format_bundle(subcmd: Option<&str>, output: &str) -> Option<String> {
    match subcmd {
        Some("install") | Some("update") => Some(format_bundle_install(output)),
        _ => None,
    }
}

fn format_bundle_install(output: &str) -> String {
    if output.trim().is_empty() {
        return String::new();
    }

    if output.contains("Bundle updated!") {
        return "ok bundle: updated".to_string();
    }
    if output.contains("Bundle complete!") {
        return "ok bundle: complete".to_string();
    }

    // No completion line — likely an error. Keep installs + anything that looks
    // like an error, drop "Using"/resolving noise.
    let kept: Vec<&str> = output
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty()
                && !t.starts_with("Using ")
                && !t.starts_with("Fetching gem metadata")
                && !t.starts_with("Resolving dependencies")
        })
        .collect();

    if kept.is_empty() {
        return "ok bundle".to_string();
    }
    let start = kept.len().saturating_sub(30);
    kept[start..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_complete_short_circuits() {
        let output = "Using bundler 2.5.6\nUsing rake 13.1.0\nBundle complete! 85 Gemfile dependencies, 200 gems now installed.";
        assert_eq!(
            format_bundle(Some("install"), output).unwrap(),
            "ok bundle: complete"
        );
    }

    #[test]
    fn bundle_updated() {
        let output = "Using rake 13.1.0\nInstalling rspec 3.14.0\nBundle updated!";
        assert_eq!(
            format_bundle(Some("update"), output).unwrap(),
            "ok bundle: updated"
        );
    }

    #[test]
    fn bundle_non_install_returns_none() {
        assert!(format_bundle(Some("exec"), "anything").is_none());
    }
}

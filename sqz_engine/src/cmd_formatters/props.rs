use proptest::prelude::*;

use super::format_command;

fn known_commands() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("git status".to_string()),
        Just("git log".to_string()),
        Just("git diff".to_string()),
        Just("git push".to_string()),
        Just("git pull".to_string()),
        Just("git stash".to_string()),
        Just("git fetch".to_string()),
        Just("cargo test".to_string()),
        Just("cargo build".to_string()),
        Just("cargo clippy".to_string()),
        Just("npm install".to_string()),
        Just("npm test".to_string()),
        Just("npm audit".to_string()),
        Just("pnpm install".to_string()),
        Just("pytest".to_string()),
        Just("ruff check".to_string()),
        Just("mypy .".to_string()),
        Just("go test ./...".to_string()),
        Just("go build".to_string()),
        Just("docker ps".to_string()),
        Just("kubectl get pods".to_string()),
        Just("kubectl describe pod nginx".to_string()),
        Just("ls -la".to_string()),
        Just("find . -name *.rs".to_string()),
        Just("grep foo src/".to_string()),
        Just("tree".to_string()),
        Just("curl http://example.com".to_string()),
        Just("gh pr list".to_string()),
        Just("aws s3 ls".to_string()),
        Just("terraform plan".to_string()),
        Just("gradle build".to_string()),
        Just("mvn compile".to_string()),
    ]
}

proptest! {
    #[test]
    fn format_command_never_panics(
        cmd in "[a-z]{1,10}( [a-z-]{1,10}){0,3}",
        output in ".{0,5000}"
    ) {
        let _ = format_command(&cmd, &output);
    }

    #[test]
    fn known_commands_never_panic(
        cmd in known_commands(),
        output in "[ -~\n\t]{0,10000}"
    ) {
        let _ = format_command(&cmd, &output);
    }

    #[test]
    fn format_command_never_expands_known_output(
        cmd in known_commands(),
        output in ".{200,5000}"
    ) {
        if let Some(formatted) = format_command(&cmd, &output) {
            // Allow small expansion from summary headers (e.g., "3 files +10 -5\n")
            // but formatted should not be grossly larger than input
            prop_assert!(
                formatted.len() <= output.len() + 100,
                "Formatter expanded output: {} -> {} bytes (cmd: {})",
                output.len(), formatted.len(), cmd
            );
        }
    }

    #[test]
    fn format_command_deterministic(
        cmd in known_commands(),
        output in ".{0,2000}"
    ) {
        let result1 = format_command(&cmd, &output);
        let result2 = format_command(&cmd, &output);
        prop_assert_eq!(result1, result2);
    }

    #[test]
    fn format_command_handles_binary_garbage(
        cmd in known_commands(),
        output in proptest::collection::vec(0u8..=255, 0..2000)
    ) {
        let output_str = String::from_utf8_lossy(&output).to_string();
        let _ = format_command(&cmd, &output_str);
    }

    #[test]
    fn format_command_handles_long_lines(
        cmd in known_commands(),
        line_content in "[a-z]{1000,5000}",
        line_count in 1usize..20
    ) {
        let output = vec![line_content.as_str(); line_count].join("\n");
        let _ = format_command(&cmd, &output);
    }

    #[test]
    fn format_command_handles_unicode(
        cmd in known_commands(),
        output in "[\\p{L}\\p{N}\\s.:/-]{0,2000}"
    ) {
        let _ = format_command(&cmd, &output);
    }
}

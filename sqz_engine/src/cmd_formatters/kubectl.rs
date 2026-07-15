use super::truncate::CAP_LIST;

pub fn format_kubectl(subcmd: Option<&str>, output: &str) -> Option<String> {
    match subcmd? {
        "get" => Some(format_kubectl_get(output)),
        "describe" => Some(format_kubectl_describe(output)),
        "logs" => Some(format_kubectl_logs(output)),
        "apply" => Some(format_kubectl_apply(output)),
        _ => None,
    }
}

fn format_kubectl_get(output: &str) -> String {
    // If JSON output
    if output.trim_start().starts_with('{') || output.trim_start().starts_with('[') {
        return format_kubectl_json(output);
    }

    let lines: Vec<&str> = output.lines().collect();
    if lines.is_empty() {
        return output.to_string();
    }

    // Collapse multi-space columns to single space
    let mut result = Vec::new();
    for line in &lines {
        let collapsed: String = line.split_whitespace().collect::<Vec<_>>().join(" ");
        result.push(collapsed);
    }

    if result.len() > CAP_LIST + 1 {
        let header = result[0].clone();
        let data_count = result.len() - 1;
        let mut truncated = vec![header];
        truncated.extend(result[1..CAP_LIST + 1].to_vec());
        truncated.push(format!("...+{} more", data_count - CAP_LIST));
        return truncated.join("\n");
    }

    result.join("\n")
}

fn format_kubectl_describe(output: &str) -> String {
    // Describe output is verbose — keep key sections, skip events detail
    let mut result = Vec::new();
    let mut in_events = false;
    let mut event_count = 0;

    for line in output.lines() {
        if line.starts_with("Events:") {
            in_events = true;
            result.push(line.to_string());
            continue;
        }
        if in_events {
            if line.starts_with("  ") || line.starts_with("\t") {
                event_count += 1;
                if event_count <= 5 {
                    result.push(line.to_string());
                }
            } else {
                if event_count > 5 {
                    result.push(format!("  ...+{} more events", event_count - 5));
                }
                in_events = false;
                event_count = 0;
                result.push(line.to_string());
            }
            continue;
        }
        result.push(line.to_string());
    }
    if in_events && event_count > 5 {
        result.push(format!("  ...+{} more events", event_count - 5));
    }

    result.join("\n")
}

fn format_kubectl_logs(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 50 {
        return output.to_string();
    }

    // Keep first 10 and last 20 lines, summarize middle
    let mut result = Vec::new();
    for line in lines.iter().take(10) {
        result.push(line.to_string());
    }
    result.push(format!("... ({} lines omitted) ...", lines.len() - 30));
    for line in lines.iter().skip(lines.len() - 20) {
        result.push(line.to_string());
    }
    result.join("\n")
}

fn format_kubectl_apply(output: &str) -> String {
    let mut created = 0;
    let mut configured = 0;
    let mut unchanged = 0;

    for line in output.lines() {
        if line.contains("created") {
            created += 1;
        } else if line.contains("configured") {
            configured += 1;
        } else if line.contains("unchanged") {
            unchanged += 1;
        }
    }

    let total = created + configured + unchanged;
    if total == 0 {
        return output.to_string();
    }

    let mut parts = Vec::new();
    if created > 0 {
        parts.push(format!("{} created", created));
    }
    if configured > 0 {
        parts.push(format!("{} configured", configured));
    }
    if unchanged > 0 {
        parts.push(format!("{} unchanged", unchanged));
    }

    format!("applied: {}", parts.join(", "))
}

fn format_kubectl_json(output: &str) -> String {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(output) {
        if let Some(items) = val.get("items").and_then(|v| v.as_array()) {
            let kind = val.get("kind").and_then(|v| v.as_str()).unwrap_or("items");
            if items.len() > CAP_LIST {
                return format!("{}: {} items (showing {})", kind, items.len(), CAP_LIST);
            }
            return format!("{}: {} items", kind, items.len());
        }
        // Single resource
        if let (Some(kind), Some(name)) = (
            val.get("kind").and_then(|v| v.as_str()),
            val.get("metadata")
                .and_then(|m| m.get("name"))
                .and_then(|v| v.as_str()),
        ) {
            let status = val
                .get("status")
                .and_then(|s| s.get("phase"))
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            return format!("{}/{} [{}]", kind, name, status);
        }
    }
    output.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kubectl_get_table() {
        let output =
            "NAME    READY   STATUS    RESTARTS   AGE\nnginx   1/1     Running   0          5d\n";
        let result = format_kubectl_get(output);
        assert!(result.contains("NAME READY STATUS"));
        assert!(result.contains("nginx"));
    }

    #[test]
    fn test_kubectl_apply() {
        let output =
            "deployment.apps/web created\nservice/web configured\nconfigmap/env unchanged\n";
        let result = format_kubectl_apply(output);
        assert!(result.contains("1 created"));
        assert!(result.contains("1 configured"));
        assert!(result.contains("1 unchanged"));
    }

    #[test]
    fn test_kubectl_describe_truncates_events() {
        let mut lines = vec!["Name: nginx".to_string(), "Events:".to_string()];
        for i in 0..20 {
            lines.push(format!(
                "  Normal  Scheduled  {}m ago  default-scheduler",
                i
            ));
        }
        let output = lines.join("\n");
        let result = format_kubectl_describe(&output);
        assert!(result.contains("...+15 more events"));
    }

    #[test]
    fn test_kubectl_json_list() {
        let json =
            r#"{"kind":"PodList","items":[{"metadata":{"name":"a"}},{"metadata":{"name":"b"}}]}"#;
        let result = format_kubectl_json(json);
        assert!(result.contains("PodList: 2 items"));
    }
}

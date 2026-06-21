use super::truncate::CAP_LIST;

pub fn format_aws(subcmd: Option<&str>, output: &str) -> Option<String> {
    // AWS CLI almost always outputs JSON (default --output json)
    if output.trim_start().starts_with('{') || output.trim_start().starts_with('[') {
        return Some(format_aws_json(output));
    }

    // Table format — collapse whitespace, truncate rows
    match subcmd? {
        "s3" => Some(format_aws_s3(output)),
        _ => Some(format_table_output(output)),
    }
}

pub fn format_terraform(subcmd: Option<&str>, output: &str) -> Option<String> {
    match subcmd? {
        "plan" => Some(format_tf_plan(output)),
        "apply" => Some(format_tf_apply(output)),
        "init" => Some(format_tf_init(output)),
        _ => None,
    }
}

pub fn format_gcloud(output: &str) -> Option<String> {
    if output.trim_start().starts_with('[') || output.trim_start().starts_with('{') {
        return Some(format_aws_json(output));
    }
    Some(format_table_output(output))
}

fn format_aws_json(output: &str) -> String {
    // Parse and extract key info from common AWS responses
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(output) {
        if let Some(arr) = val.as_array() {
            if arr.len() > CAP_LIST {
                return format!("[{} items, showing first {}]\n{}",
                    arr.len(), CAP_LIST,
                    serde_json::to_string(&arr[..CAP_LIST]).unwrap_or_default());
            }
        }
        // For objects with large nested arrays, summarize
        if let Some(obj) = val.as_object() {
            for (key, value) in obj {
                if let Some(arr) = value.as_array() {
                    if arr.len() > CAP_LIST {
                        return format!("{{\"{}\": [{} items, first {} shown], ...}}",
                            key, arr.len(), CAP_LIST);
                    }
                }
            }
        }
    }
    // JSON is small enough or unparseable — pass through
    output.to_string()
}

fn format_aws_s3(output: &str) -> String {
    let lines: Vec<&str> = output.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() <= CAP_LIST {
        return output.to_string();
    }
    format!("{} objects:\n{}\n...+{} more",
        lines.len(),
        lines[..CAP_LIST].join("\n"),
        lines.len() - CAP_LIST)
}

fn format_tf_plan(output: &str) -> String {
    let mut adds = 0;
    let mut changes = 0;
    let mut destroys = 0;
    let mut resources = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("# ") || trimmed.starts_with("+ ") || trimmed.starts_with("~ ") || trimmed.starts_with("- ") {
            if trimmed.starts_with("# ") {
                resources.push(trimmed.to_string());
            }
        }
        if trimmed.contains("to add") && trimmed.contains("to change") && trimmed.contains("to destroy") {
            // "Plan: 3 to add, 1 to change, 0 to destroy."
            for part in trimmed.split(',') {
                let p = part.trim();
                if p.contains("to add") {
                    adds = extract_leading_number(p);
                } else if p.contains("to change") {
                    changes = extract_leading_number(p);
                } else if p.contains("to destroy") {
                    destroys = extract_leading_number(p);
                }
            }
        }
    }

    if adds == 0 && changes == 0 && destroys == 0 {
        // No plan summary found — look for "No changes"
        if output.contains("No changes") {
            return "ok: no changes".to_string();
        }
        return output.to_string();
    }

    let mut result = format!("terraform plan: +{} ~{} -{}\n", adds, changes, destroys);
    for r in resources.iter().take(CAP_LIST) {
        result.push_str(&format!("  {}\n", r));
    }
    if resources.len() > CAP_LIST {
        result.push_str(&format!("  ...+{} more resources\n", resources.len() - CAP_LIST));
    }
    result.trim().to_string()
}

fn format_tf_apply(output: &str) -> String {
    // Keep the "Apply complete!" summary
    for line in output.lines().rev() {
        if line.contains("Apply complete!") {
            return line.trim().to_string();
        }
    }
    if output.contains("No changes") {
        return "ok: no changes".to_string();
    }
    format_tf_plan(output)
}

fn format_tf_init(output: &str) -> String {
    if output.contains("successfully initialized") || output.contains("has been successfully initialized") {
        return "ok: initialized".to_string();
    }
    // Strip download noise
    let meaningful: Vec<&str> = output.lines().filter(|l| {
        let t = l.trim();
        !t.starts_with("- Downloading")
            && !t.starts_with("- Installing")
            && !t.starts_with("- Finding")
            && !t.is_empty()
    }).collect();
    meaningful.join("\n")
}

fn format_table_output(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= CAP_LIST {
        return output.to_string();
    }
    let header = lines[0];
    let data = &lines[1..];
    format!("{}\n{}\n...+{} more rows",
        header,
        data[..CAP_LIST.min(data.len())].join("\n"),
        data.len().saturating_sub(CAP_LIST))
}

fn extract_leading_number(s: &str) -> usize {
    let num_str: String = s.chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    num_str.parse().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tf_plan_summary() {
        let output = "Terraform will perform the following actions:\n\n# aws_instance.web will be created\n+ resource \"aws_instance\" \"web\" {}\n\nPlan: 1 to add, 0 to change, 0 to destroy.\n";
        let result = format_tf_plan(output);
        assert!(result.contains("+1 ~0 -0"));
        assert!(result.contains("aws_instance.web"));
    }

    #[test]
    fn test_tf_plan_no_changes() {
        let result = format_tf_plan("No changes. Infrastructure is up-to-date.\n");
        assert_eq!(result, "ok: no changes");
    }

    #[test]
    fn test_tf_init() {
        let output = "Initializing the backend...\n- Downloading hashicorp/aws 4.0\n- Installing hashicorp/aws v4.0\n\nTerraform has been successfully initialized!\n";
        let result = format_tf_init(output);
        assert_eq!(result, "ok: initialized");
    }

    #[test]
    fn test_aws_json_large_array() {
        let items: Vec<serde_json::Value> = (0..50).map(|i| serde_json::json!({"id": i})).collect();
        let output = serde_json::to_string(&items).unwrap();
        let result = format_aws_json(&output);
        assert!(result.contains("50 items"));
    }

    #[test]
    fn test_aws_json_small() {
        let output = r#"{"StackId": "arn:aws:cf:us-east-1:123:stack/my-stack"}"#;
        let result = format_aws_json(output);
        assert_eq!(result, output);
    }

    #[test]
    fn test_aws_s3_truncate() {
        let mut lines = Vec::new();
        for i in 0..50 {
            lines.push(format!("2024-01-01 s3://bucket/file_{}.txt", i));
        }
        let output = lines.join("\n");
        let result = format_aws_s3(&output);
        assert!(result.contains("50 objects"));
        assert!(result.contains("...+20 more"));
    }
}

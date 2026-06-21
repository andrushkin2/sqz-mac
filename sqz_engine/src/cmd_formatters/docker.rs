pub fn format_docker(subcmd: Option<&str>, output: &str) -> Option<String> {
    match subcmd? {
        "ps" => Some(format_docker_ps(output)),
        "images" => Some(format_docker_images(output)),
        "logs" => None,
        _ => None,
    }
}

fn format_docker_ps(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.is_empty() { return output.to_string(); }

    let mut result = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            result.push("NAME | IMAGE | STATUS".to_string());
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 5 {
            let name = parts.last().unwrap_or(&"");
            let image = parts.get(1).unwrap_or(&"");
            let status_parts: Vec<&&str> = parts.iter().skip(4).take_while(|p| !p.starts_with("0.0.0.0")).collect();
            let status = status_parts.iter().map(|s| **s).collect::<Vec<_>>().join(" ");
            result.push(format!("{} | {} | {}", name, image, status));
        } else {
            result.push(line.to_string());
        }
    }
    result.join("\n")
}

fn format_docker_images(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.is_empty() { return output.to_string(); }

    let mut result = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            result.push("REPO:TAG | SIZE".to_string());
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 5 {
            let repo = parts[0];
            let tag = parts[1];
            let size = parts.last().unwrap_or(&"");
            result.push(format!("{}:{} | {}", repo, tag, size));
        }
    }
    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_docker_ps_compact() {
        let output = "CONTAINER ID   IMAGE     COMMAND   CREATED   STATUS    PORTS     NAMES\nabc123def456   nginx     \"nginx\"   2h ago    Up 2h     80/tcp    web\n";
        let result = format_docker_ps(output);
        assert!(result.contains("NAME | IMAGE | STATUS"));
        assert!(result.contains("web"));
    }
}

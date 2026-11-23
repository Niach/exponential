//! #EXP-2067 INDEX SCOPE (`fixtures/chat-express.json` PROSE): an agent
//! text's Express blocks (`<a2ui>` … `</a2ui>`) are never indexed, never a
//! hit: the index reads its PURE prose (`plainProseCases`). The core's
//! rule (no regex, no SDK), the same lines `domain::chat_express::
//! plain_prose` drops, so the phones and the web index alike.

fn is_open_line(line: &str) -> Option<&str> {
    let rest = line.trim_start_matches([' ', '\t']).strip_prefix("<a2ui")?;
    if let Some(after) = rest.strip_prefix('>') {
        return Some(after);
    }
    if rest.starts_with([' ', '\t']) {
        let end = rest.find('>')?;
        return Some(&rest[end + 1..]);
    }
    None
}

fn is_close_line(line: &str) -> bool {
    line.trim_matches([' ', '\t']) == "</a2ui>"
}

fn is_fence(line: &str) -> bool {
    let line = line.trim_matches([' ', '\t']);
    let Some(info) = line.strip_prefix("```").or_else(|| line.strip_prefix("~~~")) else { return false };
    info.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// The text without its blocks (the whole text when it has none).
pub fn plain_prose(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    if !lines.iter().any(|line| is_open_line(line).is_some()) {
        return text.to_string();
    }
    let mut keep: Vec<Option<&str>> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some(rest) = is_open_line(lines[i]) else {
            keep.push(Some(lines[i]));
            i += 1;
            continue;
        };
        if matches!(keep.last(), Some(Some(prev)) if is_fence(prev)) {
            keep.pop();
        }
        let mut j = i;
        if !rest.contains("</a2ui>") {
            j = i + 1;
            while j < lines.len() && !is_close_line(lines[j]) {
                j += 1;
            }
        }
        if j + 1 < lines.len() && is_fence(lines[j + 1]) {
            j += 1;
        }
        keep.push(None);
        i = j + 1;
    }
    let mut runs: Vec<String> = Vec::new();
    let mut run: Vec<&str> = Vec::new();
    for line in keep {
        match line {
            None => runs.push(std::mem::take(&mut run).join("\n")),
            Some(line) => run.push(line),
        }
    }
    runs.push(run.join("\n"));
    runs.iter().map(|run| run.trim()).filter(|run| !run.is_empty()).collect::<Vec<_>>().join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPRESS: &str = include_str!("../../../../../packages/domain-contract/fixtures/chat-express.json");

    #[test]
    fn plain_prose_cases() {
        let spec: serde_json::Value = serde_json::from_str(EXPRESS).unwrap();
        for case in spec["plainProseCases"].as_array().unwrap() {
            assert_eq!(plain_prose(case["text"].as_str().unwrap()), case["expected"].as_str().unwrap(), "{}", case["name"]);
        }
        assert_eq!(plain_prose("Wrap it in `<a2ui>` tags."), "Wrap it in `<a2ui>` tags.");
        assert_eq!(plain_prose("a <a2uix> b"), "a <a2uix> b");
    }
}

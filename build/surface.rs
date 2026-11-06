use std::path::Path;

pub fn generated_cfgs(path: impl AsRef<Path>) -> Vec<(&'static str, String)> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };

    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| (cfg_key(line), line.to_owned()))
        .collect()
}

/// `tag.op` lines are operations, `tag/subtag` lines are subtags, and bare
/// lines are tags.
pub fn cfg_key(line: &str) -> &'static str {
    if line.contains('.') {
        "generated_op"
    } else if line.contains('/') {
        "generated_subtag"
    } else {
        "generated_tag"
    }
}

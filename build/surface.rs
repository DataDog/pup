use std::path::Path;

pub fn generated_cfgs(path: impl AsRef<Path>) -> Vec<(&'static str, String)> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };

    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let key = if line.contains('.') {
                "generated_op"
            } else {
                "generated_tag"
            };
            (key, line.to_owned())
        })
        .collect()
}

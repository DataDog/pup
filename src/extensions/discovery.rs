use anyhow::Result;
use std::path::{Path, PathBuf};

use super::install::validate_extension_name;
use super::manifest::Manifest;
use crate::config;

/// Information about an installed extension, for display.
pub struct ExtensionInfo {
    pub name: String,
    pub version: String,
    pub source: String,
    pub description: String,
    pub installed_at: String,
}

/// Return the base extensions directory: config_dir()/extensions/
pub fn extension_dir() -> Option<PathBuf> {
    let dir = config::config_dir()?;
    Some(dir.join("extensions"))
}

/// Platform-appropriate executable name for an extension.
fn extension_executable_name(name: &str) -> String {
    let base = format!("pup-{name}");
    if cfg!(target_os = "windows") {
        format!("{base}.exe")
    } else {
        base
    }
}

/// Look up an installed extension by name. Returns the path to the executable if found.
/// A user-installed extension wins over one bundled with the pup release.
pub fn extension_path(name: &str) -> Option<PathBuf> {
    user_extension_path(name).or_else(|| {
        let exe = std::env::current_exe().ok()?;
        bundled_extension_path(&exe, name)
    })
}

fn user_extension_path(name: &str) -> Option<PathBuf> {
    let dir = extension_dir()?;
    let exe_name = extension_executable_name(name);
    let path = dir.join(format!("pup-{name}")).join(&exe_name);
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

/// Extensions shipped inside the release archive live in
/// `libexec/pup-extensions/` next to the pup binary (tarball layout) or one
/// level up (Homebrew's `bin/` + `libexec/` layout).
fn bundled_extension_path(pup_exe: &Path, name: &str) -> Option<PathBuf> {
    validate_extension_name(name).ok()?;
    let bin_dir = pup_exe.parent()?;
    let exe_name = extension_executable_name(name);
    [Some(bin_dir), bin_dir.parent()]
        .into_iter()
        .flatten()
        .map(|dir| dir.join("libexec").join("pup-extensions").join(&exe_name))
        .find(|path| path.is_file())
}

/// List all installed extensions by scanning the extensions directory.
pub fn list_extensions() -> Result<Vec<ExtensionInfo>> {
    let dir = match extension_dir() {
        Some(d) if d.exists() => d,
        _ => return Ok(Vec::new()),
    };

    let mut extensions = Vec::new();

    let entries = std::fs::read_dir(&dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let manifest_path = path.join("manifest.json");
        if let Ok(manifest) = Manifest::load(&manifest_path) {
            extensions.push(ExtensionInfo {
                name: manifest.name,
                version: manifest.version,
                source: manifest.source,
                description: manifest.description,
                installed_at: manifest.installed_at,
            });
        }
    }

    extensions.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(extensions)
}

/// Build an "EXTENSIONS:" help section string for display in `pup --help`.
/// Returns `None` when no extensions are installed (so the help output stays
/// clean and unchanged).
pub fn build_extensions_help_section() -> Option<String> {
    let exts = list_extensions().ok()?;
    if exts.is_empty() {
        return None;
    }

    // Find the longest extension name so we can align descriptions.
    let max_name_len = exts.iter().map(|e| e.name.len()).max().unwrap_or(0);
    // Pad to at least 12 chars to match clap's default subcommand column width.
    let col_width = max_name_len.max(12);

    let mut section = String::from("EXTENSIONS:");
    for ext in &exts {
        let desc = if !ext.description.is_empty() {
            ext.description.clone()
        } else {
            format!("Extension {}", ext.name)
        };
        section.push_str(&format!(
            "\n  {:width$}  {}",
            ext.name,
            desc,
            width = col_width
        ));
    }
    Some(section)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_dir(suffix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pup-test-discovery-{suffix}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_extension_executable_name() {
        let name = extension_executable_name("hello");
        if cfg!(target_os = "windows") {
            assert_eq!(name, "pup-hello.exe");
        } else {
            assert_eq!(name, "pup-hello");
        }
    }

    #[test]
    fn test_extension_path_found() {
        let dir = make_test_dir("path-found");
        let ext_dir = dir.join("extensions").join("pup-hello");
        std::fs::create_dir_all(&ext_dir).unwrap();
        let exe_name = extension_executable_name("hello");
        let exe_path = ext_dir.join(&exe_name);
        std::fs::write(&exe_path, "#!/bin/bash\necho hello").unwrap();

        // Use PUP_CONFIG_DIR to point discovery at our test dir
        let _guard = crate::test_utils::ENV_LOCK.blocking_lock();
        std::env::set_var("PUP_CONFIG_DIR", &dir);

        let result = extension_path("hello");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), exe_path);

        std::env::remove_var("PUP_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn write_bundled(root: &std::path::Path, name: &str) -> PathBuf {
        let dir = root.join("libexec").join("pup-extensions");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(extension_executable_name(name));
        std::fs::write(&path, "#!/bin/sh\n").unwrap();
        path
    }

    #[test]
    fn test_bundled_extension_next_to_binary() {
        let dir = make_test_dir("bundled-sibling");
        let expected = write_bundled(&dir, "setup");

        let found = bundled_extension_path(&dir.join("pup"), "setup");
        assert_eq!(found, Some(expected));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_bundled_extension_homebrew_layout() {
        let dir = make_test_dir("bundled-brew");
        let expected = write_bundled(&dir, "setup");

        let found = bundled_extension_path(&dir.join("bin").join("pup"), "setup");
        assert_eq!(found, Some(expected));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_bundled_extension_missing() {
        let dir = make_test_dir("bundled-missing");
        write_bundled(&dir, "setup");

        assert_eq!(bundled_extension_path(&dir.join("pup"), "other"), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_bundled_extension_rejects_invalid_names() {
        let dir = make_test_dir("bundled-invalid");
        write_bundled(&dir, "setup");

        for name in ["../setup", "Setup", "", "set/up"] {
            assert_eq!(bundled_extension_path(&dir.join("pup"), name), None);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_bundled_extension_ignores_directories() {
        let dir = make_test_dir("bundled-dir");
        std::fs::create_dir_all(
            dir.join("libexec")
                .join("pup-extensions")
                .join(extension_executable_name("setup")),
        )
        .unwrap();

        assert_eq!(bundled_extension_path(&dir.join("pup"), "setup"), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_extension_path_not_found() {
        let dir = make_test_dir("path-not-found");
        std::fs::create_dir_all(dir.join("extensions")).unwrap();

        let _guard = crate::test_utils::ENV_LOCK.blocking_lock();
        std::env::set_var("PUP_CONFIG_DIR", &dir);

        let result = extension_path("nonexistent");
        assert!(result.is_none());

        std::env::remove_var("PUP_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_list_extensions_empty() {
        let dir = make_test_dir("list-empty");
        std::fs::create_dir_all(dir.join("extensions")).unwrap();

        let _guard = crate::test_utils::ENV_LOCK.blocking_lock();
        std::env::set_var("PUP_CONFIG_DIR", &dir);

        let result = list_extensions().unwrap();
        assert!(result.is_empty());

        std::env::remove_var("PUP_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_list_extensions_with_manifests() {
        let dir = make_test_dir("list-manifests");
        let ext_dir = dir.join("extensions").join("pup-hello");
        std::fs::create_dir_all(&ext_dir).unwrap();

        let manifest = Manifest {
            name: "hello".to_string(),
            version: "1.0.0".to_string(),
            source: "local:/tmp/pup-hello".to_string(),
            source_kind: None,
            source_release_tag: None,
            source_asset: None,
            installed_at: "2026-03-29T00:00:00Z".to_string(),
            binary: "pup-hello".to_string(),
            description: "Hello world".to_string(),
            installed_by_pup: "0.39.0".to_string(),
        };
        manifest.save(&ext_dir.join("manifest.json")).unwrap();

        let _guard = crate::test_utils::ENV_LOCK.blocking_lock();
        std::env::set_var("PUP_CONFIG_DIR", &dir);

        let result = list_extensions().unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "hello");
        assert_eq!(result[0].version, "1.0.0");

        std::env::remove_var("PUP_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_build_help_section_no_extensions() {
        let dir = make_test_dir("help-empty");
        std::fs::create_dir_all(dir.join("extensions")).unwrap();

        let _guard = crate::test_utils::ENV_LOCK.blocking_lock();
        std::env::set_var("PUP_CONFIG_DIR", &dir);

        let result = build_extensions_help_section();
        assert!(result.is_none());

        std::env::remove_var("PUP_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_build_help_section_with_extensions() {
        let dir = make_test_dir("help-with-ext");

        // Create two extensions with manifests.
        for (name, desc) in &[("hello", "Hello world"), ("terraform", "Manage Terraform")] {
            let ext_dir = dir.join("extensions").join(format!("pup-{name}"));
            std::fs::create_dir_all(&ext_dir).unwrap();
            let manifest = Manifest {
                name: name.to_string(),
                version: "1.0.0".to_string(),
                source: format!("local:/tmp/pup-{name}"),
                source_kind: None,
                source_release_tag: None,
                source_asset: None,
                installed_at: "2026-03-29T00:00:00Z".to_string(),
                binary: format!("pup-{name}"),
                description: desc.to_string(),
                installed_by_pup: "0.39.0".to_string(),
            };
            manifest.save(&ext_dir.join("manifest.json")).unwrap();
        }

        let _guard = crate::test_utils::ENV_LOCK.blocking_lock();
        std::env::set_var("PUP_CONFIG_DIR", &dir);

        let result = build_extensions_help_section();
        assert!(result.is_some());
        let section = result.unwrap();
        assert!(section.starts_with("EXTENSIONS:"));
        assert!(section.contains("hello"));
        assert!(section.contains("Hello world"));
        assert!(section.contains("terraform"));
        assert!(section.contains("Manage Terraform"));

        std::env::remove_var("PUP_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_build_help_section_fallback_to_extension_name() {
        let dir = make_test_dir("help-no-desc");
        let ext_dir = dir.join("extensions").join("pup-nodesc");
        std::fs::create_dir_all(&ext_dir).unwrap();
        let manifest = Manifest {
            name: "nodesc".to_string(),
            version: "2.5.0".to_string(),
            source: "local:/tmp/pup-nodesc".to_string(),
            source_kind: None,
            source_release_tag: None,
            source_asset: None,
            installed_at: "2026-03-29T00:00:00Z".to_string(),
            binary: "pup-nodesc".to_string(),
            description: String::new(),
            installed_by_pup: "0.39.0".to_string(),
        };
        manifest.save(&ext_dir.join("manifest.json")).unwrap();

        let _guard = crate::test_utils::ENV_LOCK.blocking_lock();
        std::env::set_var("PUP_CONFIG_DIR", &dir);

        let result = build_extensions_help_section();
        assert!(result.is_some());
        let section = result.unwrap();
        assert!(section.contains("Extension nodesc"));

        std::env::remove_var("PUP_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

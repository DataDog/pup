use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Read};
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use super::discovery::extension_dir;
use super::manifest::Manifest;
use crate::version;

/// GitHub release asset metadata (subset of the GitHub Releases API response).
#[derive(Debug, serde::Deserialize)]
struct GitHubAsset {
    name: String,
    #[serde(default)]
    url: Option<String>,
    browser_download_url: String,
}

/// GitHub release metadata (subset of the GitHub Releases API response).
#[derive(Debug, serde::Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug)]
struct ArchiveDownload {
    release_tag: String,
    version: String,
    asset_name: String,
    bytes: Vec<u8>,
    extensions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GitHubAuthSource {
    Env(&'static str),
    GhActive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GitHubAuthResolution {
    token: Option<String>,
    source: Option<GitHubAuthSource>,
    gh_error: Option<String>,
}

static GITHUB_AUTH: OnceLock<GitHubAuthResolution> = OnceLock::new();

/// Map `std::env::consts::OS` to the asset name convention.
fn platform_os() -> &'static str {
    match std::env::consts::OS {
        "macos" => "darwin",
        "linux" => "linux",
        "windows" => "windows",
        other => other,
    }
}

/// Map `std::env::consts::ARCH` to the asset name convention.
fn platform_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        other => other,
    }
}

/// Map `std::env::consts::OS` to the GoReleaser archive OS convention.
fn archive_platform_os() -> &'static str {
    match std::env::consts::OS {
        "macos" => "Darwin",
        "linux" => "Linux",
        "windows" => "Windows",
        other => other,
    }
}

/// Map `std::env::consts::ARCH` to the GoReleaser archive arch convention.
fn archive_platform_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "arm64",
        other => other,
    }
}

/// Build a reqwest client with a User-Agent header (required by GitHub API).
fn github_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(format!("pup/{}", version::VERSION))
        .build()
        .context("building HTTP client for GitHub API")
}

fn resolve_github_auth_with<EnvLookup, GhToken>(
    env_lookup: EnvLookup,
    gh_token: GhToken,
) -> GitHubAuthResolution
where
    EnvLookup: Fn(&str) -> Option<String>,
    GhToken: Fn() -> Result<String>,
{
    for name in ["GH_TOKEN", "GITHUB_TOKEN", "HOMEBREW_GITHUB_API_TOKEN"] {
        if let Some(token) = env_lookup(name)
            .map(|token| token.trim().to_string())
            .filter(|token| !token.is_empty())
        {
            return GitHubAuthResolution {
                token: Some(token),
                source: Some(GitHubAuthSource::Env(name)),
                gh_error: None,
            };
        }
    }

    match gh_token() {
        Ok(token) => {
            let token = token.trim().to_string();
            if token.is_empty() {
                GitHubAuthResolution {
                    token: None,
                    source: None,
                    gh_error: Some("gh auth token returned an empty token".to_string()),
                }
            } else {
                GitHubAuthResolution {
                    token: Some(token),
                    source: Some(GitHubAuthSource::GhActive),
                    gh_error: None,
                }
            }
        }
        Err(err) => GitHubAuthResolution {
            token: None,
            source: None,
            gh_error: Some(err.to_string()),
        },
    }
}

fn active_gh_token() -> Result<String> {
    let output = Command::new("gh")
        .args(["auth", "token", "--hostname", "github.com"])
        .output()
        .context("running gh auth token --hostname github.com")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        if stderr.is_empty() {
            bail!("gh auth token failed with status {}", output.status);
        }
        bail!("gh auth token failed: {stderr}");
    }

    String::from_utf8(output.stdout).context("gh auth token output was not UTF-8")
}

fn resolve_github_auth() -> GitHubAuthResolution {
    resolve_github_auth_with(|name| std::env::var(name).ok(), active_gh_token)
}

fn github_auth() -> &'static GitHubAuthResolution {
    GITHUB_AUTH.get_or_init(resolve_github_auth)
}

fn github_token() -> Option<&'static str> {
    github_auth().token.as_deref()
}

fn github_auth_status_diagnostic() -> Option<String> {
    let output = Command::new("gh")
        .args([
            "auth",
            "status",
            "--hostname",
            "github.com",
            "--json",
            "hosts",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    github_auth_status_diagnostic_from_json(&stdout)
}

fn github_auth_status_diagnostic_from_json(json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let accounts = value
        .get("hosts")?
        .get("github.com")?
        .as_array()
        .map(Vec::as_slice)?;
    if accounts.is_empty() {
        return Some("No GitHub CLI accounts are authenticated for github.com.".to_string());
    }

    let active_login = accounts
        .iter()
        .find(|account| {
            account
                .get("active")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        })
        .and_then(|account| account.get("login"))
        .and_then(serde_json::Value::as_str);

    if accounts.len() > 1 {
        let active = active_login.unwrap_or("unknown");
        Some(format!(
            "GitHub CLI has multiple accounts configured for github.com. Active account: {active}."
        ))
    } else {
        let active = active_login
            .or_else(|| accounts[0].get("login").and_then(serde_json::Value::as_str))
            .unwrap_or("unknown");
        Some(format!("GitHub CLI active account: {active}."))
    }
}

fn github_access_guidance(
    owner: &str,
    repo: &str,
    auth: &GitHubAuthResolution,
    gh_status: Option<&str>,
) -> String {
    let mut message = match auth.source {
        Some(GitHubAuthSource::Env(name)) => format!(
            "GitHub access failed for {owner}/{repo} using token from {name}.\n\n\
             Check that the token can access this repository."
        ),
        Some(GitHubAuthSource::GhActive) => format!(
            "GitHub access failed for {owner}/{repo} using the active GitHub CLI account.\n\n\
             Check the active account:\n\
               gh auth status --hostname github.com\n\n\
             To switch accounts:\n\
               gh auth switch --hostname github.com\n\n\
             Or provide a token explicitly:\n\
               GH_TOKEN=<token> pup extension install {owner}/{repo} --extension foo"
        ),
        None => format!(
            "GitHub access failed for {owner}/{repo}.\n\n\
             For private repositories, provide a token:\n\
               export GH_TOKEN=<token>\n\n\
             Or authenticate with GitHub CLI:\n\
               gh auth login --hostname github.com --scopes repo"
        ),
    };

    if let Some(gh_error) = &auth.gh_error {
        message.push_str("\n\nGitHub CLI token lookup failed: ");
        message.push_str(gh_error);
    }

    if let Some(gh_status) = gh_status {
        message.push_str("\n\n");
        message.push_str(gh_status);
    }

    message
}

fn github_failure_guidance(owner: &str, repo: &str) -> String {
    let auth = github_auth();
    let gh_status = match auth.source {
        Some(GitHubAuthSource::Env(_)) => None,
        Some(GitHubAuthSource::GhActive) | None => github_auth_status_diagnostic(),
    };
    github_access_guidance(owner, repo, auth, gh_status.as_deref())
}

fn github_api_get(client: &reqwest::Client, url: &str) -> reqwest::RequestBuilder {
    let mut req = client
        .get(url)
        .header("Accept", "application/vnd.github+json");
    if let Some(token) = github_token() {
        req = req.header("Authorization", format!("Bearer {token}"));
    }
    req
}

/// Fetch a GitHub release (latest or by tag).
async fn fetch_github_release(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    tag: Option<&str>,
) -> Result<GitHubRelease> {
    let url = match tag {
        Some(t) => format!("https://api.github.com/repos/{owner}/{repo}/releases/tags/{t}"),
        None => format!("https://api.github.com/repos/{owner}/{repo}/releases/latest"),
    };

    let resp = github_api_get(client, &url)
        .send()
        .await
        .with_context(|| format!("fetching release from {url}"))?;

    let status = resp.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        let guidance = github_failure_guidance(owner, repo);
        match tag {
            Some(t) => bail!(
                "release tag '{t}' not found in {owner}/{repo}. \
                 Check that the tag exists at https://github.com/{owner}/{repo}/releases\n\n\
                 {guidance}"
            ),
            None => bail!(
                "no releases found for {owner}/{repo}. \
                 Check that the repository exists and has at least one release at \
                 https://github.com/{owner}/{repo}/releases\n\n\
                 {guidance}"
            ),
        }
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        let guidance = github_failure_guidance(owner, repo);
        bail!("GitHub API returned {status} for {url}: {body}\n\n{guidance}");
    }

    resp.json::<GitHubRelease>()
        .await
        .with_context(|| format!("parsing release JSON from {url}"))
}

/// Fetch GitHub releases newest-first.
async fn fetch_github_releases(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
) -> Result<Vec<GitHubRelease>> {
    let mut releases = Vec::new();
    let mut page = 1;

    loop {
        let url = format!(
            "https://api.github.com/repos/{owner}/{repo}/releases?per_page=100&page={page}"
        );
        let resp = github_api_get(client, &url)
            .send()
            .await
            .with_context(|| format!("fetching releases from {url}"))?;

        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            let guidance = github_failure_guidance(owner, repo);
            bail!(
                "no releases found for {owner}/{repo}. \
                 Check that the repository exists and is accessible\n\n\
                 {guidance}"
            );
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            let guidance = github_failure_guidance(owner, repo);
            bail!("GitHub API returned {status} for {url}: {body}\n\n{guidance}");
        }

        let mut page_releases = resp
            .json::<Vec<GitHubRelease>>()
            .await
            .with_context(|| format!("parsing release JSON from {url}"))?;
        let count = page_releases.len();
        releases.append(&mut page_releases);
        if count < 100 {
            break;
        }
        page += 1;
    }

    Ok(releases)
}

/// Find the matching asset for the current platform in a release.
fn find_platform_asset<'a>(release: &'a GitHubRelease, ext_name: &str) -> Result<&'a GitHubAsset> {
    let os = platform_os();
    let arch = platform_arch();

    // Expected asset name: pup-<name>-<os>-<arch> (or with .exe on Windows)
    let expected = format!("pup-{ext_name}-{os}-{arch}");
    let expected_exe = format!("{expected}.exe");

    release
        .assets
        .iter()
        .find(|a| a.name == expected || a.name == expected_exe)
        .ok_or_else(|| {
            let available: Vec<&str> = release.assets.iter().map(|a| a.name.as_str()).collect();
            anyhow::anyhow!(
                "no matching asset for platform {os}-{arch} (expected '{expected}'). \
                 Available assets: {}",
                if available.is_empty() {
                    "(none)".to_string()
                } else {
                    available.join(", ")
                }
            )
        })
}

/// Find a platform archive that bundles one or more `pup-*` executables.
fn find_platform_archive_asset<'a>(
    release: &'a GitHubRelease,
    project_name: &str,
) -> Result<&'a GitHubAsset> {
    let version = extract_version(&release.tag_name);
    let os = archive_platform_os();
    let arch = archive_platform_arch();
    let expected_tar = format!("{project_name}_{version}_{os}_{arch}.tar.gz");
    let expected_zip = format!("{project_name}_{version}_{os}_{arch}.zip");

    release
        .assets
        .iter()
        .find(|a| a.name == expected_tar || a.name == expected_zip)
        .ok_or_else(|| {
            let available: Vec<&str> = release.assets.iter().map(|a| a.name.as_str()).collect();
            anyhow::anyhow!(
                "no matching archive asset for platform {os}-{arch} \
                 (expected '{expected_tar}' or '{expected_zip}'). Available assets: {}",
                if available.is_empty() {
                    "(none)".to_string()
                } else {
                    available.join(", ")
                }
            )
        })
}

fn extension_name_from_archive_path(path: &Path) -> Option<String> {
    if path.components().count() != 1 {
        return None;
    }
    let file_name = path.file_name()?.to_str()?;
    let file_name = file_name.strip_suffix(".exe").unwrap_or(file_name);
    let name = file_name.strip_prefix("pup-")?;
    if validate_extension_name(name).is_ok() {
        Some(name.to_string())
    } else {
        None
    }
}

fn extension_archive_member_matches(path: &Path, name: &str) -> bool {
    if path.components().count() != 1 {
        return false;
    }
    let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    file_name == format!("pup-{name}") || file_name == format!("pup-{name}.exe")
}

fn extension_names_from_archive(asset_name: &str, bytes: &[u8]) -> Result<Vec<String>> {
    if asset_name.ends_with(".tar.gz") {
        extension_names_from_tar_gz(bytes)
    } else if asset_name.ends_with(".zip") {
        extension_names_from_zip(bytes)
    } else {
        bail!("unsupported extension archive format: {asset_name}");
    }
}

fn extension_names_from_tar_gz(bytes: &[u8]) -> Result<Vec<String>> {
    let decoder = flate2::read::GzDecoder::new(Cursor::new(bytes));
    let mut archive = tar::Archive::new(decoder);
    let mut names = Vec::new();

    for entry in archive
        .entries()
        .context("reading tar.gz archive entries")?
    {
        let entry = entry.context("reading tar.gz archive entry")?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path().context("reading tar.gz archive path")?;
        if let Some(name) = extension_name_from_archive_path(path.as_ref()) {
            names.push(name);
        }
    }

    names.sort();
    names.dedup();
    Ok(names)
}

fn extension_names_from_zip(bytes: &[u8]) -> Result<Vec<String>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).context("reading zip archive")?;
    let mut names = Vec::new();

    for i in 0..archive.len() {
        let file = archive
            .by_index(i)
            .with_context(|| format!("reading zip archive entry {i}"))?;
        if !file.is_file() {
            continue;
        }
        if let Some(name) = extension_name_from_archive_path(Path::new(file.name())) {
            names.push(name);
        }
    }

    names.sort();
    names.dedup();
    Ok(names)
}

fn extract_extension_from_archive(asset_name: &str, bytes: &[u8], name: &str) -> Result<Vec<u8>> {
    validate_extension_name(name)?;
    if asset_name.ends_with(".tar.gz") {
        extract_extension_from_tar_gz(bytes, name)
    } else if asset_name.ends_with(".zip") {
        extract_extension_from_zip(bytes, name)
    } else {
        bail!("unsupported extension archive format: {asset_name}");
    }
}

fn extract_extension_from_tar_gz(bytes: &[u8], name: &str) -> Result<Vec<u8>> {
    let decoder = flate2::read::GzDecoder::new(Cursor::new(bytes));
    let mut archive = tar::Archive::new(decoder);

    for entry in archive
        .entries()
        .context("reading tar.gz archive entries")?
    {
        let mut entry = entry.context("reading tar.gz archive entry")?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path().context("reading tar.gz archive path")?;
        if extension_archive_member_matches(path.as_ref(), name) {
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .context("reading extension binary from tar.gz archive")?;
            return Ok(bytes);
        }
    }

    bail!("archive does not contain extension 'pup-{name}'")
}

fn extract_extension_from_zip(bytes: &[u8], name: &str) -> Result<Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).context("reading zip archive")?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .with_context(|| format!("reading zip archive entry {i}"))?;
        if !file.is_file() {
            continue;
        }
        if extension_archive_member_matches(Path::new(file.name()), name) {
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .context("reading extension binary from zip archive")?;
            return Ok(bytes);
        }
    }

    bail!("archive does not contain extension 'pup-{name}'")
}

fn selected_archive_extension_names(
    available: &[String],
    extension: Option<&str>,
    all: bool,
) -> Result<Vec<String>> {
    if extension.is_some() && all {
        bail!("choose either --extension or --all, not both");
    }

    let mut available = available.to_vec();
    available.sort();
    available.dedup();

    if all {
        if available.is_empty() {
            bail!("release archive does not contain any pup extensions");
        }
        return Ok(available);
    }

    if let Some(extension) = extension {
        validate_extension_name(extension)?;
        if available.iter().any(|name| name == extension) {
            return Ok(vec![extension.to_string()]);
        }
        bail!(
            "release archive does not contain extension '{extension}'. Available extensions: {}",
            if available.is_empty() {
                "(none)".to_string()
            } else {
                available.join(", ")
            }
        );
    }

    match available.as_slice() {
        [] => bail!("release archive does not contain any pup extensions"),
        [name] => Ok(vec![name.clone()]),
        _ => bail!(
            "release archive contains multiple extensions: {}.\n\
             Install one with: pup extension install <owner/repo> --extension <name>\n\
             Install all with: pup extension install <owner/repo> --all",
            available.join(", ")
        ),
    }
}

/// Download a release asset. Authenticated GitHub API asset URLs are used when
/// a GitHub token is available; public browser URLs remain the fallback.
async fn download_asset(client: &reqwest::Client, asset: &GitHubAsset) -> Result<Vec<u8>> {
    let token = github_token();
    let use_api_asset = token.is_some() && asset.url.is_some();
    let url = if use_api_asset {
        asset.url.as_deref().unwrap()
    } else {
        &asset.browser_download_url
    };

    let mut req = client.get(url);
    if use_api_asset {
        req = req.header("Accept", "application/octet-stream");
        if let Some(token) = token {
            req = req.header("Authorization", format!("Bearer {token}"));
        }
    }

    let resp = req
        .send()
        .await
        .with_context(|| format!("downloading asset from {url}"))?;

    let status = resp.status();
    if !status.is_success() {
        bail!("download failed with HTTP {status} for {url}");
    }

    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .with_context(|| format!("reading asset bytes from {url}"))
}

/// Validate that a string contains only characters allowed in GitHub usernames/repo names.
/// GitHub allows `[a-zA-Z0-9._-]` for both owners and repos.
fn is_valid_github_name(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

/// Validate that a GitHub release tag contains only safe characters.
/// Tags generally allow `[a-zA-Z0-9._-/+]`.
fn is_valid_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.chars().all(|c| {
            c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' || c == '/' || c == '+'
        })
}

/// Parse an "owner/repo" string into (owner, repo).
pub fn parse_owner_repo(source: &str) -> Result<(&str, &str)> {
    let parts: Vec<&str> = source.splitn(2, '/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        bail!(
            "invalid GitHub source '{source}': expected format 'owner/repo' \
             (e.g., 'jkirsteins/pup-hello')"
        );
    }
    if !is_valid_github_name(parts[0]) {
        bail!(
            "invalid GitHub owner '{owner}': only alphanumeric characters, hyphens, \
             underscores, and dots are allowed",
            owner = parts[0]
        );
    }
    if !is_valid_github_name(parts[1]) {
        bail!(
            "invalid GitHub repo '{repo}': only alphanumeric characters, hyphens, \
             underscores, and dots are allowed",
            repo = parts[1]
        );
    }
    Ok((parts[0], parts[1]))
}

/// Derive the extension name from a GitHub repo name.
/// Strips the "pup-" prefix if present.
pub fn derive_name_from_repo(repo: &str) -> String {
    repo.strip_prefix("pup-").unwrap_or(repo).to_string()
}

/// Prepare (create or recreate) an extension directory.
fn prepare_extension_dir(ext_dir: &Path) -> Result<()> {
    if ext_dir.exists() {
        std::fs::remove_dir_all(ext_dir).with_context(|| {
            format!(
                "removing existing extension directory: {}",
                ext_dir.display()
            )
        })?;
    }
    std::fs::create_dir_all(ext_dir).with_context(|| format!("creating {}", ext_dir.display()))?;
    Ok(())
}

/// Write a binary to the extension directory and set executable permissions.
/// Returns the executable filename (e.g., "pup-hello" or "pup-hello.exe").
fn write_extension_binary(ext_dir: &Path, name: &str, bytes: &[u8]) -> Result<String> {
    let exe_name = if cfg!(target_os = "windows") {
        format!("pup-{name}.exe")
    } else {
        format!("pup-{name}")
    };
    let dest = ext_dir.join(&exe_name);

    std::fs::write(&dest, bytes)
        .with_context(|| format!("writing binary to {}", dest.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&dest, perms)
            .with_context(|| format!("setting permissions on {}", dest.display()))?;
    }

    Ok(exe_name)
}

struct ExtensionPayload {
    name: String,
    bytes: Vec<u8>,
}

struct GitHubInstallArtifacts {
    version: String,
    source_kind: Option<String>,
    source_release_tag: Option<String>,
    source_asset: Option<String>,
    payloads: Vec<ExtensionPayload>,
}

fn archive_extension_payloads(
    archive: &ArchiveDownload,
    names: &[String],
) -> Result<Vec<ExtensionPayload>> {
    names
        .iter()
        .map(|name| {
            let bytes = extract_extension_from_archive(&archive.asset_name, &archive.bytes, name)?;
            Ok(ExtensionPayload {
                name: name.clone(),
                bytes,
            })
        })
        .collect()
}

fn save_github_payloads(
    source: &str,
    artifacts: GitHubInstallArtifacts,
    force: bool,
    description: Option<&str>,
) -> Result<Vec<String>> {
    if artifacts.payloads.is_empty() {
        bail!("no extensions selected to install");
    }
    if artifacts.payloads.len() > 1 && description.is_some() {
        bail!("--description can only be used when installing one extension");
    }

    for payload in &artifacts.payloads {
        validate_extension_name(&payload.name)?;
    }

    let ext_base =
        extension_dir().context("could not determine config directory for extensions")?;

    for payload in &artifacts.payloads {
        let ext_dir = ext_base.join(format!("pup-{}", payload.name));
        if ext_dir.exists() && !force {
            bail!(
                "extension '{}' is already installed (use --force to overwrite)",
                payload.name
            );
        }
    }

    let mut installed = Vec::new();
    for payload in artifacts.payloads {
        let ext_dir = ext_base.join(format!("pup-{}", payload.name));
        prepare_extension_dir(&ext_dir)?;
        let exe_name = write_extension_binary(&ext_dir, &payload.name, &payload.bytes)?;

        let manifest = Manifest {
            name: payload.name.clone(),
            version: artifacts.version.clone(),
            source: format!("github:{source}"),
            source_kind: artifacts.source_kind.clone(),
            source_release_tag: artifacts.source_release_tag.clone(),
            source_asset: artifacts.source_asset.clone(),
            installed_at: chrono_now_iso(),
            binary: exe_name,
            description: description.unwrap_or_default().to_string(),
            installed_by_pup: version::VERSION.to_string(),
        };
        manifest.save(&ext_dir.join("manifest.json"))?;
        installed.push(payload.name);
    }

    Ok(installed)
}

async fn required_archive_from_release(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    release: &GitHubRelease,
) -> Result<ArchiveDownload> {
    match download_archive_from_release(client, release, repo).await? {
        Some(archive) => Ok(archive),
        None => bail!(
            "release {} in {owner}/{repo} does not include a platform archive for {}-{}",
            release.tag_name,
            archive_platform_os(),
            archive_platform_arch()
        ),
    }
}

async fn archive_artifacts_for_request(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    tag: Option<&str>,
    extension: Option<&str>,
    all: bool,
) -> Result<GitHubInstallArtifacts> {
    if let Some(extension) = extension {
        validate_extension_name(extension)?;
    }

    if tag.is_none() {
        let Some(extension) = extension else {
            let release = fetch_github_release(client, owner, repo, tag).await?;
            let archive = required_archive_from_release(client, owner, repo, &release).await?;
            let names = selected_archive_extension_names(&archive.extensions, None, all)?;
            let payloads = archive_extension_payloads(&archive, &names)?;
            return Ok(GitHubInstallArtifacts {
                version: archive.version,
                source_kind: Some("github_archive".to_string()),
                source_release_tag: Some(archive.release_tag),
                source_asset: Some(archive.asset_name),
                payloads,
            });
        };
        let releases = fetch_github_releases(client, owner, repo).await?;
        for release in &releases {
            let Some(archive) = download_archive_from_release(client, release, repo).await? else {
                continue;
            };
            if archive.extensions.iter().any(|name| name == extension) {
                let names = vec![extension.to_string()];
                let payloads = archive_extension_payloads(&archive, &names)?;
                return Ok(GitHubInstallArtifacts {
                    version: archive.version,
                    source_kind: Some("github_archive".to_string()),
                    source_release_tag: Some(archive.release_tag),
                    source_asset: Some(archive.asset_name),
                    payloads,
                });
            }
        }
        bail!(
            "no release archive in {owner}/{repo} contains extension '{extension}' for {}-{}",
            archive_platform_os(),
            archive_platform_arch()
        );
    }

    let release = fetch_github_release(client, owner, repo, tag).await?;
    let archive = required_archive_from_release(client, owner, repo, &release).await?;
    let names = selected_archive_extension_names(&archive.extensions, extension, all)?;
    let payloads = archive_extension_payloads(&archive, &names)?;
    Ok(GitHubInstallArtifacts {
        version: archive.version,
        source_kind: Some("github_archive".to_string()),
        source_release_tag: Some(archive.release_tag),
        source_asset: Some(archive.asset_name),
        payloads,
    })
}

async fn latest_archive_artifacts_for_extension(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    extension: &str,
) -> Result<GitHubInstallArtifacts> {
    validate_extension_name(extension)?;

    let releases = fetch_github_releases(client, owner, repo).await?;
    for release in &releases {
        let Some(archive) = download_archive_from_release(client, release, repo).await? else {
            continue;
        };
        if archive.extensions.iter().any(|name| name == extension) {
            let names = vec![extension.to_string()];
            let payloads = archive_extension_payloads(&archive, &names)?;
            return Ok(GitHubInstallArtifacts {
                version: archive.version,
                source_kind: Some("github_archive".to_string()),
                source_release_tag: Some(archive.release_tag),
                source_asset: Some(archive.asset_name),
                payloads,
            });
        }
    }

    bail!(
        "no release archive in {owner}/{repo} contains extension '{extension}' for {}-{}",
        archive_platform_os(),
        archive_platform_arch()
    );
}

/// Install an extension from a GitHub repository.
/// Downloads the appropriate platform-specific binary from GitHub Releases.
pub fn install_from_github(
    source: &str,
    tag: Option<&str>,
    name_override: Option<&str>,
    extension: Option<&str>,
    all: bool,
    force: bool,
    description: Option<&str>,
) -> Result<Vec<String>> {
    let (owner, repo) = parse_owner_repo(source)?;
    if all && description.is_some() {
        bail!("--description can only be used when installing one extension");
    }
    if (extension.is_some() || all) && name_override.is_some() {
        bail!("--name can only be used with single-binary GitHub installs");
    }
    if let Some(t) = tag {
        if !is_valid_tag(t) {
            bail!(
                "invalid release tag '{t}': only alphanumeric characters, hyphens, \
                 underscores, dots, slashes, and plus signs are allowed"
            );
        }
    }

    if extension.is_some() || all {
        let artifacts = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let client = github_client()?;
                archive_artifacts_for_request(&client, owner, repo, tag, extension, all).await
            })
        })?;
        return save_github_payloads(source, artifacts, force, description);
    }

    // The asset name is always derived from the repo (e.g., "hello" from "pup-hello").
    // The ext_name may be overridden by the user via --name for the local directory/manifest.
    let asset_name = derive_name_from_repo(repo);
    let ext_name = match name_override {
        Some(n) => n.to_string(),
        None => asset_name.clone(),
    };

    validate_extension_name(&ext_name)?;

    // Run the async download inside the existing tokio runtime.
    let artifacts = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let client = github_client()?;
            let release = fetch_github_release(&client, owner, repo, tag).await?;
            match find_platform_asset(&release, &asset_name) {
                Ok(asset) => {
                    let bytes = download_asset(&client, asset).await?;
                    Ok::<_, anyhow::Error>(GitHubInstallArtifacts {
                        version: extract_version(&release.tag_name),
                        source_kind: None,
                        source_release_tag: None,
                        source_asset: None,
                        payloads: vec![ExtensionPayload {
                            name: ext_name,
                            bytes,
                        }],
                    })
                }
                Err(single_binary_error) => {
                    if name_override.is_some() {
                        return Err(single_binary_error);
                    }
                    let Some(archive) =
                        download_archive_from_release(&client, &release, repo).await?
                    else {
                        return Err(single_binary_error);
                    };
                    let names = selected_archive_extension_names(&archive.extensions, None, false)?;
                    let payloads = archive_extension_payloads(&archive, &names)?;
                    Ok(GitHubInstallArtifacts {
                        version: archive.version,
                        source_kind: Some("github_archive".to_string()),
                        source_release_tag: Some(archive.release_tag),
                        source_asset: Some(archive.asset_name),
                        payloads,
                    })
                }
            }
        })
    })?;

    save_github_payloads(source, artifacts, force, description)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteExtensionVersion {
    pub name: String,
    pub version: String,
    pub tag: String,
    pub source: String,
    pub asset: String,
    pub inferred_from_archive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ArchiveInventory {
    tag: String,
    version: String,
    asset: String,
    extensions: Vec<String>,
}

fn remote_versions_from_archive_inventory(
    source: &str,
    inventories: &[ArchiveInventory],
    extension_filter: Option<&str>,
) -> Vec<RemoteExtensionVersion> {
    let mut versions = Vec::new();
    for inventory in inventories {
        let mut names = inventory.extensions.clone();
        names.sort();
        names.dedup();
        for name in names {
            if extension_filter.is_some_and(|filter| filter != name) {
                continue;
            }
            versions.push(RemoteExtensionVersion {
                name,
                version: inventory.version.clone(),
                tag: inventory.tag.clone(),
                source: format!("github:{source}"),
                asset: inventory.asset.clone(),
                inferred_from_archive: true,
            });
        }
    }
    versions
}

fn parse_checksums(checksums: &str, asset_name: &str) -> Option<String> {
    checksums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let digest = parts.next()?;
        let file = parts.next()?;
        if file == asset_name {
            Some(digest.to_ascii_lowercase())
        } else {
            None
        }
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .as_slice()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

async fn verify_release_asset_checksum(
    client: &reqwest::Client,
    release: &GitHubRelease,
    asset: &GitHubAsset,
    bytes: &[u8],
) -> Result<()> {
    let Some(checksums_asset) = release.assets.iter().find(|a| a.name == "checksums.txt") else {
        return Ok(());
    };

    let checksums_bytes = download_asset(client, checksums_asset).await?;
    let checksums = String::from_utf8(checksums_bytes).context("checksums.txt is not UTF-8")?;
    let Some(expected) = parse_checksums(&checksums, &asset.name) else {
        return Ok(());
    };

    let actual = sha256_hex(bytes);
    if actual != expected {
        bail!(
            "checksum mismatch for {}: expected {}, got {}",
            asset.name,
            expected,
            actual
        );
    }
    Ok(())
}

async fn archive_inventory_from_release(
    client: &reqwest::Client,
    release: &GitHubRelease,
    project_name: &str,
) -> Result<Option<ArchiveInventory>> {
    Ok(download_archive_from_release(client, release, project_name)
        .await?
        .map(|archive| ArchiveInventory {
            tag: archive.release_tag,
            version: archive.version,
            asset: archive.asset_name,
            extensions: archive.extensions,
        }))
}

async fn download_archive_from_release(
    client: &reqwest::Client,
    release: &GitHubRelease,
    project_name: &str,
) -> Result<Option<ArchiveDownload>> {
    let asset = match find_platform_archive_asset(release, project_name) {
        Ok(asset) => asset,
        Err(_) => return Ok(None),
    };
    let bytes = download_asset(client, asset).await?;
    verify_release_asset_checksum(client, release, asset, &bytes).await?;
    let extensions = extension_names_from_archive(&asset.name, &bytes)?;

    Ok(Some(ArchiveDownload {
        release_tag: release.tag_name.clone(),
        version: extract_version(&release.tag_name),
        asset_name: asset.name.clone(),
        bytes,
        extensions,
    }))
}

pub fn list_remote_extensions(
    source: &str,
    extension: Option<&str>,
) -> Result<Vec<RemoteExtensionVersion>> {
    let (owner, repo) = parse_owner_repo(source)?;
    if let Some(extension) = extension {
        validate_extension_name(extension)?;
    }

    let inventories = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let client = github_client()?;
            let releases = fetch_github_releases(&client, owner, repo).await?;
            let mut inventories = Vec::new();
            for release in &releases {
                if let Some(inventory) =
                    archive_inventory_from_release(&client, release, repo).await?
                {
                    inventories.push(inventory);
                }
            }
            Ok::<_, anyhow::Error>(inventories)
        })
    })?;

    Ok(remote_versions_from_archive_inventory(
        source,
        &inventories,
        extension,
    ))
}

/// Upgrade a single GitHub-sourced extension. Returns a message describing what happened.
pub fn upgrade_extension(name: &str) -> Result<String> {
    validate_extension_name(name)?;

    let ext_base =
        extension_dir().context("could not determine config directory for extensions")?;
    let ext_dir = ext_base.join(format!("pup-{name}"));

    if !ext_dir.exists() {
        bail!("extension '{name}' is not installed");
    }

    let manifest = Manifest::load(&ext_dir.join("manifest.json"))
        .with_context(|| format!("loading manifest for extension '{name}'"))?;

    if manifest.source.starts_with("local:") || manifest.source.starts_with("local-link:") {
        bail!(
            "extension '{name}' was installed from a local source ({}) and cannot be upgraded \
             automatically. Reinstall it manually with: pup extension install --local <path> --force",
            manifest.source
        );
    }

    if !manifest.source.starts_with("github:") {
        bail!(
            "extension '{name}' has an unrecognized source type '{}' and cannot be upgraded",
            manifest.source
        );
    }

    let gh_source = manifest
        .source
        .strip_prefix("github:")
        .expect("source starts with github:");
    let gh_source = gh_source
        .split_once('@')
        .map_or(gh_source, |(base, _)| base);

    let (owner, repo) = parse_owner_repo(gh_source)?;
    // Asset name is derived from the repo, not the manifest name (which may have been overridden
    // via --name at install time).
    let asset_name = derive_name_from_repo(repo);

    // Build the HTTP client once for both the metadata fetch and the binary download.
    let client = github_client()?;

    if manifest.source_kind.as_deref() == Some("github_archive") {
        let old_version = manifest.version.clone();
        let old_release_tag = manifest.source_release_tag.clone();
        let description = manifest.description.clone();
        let artifacts = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                latest_archive_artifacts_for_extension(&client, owner, repo, name).await
            })
        })?;

        let new_version = artifacts.version.clone();
        let new_release_tag = artifacts.source_release_tag.clone();
        let already_latest = match old_release_tag {
            Some(old_tag) => Some(old_tag.as_str()) == new_release_tag.as_deref(),
            None => new_version == old_version,
        };

        if already_latest {
            return Ok(format!("{name}: already at latest version ({new_version})"));
        }

        let description = if description.is_empty() {
            None
        } else {
            Some(description.as_str())
        };
        save_github_payloads(gh_source, artifacts, true, description)?;

        return Ok(format!("{name}: upgraded {old_version} -> {new_version}"));
    }

    // Step 1: Fetch the release metadata (small JSON) and check version BEFORE downloading.
    let release = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current()
            .block_on(async { fetch_github_release(&client, owner, repo, None).await })
    })?;

    let new_version = extract_version(&release.tag_name);

    if new_version == manifest.version {
        return Ok(format!("{name}: already at latest version ({new_version})"));
    }

    let old_version = manifest.version.clone();

    // Step 2: Version differs - now download the binary.
    let asset = find_platform_asset(&release, &asset_name)?;

    let asset_bytes = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async { download_asset(&client, asset).await })
    })?;

    // Prepare (recreate) the extension directory before writing, so a failed write
    // does not leave a partially-corrupted state.
    prepare_extension_dir(&ext_dir)?;

    let exe_name = write_extension_binary(&ext_dir, name, &asset_bytes)?;

    // Update the manifest
    let updated_manifest = Manifest {
        version: new_version.clone(),
        source: format!("github:{gh_source}"),
        source_kind: None,
        source_release_tag: None,
        source_asset: None,
        installed_at: chrono_now_iso(),
        binary: exe_name,
        installed_by_pup: version::VERSION.to_string(),
        ..manifest
    };
    updated_manifest.save(&ext_dir.join("manifest.json"))?;

    Ok(format!("{name}: upgraded {old_version} -> {new_version}"))
}

/// Upgrade all installed extensions. Returns a summary of what happened.
pub fn upgrade_all_extensions() -> Result<Vec<String>> {
    let exts = super::discovery::list_extensions()?;
    if exts.is_empty() {
        return Ok(vec!["No extensions installed.".to_string()]);
    }

    let mut results = Vec::new();

    for ext in &exts {
        if ext.source.starts_with("local:") || ext.source.starts_with("local-link:") {
            results.push(format!(
                "{}: skipped (installed from local source)",
                ext.name
            ));
            continue;
        }
        match upgrade_extension(&ext.name) {
            Ok(msg) => results.push(msg),
            Err(e) => results.push(format!("{}: error: {e}", ext.name)),
        }
    }

    Ok(results)
}

/// Validate that an extension name is well-formed and does not conflict with built-in commands.
pub fn validate_extension_name(name: &str) -> Result<()> {
    // Must match ^[a-z][a-z0-9-]*$
    if name.is_empty() {
        bail!("extension name cannot be empty");
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_lowercase() {
        bail!("extension name must start with a lowercase letter, got '{name}'");
    }
    for c in chars {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-' {
            bail!(
                "extension name '{name}' contains invalid character '{c}' \
                 (only lowercase letters, digits, and hyphens allowed)"
            );
        }
    }

    // Reject names that collide with built-in commands.
    if super::is_builtin_command(name) {
        bail!(
            "'{name}' conflicts with a built-in pup command and cannot be used as an extension name"
        );
    }

    Ok(())
}

/// Install an extension from a local file path.
/// If `link` is true, creates a symlink instead of copying.
pub fn install_from_local(
    source: &Path,
    name: &str,
    link: bool,
    force: bool,
    description: Option<&str>,
) -> Result<()> {
    validate_extension_name(name)?;

    if !source.exists() {
        bail!("source file does not exist: {}", source.display());
    }
    if !source.is_file() {
        bail!(
            "source must be a regular file, not a directory: {}",
            source.display()
        );
    }

    // Canonicalize the source path so that symlinks resolve correctly.
    // Without this, a relative path like ./pup-hello would be resolved
    // relative to the symlink's parent directory, not the user's CWD.
    let source = std::fs::canonicalize(source)
        .with_context(|| format!("resolving absolute path for source: {}", source.display()))?;

    let ext_base =
        extension_dir().context("could not determine config directory for extensions")?;
    let ext_dir = ext_base.join(format!("pup-{name}"));

    if ext_dir.exists() && !force {
        bail!("extension '{name}' is already installed (use --force to overwrite)");
    }

    prepare_extension_dir(&ext_dir)?;

    let exe_name = if link {
        // For symlinks, we need to create the link directly rather than writing bytes.
        let exe_name = if cfg!(target_os = "windows") {
            format!("pup-{name}.exe")
        } else {
            format!("pup-{name}")
        };
        let dest = ext_dir.join(&exe_name);

        #[cfg(unix)]
        std::os::unix::fs::symlink(&source, &dest).with_context(|| {
            format!(
                "creating symlink {} -> {}",
                dest.display(),
                source.display()
            )
        })?;
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&source, &dest).with_context(|| {
            format!(
                "creating symlink {} -> {}",
                dest.display(),
                source.display()
            )
        })?;

        exe_name
    } else {
        let bytes = std::fs::read(&source)
            .with_context(|| format!("reading source file: {}", source.display()))?;
        write_extension_binary(&ext_dir, name, &bytes)?
    };

    let source_str = if link {
        format!("local-link:{}", source.display())
    } else {
        format!("local:{}", source.display())
    };

    // Local installs have no version source (unlike GitHub releases which provide a tag).
    let manifest = Manifest {
        name: name.to_string(),
        version: "unknown".to_string(),
        source: source_str,
        source_kind: None,
        source_release_tag: None,
        source_asset: None,
        installed_at: chrono_now_iso(),
        binary: exe_name,
        description: description.unwrap_or_default().to_string(),
        installed_by_pup: version::VERSION.to_string(),
    };
    manifest.save(&ext_dir.join("manifest.json"))?;

    Ok(())
}

/// Remove an installed extension by name.
pub fn remove_extension(name: &str) -> Result<()> {
    validate_extension_name(name)?;

    let ext_base =
        extension_dir().context("could not determine config directory for extensions")?;
    let ext_dir = ext_base.join(format!("pup-{name}"));

    if !ext_dir.exists() {
        bail!("extension '{name}' is not installed");
    }

    std::fs::remove_dir_all(&ext_dir).with_context(|| format!("removing {}", ext_dir.display()))?;
    Ok(())
}

/// Extract a version string from a GitHub release tag name, stripping the 'v' prefix if present.
fn extract_version(tag_name: &str) -> String {
    tag_name.strip_prefix('v').unwrap_or(tag_name).to_string()
}

/// Return the current time as an ISO 8601 / RFC 3339 string (UTC).
fn chrono_now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_name_valid() {
        assert!(validate_extension_name("hello").is_ok());
        assert!(validate_extension_name("my-tool").is_ok());
        assert!(validate_extension_name("tool2").is_ok());
        assert!(validate_extension_name("a").is_ok());
    }

    #[test]
    fn test_validate_name_empty() {
        assert!(validate_extension_name("").is_err());
    }

    #[test]
    fn test_validate_name_starts_with_digit() {
        assert!(validate_extension_name("2tool").is_err());
    }

    #[test]
    fn test_validate_name_uppercase() {
        assert!(validate_extension_name("Hello").is_err());
    }

    #[test]
    fn test_validate_name_special_chars() {
        assert!(validate_extension_name("my_tool").is_err());
        assert!(validate_extension_name("my.tool").is_err());
    }

    #[test]
    fn test_validate_name_builtin_conflict() {
        assert!(validate_extension_name("monitors").is_err());
        assert!(validate_extension_name("extension").is_err());
        assert!(validate_extension_name("help").is_err());
        assert!(validate_extension_name("version").is_err());
    }

    #[test]
    fn test_validate_name_path_traversal() {
        // Names containing path separators or traversal sequences must be rejected
        assert!(validate_extension_name("../etc").is_err());
        assert!(validate_extension_name("foo/bar").is_err());
        assert!(validate_extension_name("..").is_err());
    }

    #[test]
    fn test_chrono_now_iso_format() {
        let ts = chrono_now_iso();
        // Must parse as a valid RFC 3339 / ISO 8601 timestamp
        assert!(
            chrono::DateTime::parse_from_rfc3339(&ts).is_ok(),
            "chrono_now_iso() produced invalid RFC 3339: {}",
            ts
        );
    }

    #[test]
    fn test_resolve_github_auth_prefers_env_token_over_gh() {
        let auth = resolve_github_auth_with(
            |name| match name {
                "GH_TOKEN" => Some(" env-token \n".to_string()),
                _ => None,
            },
            || Ok("gh-token\n".to_string()),
        );

        assert_eq!(auth.token.as_deref(), Some("env-token"));
        assert_eq!(auth.source, Some(GitHubAuthSource::Env("GH_TOKEN")));
        assert!(auth.gh_error.is_none());
    }

    #[test]
    fn test_resolve_github_auth_ignores_empty_env_and_uses_gh() {
        let auth = resolve_github_auth_with(
            |name| match name {
                "GH_TOKEN" => Some("   ".to_string()),
                "GITHUB_TOKEN" => None,
                "HOMEBREW_GITHUB_API_TOKEN" => None,
                _ => None,
            },
            || Ok(" gh-token \n".to_string()),
        );

        assert_eq!(auth.token.as_deref(), Some("gh-token"));
        assert_eq!(auth.source, Some(GitHubAuthSource::GhActive));
        assert!(auth.gh_error.is_none());
    }

    #[test]
    fn test_resolve_github_auth_falls_back_to_anonymous_when_gh_fails() {
        let auth =
            resolve_github_auth_with(|_| None, || Err(anyhow::anyhow!("gh is not installed")));

        assert_eq!(auth.token, None);
        assert_eq!(auth.source, None);
        assert_eq!(auth.gh_error.as_deref(), Some("gh is not installed"));
    }

    #[test]
    fn test_github_access_guidance_for_active_gh_token_does_not_include_token() {
        let auth = GitHubAuthResolution {
            token: Some("secret-token".to_string()),
            source: Some(GitHubAuthSource::GhActive),
            gh_error: None,
        };

        let guidance = github_access_guidance("owner", "repo", &auth, None);

        assert!(guidance.contains("active GitHub CLI account"));
        assert!(guidance.contains("gh auth status --hostname github.com"));
        assert!(guidance.contains("gh auth switch --hostname github.com"));
        assert!(!guidance.contains("secret-token"));
    }

    #[test]
    fn test_github_access_guidance_for_no_token_mentions_env_and_gh_login() {
        let auth = GitHubAuthResolution {
            token: None,
            source: None,
            gh_error: Some("gh is not installed".to_string()),
        };

        let guidance = github_access_guidance("owner", "repo", &auth, None);

        assert!(guidance.contains("export GH_TOKEN=<token>"));
        assert!(guidance.contains("gh auth login --hostname github.com --scopes repo"));
        assert!(guidance.contains("gh is not installed"));
    }

    #[test]
    fn test_github_auth_status_diagnostic_reports_multiple_accounts() {
        let json = r#"{
            "hosts": {
                "github.com": [
                    {"state": "success", "active": true, "login": "alice"},
                    {"state": "success", "active": false, "login": "bob"}
                ]
            }
        }"#;

        let diagnostic = github_auth_status_diagnostic_from_json(json).unwrap();

        assert!(diagnostic.contains("multiple accounts"));
        assert!(diagnostic.contains("alice"));
        assert!(!diagnostic.contains("bob"));
    }

    #[test]
    fn test_remove_rejects_path_traversal() {
        // remove_extension must reject names with path traversal characters
        // before attempting any filesystem operations.
        assert!(remove_extension("../important-data").is_err());
        assert!(remove_extension("foo/bar").is_err());
        assert!(remove_extension("..").is_err());
    }

    #[test]
    fn test_remove_nonexistent() {
        let dir = std::env::temp_dir().join("pup-test-remove-nonexistent");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("extensions")).unwrap();

        let _guard = crate::test_utils::ENV_LOCK.blocking_lock();
        std::env::set_var("PUP_CONFIG_DIR", &dir);

        let result = remove_extension("nonexistent");
        assert!(result.is_err());

        std::env::remove_var("PUP_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parse_owner_repo_valid() {
        let (owner, repo) = parse_owner_repo("jkirsteins/pup-hello").unwrap();
        assert_eq!(owner, "jkirsteins");
        assert_eq!(repo, "pup-hello");
    }

    #[test]
    fn test_parse_owner_repo_no_slash() {
        assert!(parse_owner_repo("noslash").is_err());
    }

    #[test]
    fn test_parse_owner_repo_empty_parts() {
        assert!(parse_owner_repo("/repo").is_err());
        assert!(parse_owner_repo("owner/").is_err());
        assert!(parse_owner_repo("").is_err());
    }

    #[test]
    fn test_parse_owner_repo_extra_slashes() {
        assert!(parse_owner_repo("a/b/c").is_err());
    }

    #[test]
    fn test_derive_name_from_repo_strips_prefix() {
        assert_eq!(derive_name_from_repo("pup-hello"), "hello");
        assert_eq!(derive_name_from_repo("pup-my-tool"), "my-tool");
    }

    #[test]
    fn test_derive_name_from_repo_no_prefix() {
        assert_eq!(derive_name_from_repo("hello"), "hello");
        assert_eq!(derive_name_from_repo("my-tool"), "my-tool");
    }

    #[test]
    fn test_platform_os_known() {
        let os = platform_os();
        // Should be one of the expected values on any CI/dev machine
        assert!(
            ["darwin", "linux", "windows"].contains(&os),
            "unexpected platform_os: {os}"
        );
    }

    #[test]
    fn test_platform_arch_known() {
        let arch = platform_arch();
        assert!(
            ["x86_64", "aarch64"].contains(&arch),
            "unexpected platform_arch: {arch}"
        );
    }

    #[test]
    fn test_find_platform_asset_found() {
        let os = platform_os();
        let arch = platform_arch();
        let expected_name = format!("pup-hello-{os}-{arch}");
        let release = GitHubRelease {
            tag_name: "v1.0.0".to_string(),
            assets: vec![
                GitHubAsset {
                    name: "pup-hello-linux-x86_64".to_string(),
                    url: None,
                    browser_download_url: "https://example.com/linux-x86_64".to_string(),
                },
                GitHubAsset {
                    name: "pup-hello-darwin-aarch64".to_string(),
                    url: None,
                    browser_download_url: "https://example.com/darwin-aarch64".to_string(),
                },
                GitHubAsset {
                    name: "pup-hello-darwin-x86_64".to_string(),
                    url: None,
                    browser_download_url: "https://example.com/darwin-x86_64".to_string(),
                },
                GitHubAsset {
                    name: "pup-hello-windows-x86_64".to_string(),
                    url: None,
                    browser_download_url: "https://example.com/windows-x86_64".to_string(),
                },
            ],
        };
        let asset = find_platform_asset(&release, "hello").unwrap();
        assert_eq!(asset.name, expected_name);
    }

    #[test]
    fn test_find_platform_asset_not_found() {
        let release = GitHubRelease {
            tag_name: "v1.0.0".to_string(),
            assets: vec![GitHubAsset {
                name: "pup-hello-fakeos-fakearch".to_string(),
                url: None,
                browser_download_url: "https://example.com/fake".to_string(),
            }],
        };
        let result = find_platform_asset(&release, "hello");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("no matching asset"),
            "error should mention 'no matching asset': {err_msg}"
        );
    }

    #[test]
    fn test_find_platform_asset_empty() {
        let release = GitHubRelease {
            tag_name: "v1.0.0".to_string(),
            assets: vec![],
        };
        let result = find_platform_asset(&release, "hello");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("(none)"),
            "error for empty assets should mention '(none)': {err_msg}"
        );
    }

    #[test]
    fn test_is_valid_github_name() {
        assert!(is_valid_github_name("jkirsteins"));
        assert!(is_valid_github_name("pup-hello"));
        assert!(is_valid_github_name("my_repo.v2"));
        assert!(is_valid_github_name("A-Z"));
        // GitHub rejects "." and ".." as repo names
        assert!(!is_valid_github_name("."));
        assert!(!is_valid_github_name(".."));
        assert!(!is_valid_github_name(""));
        assert!(!is_valid_github_name("owner name"));
        assert!(!is_valid_github_name("owner%0a"));
        assert!(!is_valid_github_name("foo/bar"));
    }

    #[test]
    fn test_parse_owner_repo_rejects_invalid_chars() {
        assert!(parse_owner_repo("owner%0a/repo").is_err());
        assert!(parse_owner_repo("owner/repo%00").is_err());
        assert!(parse_owner_repo("own er/repo").is_err());
        assert!(parse_owner_repo("owner/re po").is_err());
    }

    #[test]
    fn test_is_valid_tag() {
        assert!(is_valid_tag("v1.0.0"));
        assert!(is_valid_tag("v1.0.0-rc1"));
        assert!(is_valid_tag("release/v2.0"));
        assert!(is_valid_tag("v1.0.0+build.123"));
        assert!(!is_valid_tag(""));
        assert!(!is_valid_tag("v1.0.0 spaces"));
        assert!(!is_valid_tag("v1.0.0%0a"));
        assert!(!is_valid_tag("v1.0.0\nnewline"));
    }

    #[test]
    fn test_find_platform_asset_uses_asset_name_not_ext_name() {
        // Verify that find_platform_asset uses the repo-derived name, not a user override.
        // If installed with --name custom, the asset should still be looked up as "pup-hello-..."
        let os = platform_os();
        let arch = platform_arch();
        let release = GitHubRelease {
            tag_name: "v1.0.0".to_string(),
            assets: vec![GitHubAsset {
                name: format!("pup-hello-{os}-{arch}"),
                url: None,
                browser_download_url: "https://example.com/hello".to_string(),
            }],
        };
        // Looking up by the repo-derived name "hello" should succeed.
        assert!(find_platform_asset(&release, "hello").is_ok());
        // Looking up by a user-overridden name "custom" should fail (no such asset).
        assert!(find_platform_asset(&release, "custom").is_err());
    }

    #[test]
    fn test_derive_name_from_repo_used_for_asset_lookup() {
        // Verify that derive_name_from_repo produces the correct asset lookup name,
        // independent of any --name override.
        assert_eq!(derive_name_from_repo("pup-hello"), "hello");
        assert_eq!(derive_name_from_repo("pup-my-extension"), "my-extension");
        assert_eq!(derive_name_from_repo("my-tool"), "my-tool");
    }

    fn make_tar_gz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        {
            let mut tar = tar::Builder::new(&mut gz);
            for (path, bytes) in entries {
                let mut header = tar::Header::new_gnu();
                header.set_size(bytes.len() as u64);
                header.set_mode(0o755);
                header.set_cksum();
                tar.append_data(&mut header, path, *bytes).unwrap();
            }
            tar.finish().unwrap();
        }
        gz.finish().unwrap()
    }

    fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut cursor);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(0o755);
            for (path, bytes) in entries {
                zip.start_file(path, options).unwrap();
                std::io::Write::write_all(&mut zip, bytes).unwrap();
            }
            zip.finish().unwrap();
        }
        cursor.into_inner()
    }

    #[test]
    fn test_find_platform_archive_asset_found() {
        let version = "1.2.3";
        let release = GitHubRelease {
            tag_name: format!("v{version}"),
            assets: vec![
                GitHubAsset {
                    name: format!("bundle_{version}_Darwin_arm64.tar.gz"),
                    url: None,
                    browser_download_url: "https://example.com/darwin-arm64".to_string(),
                },
                GitHubAsset {
                    name: format!("bundle_{version}_Darwin_x86_64.tar.gz"),
                    url: None,
                    browser_download_url: "https://example.com/darwin-x86_64".to_string(),
                },
                GitHubAsset {
                    name: format!("bundle_{version}_Linux_arm64.tar.gz"),
                    url: None,
                    browser_download_url: "https://example.com/linux-arm64".to_string(),
                },
                GitHubAsset {
                    name: format!("bundle_{version}_Linux_x86_64.tar.gz"),
                    url: None,
                    browser_download_url: "https://example.com/linux-x86_64".to_string(),
                },
                GitHubAsset {
                    name: format!("bundle_{version}_Windows_x86_64.zip"),
                    url: None,
                    browser_download_url: "https://example.com/windows-x86_64".to_string(),
                },
            ],
        };

        let asset = find_platform_archive_asset(&release, "bundle").unwrap();
        assert!(
            asset.name.contains(&format!(
                "_{}_{}.",
                archive_platform_os(),
                archive_platform_arch()
            )),
            "selected archive should match this platform, got {}",
            asset.name
        );
    }

    #[test]
    fn test_extension_names_from_archive_tar_gz() {
        let archive = make_tar_gz(&[
            ("README.md", b"docs"),
            ("pup-foo", b"foo"),
            ("pup-bar", b"bar"),
            ("nested/pup-hidden", b"hidden"),
            ("not-pup", b"ignored"),
        ]);

        let names = extension_names_from_archive("bundle_1.2.3_Darwin_arm64.tar.gz", &archive)
            .expect("archive should parse");

        assert_eq!(names, vec!["bar".to_string(), "foo".to_string()]);
    }

    #[test]
    fn test_extract_extension_from_archive_tar_gz() {
        let archive = make_tar_gz(&[
            ("pup-foo", b"foo"),
            ("pup-bar", b"bar"),
            ("nested/pup-bar", b"wrong"),
        ]);

        let extracted =
            extract_extension_from_archive("bundle_1.2.3_Darwin_arm64.tar.gz", &archive, "bar")
                .expect("bar should be extracted");

        assert_eq!(extracted, b"bar");
    }

    #[test]
    fn test_extension_names_from_archive_zip() {
        let archive = make_zip(&[
            ("README.md", b"docs"),
            ("pup-foo.exe", b"foo"),
            ("pup-bar.exe", b"bar"),
            ("nested/pup-hidden.exe", b"hidden"),
        ]);

        let names = extension_names_from_archive("bundle_1.2.3_Windows_x86_64.zip", &archive)
            .expect("zip archive should parse");

        assert_eq!(names, vec!["bar".to_string(), "foo".to_string()]);
    }

    #[test]
    fn test_extract_extension_from_archive_zip() {
        let archive = make_zip(&[
            ("pup-foo.exe", b"foo"),
            ("pup-bar.exe", b"bar"),
            ("nested/pup-bar.exe", b"wrong"),
        ]);

        let extracted =
            extract_extension_from_archive("bundle_1.2.3_Windows_x86_64.zip", &archive, "bar")
                .expect("bar should be extracted from zip");

        assert_eq!(extracted, b"bar");
    }

    #[test]
    fn test_selected_archive_extension_names_accepts_exact_extension() {
        let available = vec!["bar".to_string(), "foo".to_string()];

        let selected = selected_archive_extension_names(&available, Some("foo"), false)
            .expect("foo should be selectable");

        assert_eq!(selected, vec!["foo".to_string()]);
    }

    #[test]
    fn test_selected_archive_extension_names_rejects_missing_extension() {
        let available = vec!["bar".to_string()];

        let result = selected_archive_extension_names(&available, Some("foo"), false);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Available extensions: bar"));
    }

    #[test]
    fn test_selected_archive_extension_names_requires_choice_for_multiple() {
        let available = vec!["foo".to_string(), "bar".to_string()];

        let result = selected_archive_extension_names(&available, None, false);

        assert!(result.is_err());
        let message = result.unwrap_err().to_string();
        assert!(message.contains("--extension"));
        assert!(message.contains("--all"));
    }

    #[test]
    fn test_selected_archive_extension_names_infers_single() {
        let available = vec!["foo".to_string()];

        let selected = selected_archive_extension_names(&available, None, false)
            .expect("single extension should be inferred");

        assert_eq!(selected, vec!["foo".to_string()]);
    }

    #[test]
    fn test_selected_archive_extension_names_all_returns_sorted_names() {
        let available = vec!["foo".to_string(), "bar".to_string()];

        let selected =
            selected_archive_extension_names(&available, None, true).expect("all should select");

        assert_eq!(selected, vec!["bar".to_string(), "foo".to_string()]);
    }

    #[test]
    fn test_remote_versions_are_inferred_per_extension() {
        let releases = vec![
            ArchiveInventory {
                tag: "v0.2.0".to_string(),
                version: "0.2.0".to_string(),
                asset: "bundle_0.2.0_Darwin_arm64.tar.gz".to_string(),
                extensions: vec!["bar".to_string(), "foo".to_string()],
            },
            ArchiveInventory {
                tag: "v0.1.0".to_string(),
                version: "0.1.0".to_string(),
                asset: "bundle_0.1.0_Darwin_arm64.tar.gz".to_string(),
                extensions: vec!["foo".to_string()],
            },
        ];

        let versions = remote_versions_from_archive_inventory("owner/repo", &releases, None);

        assert_eq!(
            versions
                .iter()
                .map(|v| (v.name.as_str(), v.tag.as_str()))
                .collect::<Vec<_>>(),
            vec![("bar", "v0.2.0"), ("foo", "v0.2.0"), ("foo", "v0.1.0")]
        );
    }

    #[test]
    fn test_remote_versions_can_filter_one_extension() {
        let releases = vec![
            ArchiveInventory {
                tag: "v0.2.0".to_string(),
                version: "0.2.0".to_string(),
                asset: "bundle_0.2.0_Darwin_arm64.tar.gz".to_string(),
                extensions: vec!["bar".to_string(), "foo".to_string()],
            },
            ArchiveInventory {
                tag: "v0.1.0".to_string(),
                version: "0.1.0".to_string(),
                asset: "bundle_0.1.0_Darwin_arm64.tar.gz".to_string(),
                extensions: vec!["foo".to_string()],
            },
        ];

        let versions = remote_versions_from_archive_inventory("owner/repo", &releases, Some("foo"));

        assert_eq!(
            versions
                .iter()
                .map(|v| (v.name.as_str(), v.tag.as_str()))
                .collect::<Vec<_>>(),
            vec![("foo", "v0.2.0"), ("foo", "v0.1.0")]
        );
    }
}

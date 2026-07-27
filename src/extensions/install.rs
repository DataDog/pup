use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::{self, Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::discovery::extension_dir;
use super::manifest::Manifest;
use crate::version;

const MAX_REMOTE_LIST_RELEASES: usize = 100;
const MAX_REMOTE_LIST_ARCHIVE_BYTES: usize = 100 * 1024 * 1024;
const MAX_IMPLICIT_RELEASE_SCAN: usize = 100;
const MAX_RAW_RELEASE_SCAN: usize = 1000;
const MAX_ARCHIVE_SCAN_TOTAL_BYTES: usize = 512 * 1024 * 1024;
const MAX_INSTALL_ARCHIVE_BYTES: usize = 100 * 1024 * 1024;
const MAX_EXTENSION_BINARY_BYTES: usize = 100 * 1024 * 1024;
const MAX_ARCHIVE_DECODED_BYTES: usize = 256 * 1024 * 1024;
const MAX_SELECTED_ARCHIVE_EXTENSIONS: usize = 100;
const MAX_TOTAL_EXTENSION_PAYLOAD_BYTES: usize = 256 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 4096;
const MAX_CHECKSUMS_BYTES: usize = 1024 * 1024;
const GH_AUTH_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

/// GitHub release asset metadata (subset of the GitHub Releases API response).
#[derive(Debug, serde::Deserialize)]
struct GitHubAsset {
    name: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    size: Option<u64>,
    browser_download_url: String,
}

/// GitHub release metadata (subset of the GitHub Releases API response).
#[derive(Debug, serde::Deserialize)]
struct GitHubRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
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

struct ArchiveScanBudget {
    remaining_bytes: usize,
}

impl ArchiveScanBudget {
    fn new(max_bytes: usize) -> Self {
        Self {
            remaining_bytes: max_bytes,
        }
    }

    fn remaining(&self) -> usize {
        self.remaining_bytes
    }

    fn consume(&mut self, asset: &GitHubAsset, bytes: usize) -> Result<()> {
        if bytes > self.remaining_bytes {
            bail!(
                "archive scan budget exceeded while downloading '{}': {} bytes remaining, {} bytes needed",
                asset.name,
                self.remaining_bytes,
                bytes
            );
        }
        self.remaining_bytes -= bytes;
        Ok(())
    }
}

struct DecodedByteLimitReader<R> {
    inner: R,
    remaining_bytes: usize,
    max_bytes: usize,
}

impl<R> DecodedByteLimitReader<R> {
    fn new(inner: R, max_bytes: usize) -> Self {
        Self {
            inner,
            remaining_bytes: max_bytes,
            max_bytes,
        }
    }
}

impl<R: Read> Read for DecodedByteLimitReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.remaining_bytes == 0 {
            let mut probe = [0_u8; 1];
            return match self.inner.read(&mut probe) {
                Ok(0) => Ok(0),
                Ok(_) => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "tar.gz archive decoded stream is larger than the {} byte decoded limit",
                        self.max_bytes
                    ),
                )),
                Err(err) => Err(err),
            };
        }

        let max_read = buf.len().min(self.remaining_bytes);
        let read = self.inner.read(&mut buf[..max_read])?;
        self.remaining_bytes -= read;
        Ok(read)
    }
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
static GITHUB_GH_AUTH: OnceLock<GitHubAuthResolution> = OnceLock::new();

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

fn resolve_github_env_auth_with<EnvLookup>(env_lookup: EnvLookup) -> GitHubAuthResolution
where
    EnvLookup: Fn(&str) -> Option<String>,
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

    GitHubAuthResolution {
        token: None,
        source: None,
        gh_error: None,
    }
}

fn resolve_github_gh_auth_with<GhToken>(gh_token: GhToken) -> GitHubAuthResolution
where
    GhToken: Fn() -> Result<String>,
{
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

fn command_output_with_timeout(command: &mut Command, timeout: Duration) -> Result<Output> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("starting command")?;
    let started = Instant::now();

    loop {
        if child
            .try_wait()
            .context("checking command status")?
            .is_some()
        {
            return child.wait_with_output().context("reading command output");
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            bail!("command timed out after {} seconds", timeout.as_secs());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn active_gh_token() -> Result<String> {
    let mut command = Command::new("gh");
    command.args(["auth", "token", "--hostname", "github.com"]);
    let output = command_output_with_timeout(&mut command, GH_AUTH_COMMAND_TIMEOUT)
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
    resolve_github_env_auth_with(|name| std::env::var(name).ok())
}

fn resolve_github_gh_auth() -> GitHubAuthResolution {
    resolve_github_gh_auth_with(active_gh_token)
}

fn github_auth() -> &'static GitHubAuthResolution {
    GITHUB_AUTH.get_or_init(resolve_github_auth)
}

fn github_gh_auth() -> &'static GitHubAuthResolution {
    GITHUB_GH_AUTH.get_or_init(resolve_github_gh_auth)
}

fn github_effective_auth() -> &'static GitHubAuthResolution {
    if github_auth().source.is_some() {
        github_auth()
    } else {
        GITHUB_GH_AUTH.get().unwrap_or_else(github_auth)
    }
}

fn github_token() -> Option<&'static str> {
    github_auth()
        .token
        .as_deref()
        .or_else(|| GITHUB_GH_AUTH.get().and_then(|auth| auth.token.as_deref()))
}

fn github_auth_status_diagnostic() -> Option<String> {
    let mut command = Command::new("gh");
    command.args([
        "auth",
        "status",
        "--hostname",
        "github.com",
        "--json",
        "hosts",
    ]);
    let output = command_output_with_timeout(&mut command, GH_AUTH_COMMAND_TIMEOUT).ok()?;

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
               GH_TOKEN=<token> pup extension install {owner}/{repo} --extension <name>"
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
    let auth = github_effective_auth();
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

fn should_retry_github_api_with_gh(status: reqwest::StatusCode) -> bool {
    matches!(
        status,
        reqwest::StatusCode::UNAUTHORIZED
            | reqwest::StatusCode::FORBIDDEN
            | reqwest::StatusCode::NOT_FOUND
    )
}

async fn send_github_api_get(
    client: &reqwest::Client,
    url: &str,
    context: &str,
) -> Result<reqwest::Response> {
    let resp = github_api_get(client, url)
        .send()
        .await
        .with_context(|| format!("{context} from {url}"))?;
    if github_auth().source.is_none()
        && GITHUB_GH_AUTH.get().is_none()
        && should_retry_github_api_with_gh(resp.status())
        && github_gh_auth().token.is_some()
    {
        return github_api_get(client, url)
            .send()
            .await
            .with_context(|| format!("{context} from {url} with GitHub CLI token"));
    }
    Ok(resp)
}

/// Fetch a GitHub release (latest or by tag).
async fn fetch_github_release(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    tag: Option<&str>,
    extension_hint: Option<&str>,
) -> Result<GitHubRelease> {
    if let Some(tag) = tag {
        let url = format!("https://api.github.com/repos/{owner}/{repo}/releases/tags/{tag}");
        let resp = send_github_api_get(client, &url, "fetching release").await?;

        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            let guidance = github_failure_guidance(owner, repo);
            bail!(
                "{}\n\n{}",
                release_tag_not_found_message(owner, repo, tag, extension_hint),
                guidance
            );
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            let guidance = github_failure_guidance(owner, repo);
            bail!("GitHub API returned {status} for {url}: {body}\n\n{guidance}");
        }

        return resp
            .json::<GitHubRelease>()
            .await
            .with_context(|| format!("parsing release JSON from {url}"));
    }

    let url = format!("https://api.github.com/repos/{owner}/{repo}/releases/latest");

    let resp = send_github_api_get(client, &url, "fetching release").await?;

    let status = resp.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        let guidance = github_failure_guidance(owner, repo);
        bail!(
            "no releases found for {owner}/{repo}. \
             Check that the repository exists and has at least one release at \
             https://github.com/{owner}/{repo}/releases\n\n\
             {guidance}"
        );
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

/// Fetch one page of GitHub releases, newest-first.
async fn fetch_github_release_page(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    page: usize,
) -> Result<Vec<GitHubRelease>> {
    let url =
        format!("https://api.github.com/repos/{owner}/{repo}/releases?per_page=100&page={page}");
    let resp = send_github_api_get(client, &url, "fetching releases").await?;

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

    resp.json::<Vec<GitHubRelease>>()
        .await
        .with_context(|| format!("parsing release JSON from {url}"))
}

fn stable_releases_within_scan_limit<'a>(
    releases: &'a [GitHubRelease],
    scanned: &mut usize,
    max_releases: usize,
) -> Vec<&'a GitHubRelease> {
    let remaining = max_releases.saturating_sub(*scanned);
    let selected = releases
        .iter()
        .filter(|release| is_stable_release(release))
        .take(remaining)
        .collect::<Vec<_>>();
    *scanned += selected.len();
    selected
}

fn release_scan_limit_message(owner: &str, repo: &str, max_releases: usize) -> String {
    format!(
        "searched the first {max_releases} releases in {owner}/{repo} without finding a matching \
         platform archive. Use --tag to install an exact release."
    )
}

fn raw_release_scan_limit_message(owner: &str, repo: &str, max_releases: usize) -> String {
    format!(
        "searched the first {max_releases} releases in {owner}/{repo} without finding enough \
         stable release archives. Use --tag to install an exact release."
    )
}

fn remote_list_raw_release_scan_limit_message(
    owner: &str,
    repo: &str,
    max_releases: usize,
) -> String {
    format!(
        "searched the first {max_releases} releases in {owner}/{repo} without completing remote \
         extension discovery. If you know the extension and release tag, install that exact release \
         with --tag."
    )
}

fn is_stable_release(release: &GitHubRelease) -> bool {
    !release.draft && !release.prerelease
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
    extension_names_from_tar_gz_with_limits(
        bytes,
        MAX_EXTENSION_BINARY_BYTES,
        MAX_ARCHIVE_DECODED_BYTES,
    )
}

fn extension_names_from_tar_gz_with_limits(
    bytes: &[u8],
    max_member_bytes: usize,
    max_decoded_bytes: usize,
) -> Result<Vec<String>> {
    let decoder = flate2::read::GzDecoder::new(Cursor::new(bytes));
    let decoder = DecodedByteLimitReader::new(decoder, max_decoded_bytes);
    let mut archive = tar::Archive::new(decoder);
    let mut names = Vec::new();
    let mut decoded_bytes = 0;

    for (index, entry) in archive
        .entries()
        .context("reading tar.gz archive entries")?
        .enumerate()
    {
        if index >= MAX_ARCHIVE_ENTRIES {
            bail!("archive contains more than {MAX_ARCHIVE_ENTRIES} entries");
        }
        let entry = entry.context("reading tar.gz archive entry")?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry
            .path()
            .context("reading tar.gz archive path")?
            .into_owned();
        let size = entry.size();
        account_tar_file_size(
            &mut decoded_bytes,
            &path,
            size,
            max_member_bytes,
            max_decoded_bytes,
        )?;
        if let Some(name) = extension_name_from_archive_path(&path) {
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

    if archive.len() > MAX_ARCHIVE_ENTRIES {
        bail!("archive contains more than {MAX_ARCHIVE_ENTRIES} entries");
    }

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
    extract_extension_from_archive_with_limit(asset_name, bytes, name, MAX_EXTENSION_BINARY_BYTES)
}

fn extract_extension_from_archive_with_limit(
    asset_name: &str,
    bytes: &[u8],
    name: &str,
    max_member_bytes: usize,
) -> Result<Vec<u8>> {
    validate_extension_name(name)?;
    if asset_name.ends_with(".tar.gz") {
        extract_extension_from_tar_gz(bytes, name, max_member_bytes)
    } else if asset_name.ends_with(".zip") {
        extract_extension_from_zip(bytes, name, max_member_bytes)
    } else {
        bail!("unsupported extension archive format: {asset_name}");
    }
}

fn ensure_archive_member_size(size: u64, name: &str, max_member_bytes: usize) -> Result<()> {
    if size > max_member_bytes as u64 {
        bail!(
            "archive member for extension '{name}' is larger than the {} byte limit",
            max_member_bytes
        );
    }
    Ok(())
}

fn account_tar_file_size(
    decoded_bytes: &mut u64,
    path: &Path,
    size: u64,
    max_member_bytes: usize,
    max_decoded_bytes: usize,
) -> Result<()> {
    if size > max_member_bytes as u64 {
        bail!(
            "archive member '{}' is larger than the {} byte limit",
            path.display(),
            max_member_bytes
        );
    }
    *decoded_bytes = decoded_bytes
        .checked_add(size)
        .context("tar.gz archive decoded size overflowed")?;
    if *decoded_bytes > max_decoded_bytes as u64 {
        bail!(
            "tar.gz archive file contents are larger than the {} byte decoded limit",
            max_decoded_bytes
        );
    }
    Ok(())
}

fn read_limited_archive_member<R: Read>(
    reader: R,
    name: &str,
    max_member_bytes: usize,
    context: &str,
) -> Result<Vec<u8>> {
    let mut limited = reader.take(max_member_bytes as u64 + 1);
    let mut bytes = Vec::new();
    limited
        .read_to_end(&mut bytes)
        .context(context.to_string())?;
    if bytes.len() > max_member_bytes {
        bail!(
            "archive member for extension '{name}' is larger than the {} byte limit",
            max_member_bytes
        );
    }
    Ok(bytes)
}

fn extract_extension_from_tar_gz(
    bytes: &[u8],
    name: &str,
    max_member_bytes: usize,
) -> Result<Vec<u8>> {
    extract_extension_from_tar_gz_with_limits(
        bytes,
        name,
        max_member_bytes,
        MAX_ARCHIVE_DECODED_BYTES,
    )
}

fn extract_extension_from_tar_gz_with_limits(
    bytes: &[u8],
    name: &str,
    max_member_bytes: usize,
    max_decoded_bytes: usize,
) -> Result<Vec<u8>> {
    let decoder = flate2::read::GzDecoder::new(Cursor::new(bytes));
    let decoder = DecodedByteLimitReader::new(decoder, max_decoded_bytes);
    let mut archive = tar::Archive::new(decoder);
    let mut decoded_bytes = 0;

    for (index, entry) in archive
        .entries()
        .context("reading tar.gz archive entries")?
        .enumerate()
    {
        if index >= MAX_ARCHIVE_ENTRIES {
            bail!("archive contains more than {MAX_ARCHIVE_ENTRIES} entries");
        }
        let mut entry = entry.context("reading tar.gz archive entry")?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry
            .path()
            .context("reading tar.gz archive path")?
            .into_owned();
        let size = entry.size();
        account_tar_file_size(
            &mut decoded_bytes,
            &path,
            size,
            max_member_bytes,
            max_decoded_bytes,
        )?;
        let matches = extension_archive_member_matches(&path, name);
        if matches {
            return read_limited_archive_member(
                &mut entry,
                name,
                max_member_bytes,
                "reading extension binary from tar.gz archive",
            );
        }
    }

    bail!("archive does not contain extension 'pup-{name}'")
}

fn extract_extension_from_zip(
    bytes: &[u8],
    name: &str,
    max_member_bytes: usize,
) -> Result<Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).context("reading zip archive")?;

    if archive.len() > MAX_ARCHIVE_ENTRIES {
        bail!("archive contains more than {MAX_ARCHIVE_ENTRIES} entries");
    }

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .with_context(|| format!("reading zip archive entry {i}"))?;
        if !file.is_file() {
            continue;
        }
        if extension_archive_member_matches(Path::new(file.name()), name) {
            ensure_archive_member_size(file.size(), name, max_member_bytes)?;
            return read_limited_archive_member(
                &mut file,
                name,
                max_member_bytes,
                "reading extension binary from zip archive",
            );
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
    download_asset_with_limit(client, asset, Some(MAX_EXTENSION_BINARY_BYTES)).await
}

async fn download_asset_with_limit(
    client: &reqwest::Client,
    asset: &GitHubAsset,
    max_bytes: Option<usize>,
) -> Result<Vec<u8>> {
    if let Some(max_bytes) = max_bytes {
        if asset.size.is_some_and(|size| size > max_bytes as u64) {
            bail!(
                "asset '{}' is larger than the {} byte limit",
                asset.name,
                max_bytes
            );
        }
    }

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

    if let Some(max_bytes) = max_bytes {
        if resp
            .content_length()
            .is_some_and(|content_length| content_length > max_bytes as u64)
        {
            bail!(
                "asset '{}' is larger than the {} byte limit",
                asset.name,
                max_bytes
            );
        }

        use futures_util::StreamExt;

        let mut bytes = Vec::new();
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.with_context(|| format!("reading asset bytes from {url}"))?;
            if bytes.len() + chunk.len() > max_bytes {
                bail!(
                    "asset '{}' is larger than the {} byte limit",
                    asset.name,
                    max_bytes
                );
            }
            bytes.extend_from_slice(&chunk);
        }
        return Ok(bytes);
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

fn release_tag_suggestion(tag: &str) -> Option<String> {
    if !tag.starts_with('v') && looks_like_semver_tag(tag) {
        Some(format!("v{tag}"))
    } else {
        None
    }
}

fn looks_like_semver_tag(tag: &str) -> bool {
    let core = tag.split_once(['-', '+']).map_or(tag, |(core, _)| core);
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
}

fn release_tag_not_found_message(
    owner: &str,
    repo: &str,
    tag: &str,
    extension_hint: Option<&str>,
) -> String {
    let hint = if let Some(suggestion) = release_tag_suggestion(tag) {
        let extension_arg = extension_hint
            .map(|extension| format!(" --extension {extension}"))
            .unwrap_or_default();
        format!(
            "\n\nIf you copied a version from `pup extension list-remote`, use the tag shown in parentheses:\n  pup extension install {owner}/{repo}{extension_arg} --tag {suggestion}"
        )
    } else {
        String::new()
    };
    format!(
        "release tag '{tag}' not found or repository inaccessible in {owner}/{repo}. `--tag` uses exact GitHub release tags. \
         Check available releases at https://github.com/{owner}/{repo}/releases{hint}"
    )
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

#[derive(Debug)]
struct StagedExtension {
    target_dir: PathBuf,
    stage_dir: PathBuf,
    backup_dir: PathBuf,
}

#[derive(Debug)]
struct CommitStagedError {
    message: String,
    rollback_incomplete: bool,
}

impl CommitStagedError {
    fn new(error: anyhow::Error) -> Self {
        Self {
            message: error.to_string(),
            rollback_incomplete: false,
        }
    }

    fn rollback_incomplete(error: anyhow::Error, rollback_error: anyhow::Error) -> Self {
        Self {
            message: format!("{error}\n\n{rollback_error}"),
            rollback_incomplete: true,
        }
    }

    fn into_anyhow(self) -> anyhow::Error {
        anyhow::anyhow!(self.message)
    }
}

impl std::fmt::Display for CommitStagedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CommitStagedError {}

fn unique_work_dir(parent: &Path, prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    parent.join(format!(".{prefix}-{}-{nanos}", std::process::id()))
}

fn cleanup_dir(path: &Path) {
    if path.exists() {
        let _ = std::fs::remove_dir_all(path);
    }
}

fn remove_dir_all_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        std::fs::remove_dir_all(path)
            .with_context(|| format!("removing directory {}", path.display()))?;
    }
    Ok(())
}

fn rollback_staged_extensions(
    installed_targets: &[PathBuf],
    backups: &[(PathBuf, PathBuf)],
) -> Result<()> {
    let mut errors = Vec::new();

    for target in installed_targets.iter().rev() {
        if let Err(error) = remove_dir_all_if_exists(target) {
            errors.push(error.to_string());
        }
    }

    for (target, backup) in backups.iter().rev() {
        if !backup.exists() {
            continue;
        }
        if target.exists() {
            errors.push(format!(
                "could not restore backup {} to {} because target still exists",
                backup.display(),
                target.display()
            ));
            continue;
        }
        if let Err(error) = std::fs::rename(backup, target).with_context(|| {
            format!(
                "restoring backup {} to {}",
                backup.display(),
                target.display()
            )
        }) {
            errors.push(error.to_string());
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        bail!(
            "rollback after failed install was incomplete:\n- {}",
            errors.join("\n- ")
        )
    }
}

fn commit_staged_extensions(
    staged: &[StagedExtension],
    force: bool,
) -> std::result::Result<(), CommitStagedError> {
    commit_staged_extensions_with_hook(staged, force, |_| Ok(()))
}

fn commit_staged_extensions_with_hook<F>(
    staged: &[StagedExtension],
    force: bool,
    mut before_install: F,
) -> std::result::Result<(), CommitStagedError>
where
    F: FnMut(usize) -> Result<()>,
{
    let mut backups = Vec::new();
    let mut installed_targets = Vec::new();

    let result = (|| -> Result<()> {
        for staged_extension in staged {
            if staged_extension.target_dir.exists() {
                if !force {
                    bail!(
                        "extension '{}' is already installed (use --force to overwrite)",
                        staged_extension
                            .target_dir
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("unknown")
                            .trim_start_matches("pup-")
                    );
                }
                std::fs::rename(&staged_extension.target_dir, &staged_extension.backup_dir)
                    .with_context(|| {
                        format!(
                            "backing up existing extension directory {}",
                            staged_extension.target_dir.display()
                        )
                    })?;
                backups.push((
                    staged_extension.target_dir.clone(),
                    staged_extension.backup_dir.clone(),
                ));
            }
        }

        for (index, staged_extension) in staged.iter().enumerate() {
            before_install(index)?;
            std::fs::rename(&staged_extension.stage_dir, &staged_extension.target_dir)
                .with_context(|| {
                    format!(
                        "installing extension directory {}",
                        staged_extension.target_dir.display()
                    )
                })?;
            installed_targets.push(staged_extension.target_dir.clone());
        }

        Ok(())
    })();

    if let Err(error) = result {
        if let Err(rollback_error) = rollback_staged_extensions(&installed_targets, &backups) {
            return Err(CommitStagedError::rollback_incomplete(
                error,
                rollback_error,
            ));
        }
        return Err(CommitStagedError::new(error));
    }

    for (_, backup) in backups {
        cleanup_dir(&backup);
    }

    Ok(())
}

fn commit_staged_extensions_and_cleanup(
    staged: &[StagedExtension],
    force: bool,
    staging_base: &Path,
    backup_base: &Path,
) -> Result<()> {
    commit_staged_extensions_with_cleanup(
        staged,
        force,
        staging_base,
        backup_base,
        commit_staged_extensions,
    )
}

fn commit_staged_extensions_with_cleanup<F>(
    staged: &[StagedExtension],
    force: bool,
    staging_base: &Path,
    backup_base: &Path,
    commit: F,
) -> Result<()>
where
    F: FnOnce(&[StagedExtension], bool) -> std::result::Result<(), CommitStagedError>,
{
    if let Err(error) = commit(staged, force) {
        let rollback_incomplete = error.rollback_incomplete;
        let error = error.into_anyhow();
        cleanup_dir(staging_base);
        if rollback_incomplete {
            bail!(
                "{error}\n\nremaining extension backups were preserved in {}",
                backup_base.display()
            );
        }
        cleanup_dir(backup_base);
        return Err(error);
    }

    cleanup_dir(staging_base);
    cleanup_dir(backup_base);
    Ok(())
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
    archive_extension_payloads_with_limits(
        archive,
        names,
        MAX_SELECTED_ARCHIVE_EXTENSIONS,
        MAX_TOTAL_EXTENSION_PAYLOAD_BYTES,
    )
}

fn archive_extension_payloads_with_limits(
    archive: &ArchiveDownload,
    names: &[String],
    max_selected: usize,
    max_total_bytes: usize,
) -> Result<Vec<ExtensionPayload>> {
    if names.len() > max_selected {
        bail!("selected archive contains more than {max_selected} extensions");
    }

    let mut payloads = Vec::with_capacity(names.len());
    let mut total_bytes = 0usize;
    for name in names {
        let bytes = extract_extension_from_archive(&archive.asset_name, &archive.bytes, name)?;
        total_bytes = total_bytes
            .checked_add(bytes.len())
            .context("selected extension payload size overflowed")?;
        if total_bytes > max_total_bytes {
            bail!(
                "selected archive extensions are larger than the {} byte aggregate limit",
                max_total_bytes
            );
        }
        payloads.push(ExtensionPayload {
            name: name.clone(),
            bytes,
        });
    }

    Ok(payloads)
}

fn save_github_payloads(
    source: &str,
    artifacts: GitHubInstallArtifacts,
    force: bool,
    description: Option<&str>,
) -> Result<Vec<String>> {
    save_github_payloads_with_commit(
        source,
        artifacts,
        force,
        description,
        commit_staged_extensions,
    )
}

fn save_github_payloads_with_commit<F>(
    source: &str,
    artifacts: GitHubInstallArtifacts,
    force: bool,
    description: Option<&str>,
    commit: F,
) -> Result<Vec<String>>
where
    F: FnOnce(&[StagedExtension], bool) -> std::result::Result<(), CommitStagedError>,
{
    if artifacts.payloads.is_empty() {
        bail!("no extensions selected to install");
    }
    if artifacts.payloads.len() > 1 && description.is_some() {
        bail!("--description can only be used when installing one extension");
    }

    for payload in &artifacts.payloads {
        validate_extension_name(&payload.name)?;
    }
    let mut seen_names = HashSet::new();
    for payload in &artifacts.payloads {
        if !seen_names.insert(payload.name.as_str()) {
            bail!("extension '{}' was selected more than once", payload.name);
        }
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

    let staging_base = unique_work_dir(&ext_base, "pup-install-staging");
    std::fs::create_dir_all(&staging_base)
        .with_context(|| format!("creating staging directory {}", staging_base.display()))?;
    let backup_base = unique_work_dir(&ext_base, "pup-install-backup");
    if let Err(error) = std::fs::create_dir_all(&backup_base)
        .with_context(|| format!("creating backup directory {}", backup_base.display()))
    {
        cleanup_dir(&staging_base);
        return Err(error);
    }

    let stage_result = (|| -> Result<(Vec<StagedExtension>, Vec<String>)> {
        let mut staged = Vec::new();
        let mut installed = Vec::new();
        let installed_at = chrono_now_iso();

        for payload in artifacts.payloads {
            let ext_dir = ext_base.join(format!("pup-{}", payload.name));
            let stage_dir = staging_base.join(format!("pup-{}", payload.name));
            std::fs::create_dir_all(&stage_dir)
                .with_context(|| format!("creating {}", stage_dir.display()))?;
            let exe_name = write_extension_binary(&stage_dir, &payload.name, &payload.bytes)?;

            let manifest = Manifest {
                name: payload.name.clone(),
                version: artifacts.version.clone(),
                source: format!("github:{source}"),
                source_kind: artifacts.source_kind.clone(),
                source_release_tag: artifacts.source_release_tag.clone(),
                source_asset: artifacts.source_asset.clone(),
                installed_at: installed_at.clone(),
                binary: exe_name,
                description: description.unwrap_or_default().to_string(),
                installed_by_pup: version::VERSION.to_string(),
            };
            manifest.save(&stage_dir.join("manifest.json"))?;

            staged.push(StagedExtension {
                target_dir: ext_dir,
                stage_dir,
                backup_dir: backup_base.join(format!("pup-{}", payload.name)),
            });
            installed.push(payload.name);
        }

        Ok((staged, installed))
    })();

    let (staged, installed) = match stage_result {
        Ok(result) => result,
        Err(error) => {
            cleanup_dir(&staging_base);
            cleanup_dir(&backup_base);
            return Err(error);
        }
    };

    commit_staged_extensions_with_cleanup(&staged, force, &staging_base, &backup_base, commit)?;
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
            let release = fetch_github_release(client, owner, repo, tag, None).await?;
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
        return latest_archive_artifacts_for_extension(client, owner, repo, extension).await;
    }

    let release = fetch_github_release(client, owner, repo, tag, extension).await?;
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

    let mut stable_scanned = 0;
    let mut raw_scanned = 0;
    let mut page = 1;
    let mut scan_budget = ArchiveScanBudget::new(MAX_ARCHIVE_SCAN_TOTAL_BYTES);

    loop {
        let releases = fetch_github_release_page(client, owner, repo, page).await?;
        if releases.is_empty() {
            break;
        }

        let page_len = releases.len();
        raw_scanned += page_len;
        for release in stable_releases_within_scan_limit(
            &releases,
            &mut stable_scanned,
            MAX_IMPLICIT_RELEASE_SCAN,
        ) {
            let Some(archive) = download_archive_from_release_for_scan(
                client,
                release,
                repo,
                MAX_INSTALL_ARCHIVE_BYTES,
                &mut scan_budget,
            )
            .await?
            else {
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

        if page_len < 100 {
            break;
        }
        if stable_scanned >= MAX_IMPLICIT_RELEASE_SCAN {
            bail!(
                "{}",
                release_scan_limit_message(owner, repo, MAX_IMPLICIT_RELEASE_SCAN)
            );
        }
        if raw_scanned >= MAX_RAW_RELEASE_SCAN {
            bail!(
                "{}",
                raw_release_scan_limit_message(owner, repo, MAX_RAW_RELEASE_SCAN)
            );
        }
        page += 1;
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
            let release = fetch_github_release(&client, owner, repo, tag, None).await?;
            match find_platform_asset(&release, &asset_name) {
                Ok(asset) => {
                    let bytes = download_asset(&client, asset).await?;
                    verify_release_asset_checksum(&client, &release, asset, &bytes).await?;
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

fn parse_checksums(checksums: &str, asset_name: &str) -> Result<Option<String>> {
    for line in checksums.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(digest) = parts.next() else {
            continue;
        };
        let Some(file) = parts.next() else {
            continue;
        };
        let file = file.trim_start_matches('*');
        if file == asset_name {
            let digest = digest.to_ascii_lowercase();
            if digest.len() != 64 || !digest.chars().all(|c| c.is_ascii_hexdigit()) {
                bail!("invalid SHA-256 checksum for {asset_name} in checksums.txt");
            }
            return Ok(Some(digest));
        }
    }
    Ok(None)
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .as_slice()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn verify_checksum_contents(checksums: &str, asset_name: &str, bytes: &[u8]) -> Result<()> {
    let Some(expected) = parse_checksums(checksums, asset_name)? else {
        bail!("checksums.txt does not contain a SHA-256 checksum for {asset_name}");
    };

    let actual = sha256_hex(bytes);
    if actual != expected {
        bail!(
            "checksum mismatch for {}: expected {}, got {}",
            asset_name,
            expected,
            actual
        );
    }
    Ok(())
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

    let checksums_bytes =
        download_asset_with_limit(client, checksums_asset, Some(MAX_CHECKSUMS_BYTES)).await?;
    let checksums = String::from_utf8(checksums_bytes).context("checksums.txt is not UTF-8")?;
    verify_checksum_contents(&checksums, &asset.name, bytes)
}

async fn archive_inventory_from_release(
    client: &reqwest::Client,
    release: &GitHubRelease,
    project_name: &str,
    scan_budget: &mut ArchiveScanBudget,
) -> Result<Option<ArchiveInventory>> {
    Ok(download_archive_from_release_for_scan(
        client,
        release,
        project_name,
        MAX_REMOTE_LIST_ARCHIVE_BYTES,
        scan_budget,
    )
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
    download_archive_from_release_with_limit(
        client,
        release,
        project_name,
        Some(MAX_INSTALL_ARCHIVE_BYTES),
    )
    .await
}

async fn download_archive_from_release_for_scan(
    client: &reqwest::Client,
    release: &GitHubRelease,
    project_name: &str,
    max_asset_bytes: usize,
    scan_budget: &mut ArchiveScanBudget,
) -> Result<Option<ArchiveDownload>> {
    let asset = match find_platform_archive_asset(release, project_name) {
        Ok(asset) => asset,
        Err(_) => return Ok(None),
    };

    let max_bytes = max_asset_bytes.min(scan_budget.remaining());
    if max_bytes == 0 {
        bail!(
            "archive scan budget exhausted before downloading '{}'",
            asset.name
        );
    }
    let bytes = download_asset_with_limit(client, asset, Some(max_bytes)).await?;
    scan_budget.consume(asset, bytes.len())?;
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

async fn download_archive_from_release_with_limit(
    client: &reqwest::Client,
    release: &GitHubRelease,
    project_name: &str,
    max_bytes: Option<usize>,
) -> Result<Option<ArchiveDownload>> {
    let asset = match find_platform_archive_asset(release, project_name) {
        Ok(asset) => asset,
        Err(_) => return Ok(None),
    };
    let bytes = download_asset_with_limit(client, asset, max_bytes).await?;
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
            let mut inventories = Vec::new();
            let mut stable_scanned = 0;
            let mut raw_scanned = 0;
            let mut page = 1;
            let mut scan_budget = ArchiveScanBudget::new(MAX_ARCHIVE_SCAN_TOTAL_BYTES);

            loop {
                let releases = fetch_github_release_page(&client, owner, repo, page).await?;
                if releases.is_empty() {
                    break;
                }

                let page_len = releases.len();
                raw_scanned += page_len;
                for release in stable_releases_within_scan_limit(
                    &releases,
                    &mut stable_scanned,
                    MAX_REMOTE_LIST_RELEASES,
                ) {
                    if let Some(inventory) =
                        archive_inventory_from_release(&client, release, repo, &mut scan_budget)
                            .await?
                    {
                        inventories.push(inventory);
                    }
                }

                if page_len < 100 || stable_scanned >= MAX_REMOTE_LIST_RELEASES {
                    break;
                }
                if raw_scanned >= MAX_RAW_RELEASE_SCAN {
                    bail!(
                        "{}",
                        remote_list_raw_release_scan_limit_message(
                            owner,
                            repo,
                            MAX_RAW_RELEASE_SCAN
                        )
                    );
                }
                page += 1;
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
            .block_on(async { fetch_github_release(&client, owner, repo, None, None).await })
    })?;

    let new_version = extract_version(&release.tag_name);

    if new_version == manifest.version {
        return Ok(format!("{name}: already at latest version ({new_version})"));
    }

    let old_version = manifest.version.clone();

    // Step 2: Version differs - now download the binary.
    let asset = find_platform_asset(&release, &asset_name)?;

    let asset_bytes = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let asset_bytes = download_asset(&client, asset).await?;
            verify_release_asset_checksum(&client, &release, asset, &asset_bytes).await?;
            Ok::<_, anyhow::Error>(asset_bytes)
        })
    })?;

    let description = if manifest.description.is_empty() {
        None
    } else {
        Some(manifest.description.as_str())
    };
    save_github_payloads(
        gh_source,
        GitHubInstallArtifacts {
            version: new_version.clone(),
            source_kind: None,
            source_release_tag: None,
            source_asset: None,
            payloads: vec![ExtensionPayload {
                name: name.to_string(),
                bytes: asset_bytes,
            }],
        },
        true,
        description,
    )?;

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

    let staging_base = unique_work_dir(&ext_base, "pup-install-staging");
    std::fs::create_dir_all(&staging_base)
        .with_context(|| format!("creating staging directory {}", staging_base.display()))?;
    let backup_base = unique_work_dir(&ext_base, "pup-install-backup");
    if let Err(error) = std::fs::create_dir_all(&backup_base)
        .with_context(|| format!("creating backup directory {}", backup_base.display()))
    {
        cleanup_dir(&staging_base);
        return Err(error);
    }

    let stage_dir = staging_base.join(format!("pup-{name}"));
    let stage_result = (|| -> Result<StagedExtension> {
        std::fs::create_dir_all(&stage_dir)
            .with_context(|| format!("creating {}", stage_dir.display()))?;

        let exe_name = if link {
            // For symlinks, we need to create the link directly rather than writing bytes.
            let exe_name = if cfg!(target_os = "windows") {
                format!("pup-{name}.exe")
            } else {
                format!("pup-{name}")
            };
            let dest = stage_dir.join(&exe_name);

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
            write_extension_binary(&stage_dir, name, &bytes)?
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
        manifest.save(&stage_dir.join("manifest.json"))?;

        Ok(StagedExtension {
            target_dir: ext_dir,
            stage_dir,
            backup_dir: backup_base.join(format!("pup-{name}")),
        })
    })();

    let staged = match stage_result {
        Ok(staged) => vec![staged],
        Err(error) => {
            cleanup_dir(&staging_base);
            cleanup_dir(&backup_base);
            return Err(error);
        }
    };

    commit_staged_extensions_and_cleanup(&staged, force, &staging_base, &backup_base)?;

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
        let auth = resolve_github_env_auth_with(|name| match name {
            "GH_TOKEN" => Some(" env-token \n".to_string()),
            _ => None,
        });

        assert_eq!(auth.token.as_deref(), Some("env-token"));
        assert_eq!(auth.source, Some(GitHubAuthSource::Env("GH_TOKEN")));
        assert!(auth.gh_error.is_none());
    }

    #[test]
    fn test_resolve_github_auth_ignores_empty_env_without_gh_lookup() {
        let auth = resolve_github_env_auth_with(|name| match name {
            "GH_TOKEN" => Some("   ".to_string()),
            "GITHUB_TOKEN" => None,
            "HOMEBREW_GITHUB_API_TOKEN" => None,
            _ => None,
        });

        assert_eq!(auth.token, None);
        assert_eq!(auth.source, None);
        assert!(auth.gh_error.is_none());
    }

    #[test]
    fn test_resolve_github_gh_auth_uses_gh_token_when_requested() {
        let auth = resolve_github_gh_auth_with(|| Ok(" gh-token \n".to_string()));

        assert_eq!(auth.token.as_deref(), Some("gh-token"));
        assert_eq!(auth.source, Some(GitHubAuthSource::GhActive));
        assert!(auth.gh_error.is_none());
    }

    #[test]
    fn test_resolve_github_auth_falls_back_to_anonymous_when_gh_fails() {
        let auth = resolve_github_gh_auth_with(|| Err(anyhow::anyhow!("gh is not installed")));

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
    fn test_download_asset_applies_default_binary_size_limit_from_metadata() {
        let client = github_client().unwrap();
        let asset = GitHubAsset {
            name: "pup-foo-linux-x86_64".to_string(),
            url: None,
            size: Some(MAX_EXTENSION_BINARY_BYTES as u64 + 1),
            browser_download_url: "http://127.0.0.1:1/pup-foo".to_string(),
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let result = runtime.block_on(download_asset(&client, &asset));

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("byte limit"));
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
            draft: false,
            prerelease: false,
            assets: vec![
                GitHubAsset {
                    name: "pup-hello-linux-x86_64".to_string(),
                    url: None,
                    size: None,
                    browser_download_url: "https://example.com/linux-x86_64".to_string(),
                },
                GitHubAsset {
                    name: "pup-hello-darwin-aarch64".to_string(),
                    url: None,
                    size: None,
                    browser_download_url: "https://example.com/darwin-aarch64".to_string(),
                },
                GitHubAsset {
                    name: "pup-hello-darwin-x86_64".to_string(),
                    url: None,
                    size: None,
                    browser_download_url: "https://example.com/darwin-x86_64".to_string(),
                },
                GitHubAsset {
                    name: "pup-hello-windows-x86_64".to_string(),
                    url: None,
                    size: None,
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
            draft: false,
            prerelease: false,
            assets: vec![GitHubAsset {
                name: "pup-hello-fakeos-fakearch".to_string(),
                url: None,
                size: None,
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
            draft: false,
            prerelease: false,
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
    fn test_release_tag_suggestion_adds_v_for_plain_version() {
        assert_eq!(release_tag_suggestion("0.2.1").as_deref(), Some("v0.2.1"));
    }

    #[test]
    fn test_release_tag_suggestion_ignores_existing_v_tag() {
        assert_eq!(release_tag_suggestion("v0.2.1"), None);
    }

    #[test]
    fn test_release_tag_suggestion_ignores_slash_tags() {
        assert_eq!(release_tag_suggestion("release/v2.0"), None);
    }

    #[test]
    fn test_release_tag_suggestion_ignores_non_semver_tags() {
        assert_eq!(release_tag_suggestion("latest"), None);
    }

    #[test]
    fn test_release_tag_not_found_message_suggests_listed_tag() {
        let message = release_tag_not_found_message("owner", "repo", "0.2.1", Some("foo"));

        assert!(message.contains("release tag '0.2.1' not found or repository inaccessible"));
        assert!(message.contains("exact GitHub release tags"));
        assert!(message.contains("extension install owner/repo --extension foo --tag v0.2.1"));
        assert!(!message.contains("GitHub access failed"));
    }

    #[test]
    fn test_release_tag_not_found_message_omits_extension_when_unknown() {
        let message = release_tag_not_found_message("owner", "repo", "0.2.1", None);

        assert!(message.contains("extension install owner/repo --tag v0.2.1"));
        assert!(!message.contains("--extension"));
    }

    #[test]
    fn test_is_stable_release_rejects_drafts_and_prereleases() {
        let stable = GitHubRelease {
            tag_name: "v1.0.0".to_string(),
            draft: false,
            prerelease: false,
            assets: vec![],
        };
        let draft = GitHubRelease {
            tag_name: "v1.0.1".to_string(),
            draft: true,
            prerelease: false,
            assets: vec![],
        };
        let prerelease = GitHubRelease {
            tag_name: "v1.1.0-rc.1".to_string(),
            draft: false,
            prerelease: true,
            assets: vec![],
        };

        assert!(is_stable_release(&stable));
        assert!(!is_stable_release(&draft));
        assert!(!is_stable_release(&prerelease));
    }

    #[test]
    fn test_remote_listing_release_filter_matches_implicit_install() {
        let releases = [
            GitHubRelease {
                tag_name: "v1.0.0".to_string(),
                draft: false,
                prerelease: false,
                assets: vec![],
            },
            GitHubRelease {
                tag_name: "v1.1.0-rc.1".to_string(),
                draft: false,
                prerelease: true,
                assets: vec![],
            },
            GitHubRelease {
                tag_name: "v1.2.0".to_string(),
                draft: true,
                prerelease: false,
                assets: vec![],
            },
        ];

        let listed_tags = releases
            .iter()
            .filter(|release| is_stable_release(release))
            .map(|release| release.tag_name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(listed_tags, vec!["v1.0.0"]);
    }

    #[test]
    fn test_stable_releases_within_scan_limit_returns_first_stable_releases_before_limit() {
        let releases: Vec<GitHubRelease> = (0..101)
            .map(|index| GitHubRelease {
                tag_name: format!("v{index}"),
                draft: false,
                prerelease: false,
                assets: vec![],
            })
            .collect();
        let mut scanned = 0;

        let scanned_releases = stable_releases_within_scan_limit(&releases, &mut scanned, 100);

        assert_eq!(scanned_releases.len(), 100);
        assert_eq!(scanned, 100);
        assert_eq!(scanned_releases[0].tag_name, "v0");
        assert_eq!(scanned_releases[99].tag_name, "v99");
    }

    #[test]
    fn test_stable_releases_within_scan_limit_skips_prereleases_before_counting() {
        let mut releases: Vec<GitHubRelease> = (0..100)
            .map(|index| GitHubRelease {
                tag_name: format!("v1.0.{index}-rc.1"),
                draft: false,
                prerelease: true,
                assets: vec![],
            })
            .collect();
        releases.push(GitHubRelease {
            tag_name: "v1.0.0".to_string(),
            draft: false,
            prerelease: false,
            assets: vec![],
        });
        let mut scanned = 0;

        let scanned_releases = stable_releases_within_scan_limit(&releases, &mut scanned, 1);

        assert_eq!(scanned_releases.len(), 1);
        assert_eq!(scanned, 1);
        assert_eq!(scanned_releases[0].tag_name, "v1.0.0");
    }

    #[test]
    fn test_find_platform_asset_uses_asset_name_not_ext_name() {
        // Verify that find_platform_asset uses the repo-derived name, not a user override.
        // If installed with --name custom, the asset should still be looked up as "pup-hello-..."
        let os = platform_os();
        let arch = platform_arch();
        let release = GitHubRelease {
            tag_name: "v1.0.0".to_string(),
            draft: false,
            prerelease: false,
            assets: vec![GitHubAsset {
                name: format!("pup-hello-{os}-{arch}"),
                url: None,
                size: None,
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

    #[test]
    fn test_parse_checksums_accepts_sha256_entry() {
        let digest = sha256_hex(b"payload");
        let checksums = format!("{digest}  archive.tar.gz\n");

        let parsed = parse_checksums(&checksums, "archive.tar.gz").unwrap();

        assert_eq!(parsed.as_deref(), Some(digest.as_str()));
    }

    #[test]
    fn test_parse_checksums_accepts_binary_mode_filename() {
        let digest = sha256_hex(b"payload");
        let checksums = format!("{digest} *archive.tar.gz\n");

        let parsed = parse_checksums(&checksums, "archive.tar.gz").unwrap();

        assert_eq!(parsed.as_deref(), Some(digest.as_str()));
    }

    #[test]
    fn test_parse_checksums_rejects_invalid_matching_digest() {
        let result = parse_checksums("not-a-sha archive.tar.gz\n", "archive.tar.gz");

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid SHA-256"));
    }

    #[test]
    fn test_verify_checksum_contents_accepts_match() {
        let digest = sha256_hex(b"payload");
        let checksums = format!("{digest} archive.tar.gz\n");

        verify_checksum_contents(&checksums, "archive.tar.gz", b"payload").unwrap();
    }

    #[test]
    fn test_verify_checksum_contents_rejects_missing_asset() {
        let digest = sha256_hex(b"payload");
        let checksums = format!("{digest} other.tar.gz\n");
        let result = verify_checksum_contents(&checksums, "archive.tar.gz", b"payload");

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("does not contain a SHA-256 checksum"));
    }

    #[test]
    fn test_verify_checksum_contents_rejects_mismatch() {
        let checksums = format!("{} archive.tar.gz\n", "0".repeat(64));
        let result = verify_checksum_contents(&checksums, "archive.tar.gz", b"payload");

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("checksum mismatch"));
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

    fn make_tar_gz_with_declared_file_size(path: &str, size: u64) -> Vec<u8> {
        let mut header = tar::Header::new_gnu();
        header.set_path(path).unwrap();
        header.set_size(size);
        header.set_mode(0o755);
        header.set_cksum();

        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut gz, header.as_bytes()).unwrap();
        gz.finish().unwrap()
    }

    fn append_raw_tar_entry(tar: &mut Vec<u8>, header: &mut tar::Header, data: &[u8]) {
        header.set_cksum();
        std::io::Write::write_all(tar, header.as_bytes()).unwrap();
        std::io::Write::write_all(tar, data).unwrap();
        let padding = (512 - data.len() % 512) % 512;
        if padding > 0 {
            std::io::Write::write_all(tar, &vec![0; padding]).unwrap();
        }
    }

    fn gzip_tar_bytes(tar: &[u8]) -> Vec<u8> {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut gz, tar).unwrap();
        gz.finish().unwrap()
    }

    fn make_tar_gz_with_pax_size_override() -> Vec<u8> {
        let pax = b"9 size=3\n";
        let mut tar = Vec::new();

        let mut pax_header = tar::Header::new_ustar();
        pax_header.set_path("PaxHeaders/pup-foo").unwrap();
        pax_header.set_entry_type(tar::EntryType::XHeader);
        pax_header.set_size(pax.len() as u64);
        pax_header.set_mode(0o644);
        append_raw_tar_entry(&mut tar, &mut pax_header, pax);

        let mut file_header = tar::Header::new_gnu();
        file_header.set_path("pup-foo").unwrap();
        file_header.set_size(1);
        file_header.set_mode(0o755);
        append_raw_tar_entry(&mut tar, &mut file_header, b"abc");

        tar.extend_from_slice(&[0; 1024]);
        gzip_tar_bytes(&tar)
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

    fn test_archive_download(
        asset_name: &str,
        bytes: Vec<u8>,
        extensions: Vec<String>,
    ) -> ArchiveDownload {
        ArchiveDownload {
            release_tag: "v1.2.3".to_string(),
            version: "1.2.3".to_string(),
            asset_name: asset_name.to_string(),
            bytes,
            extensions,
        }
    }

    #[test]
    fn test_archive_scan_budget_rejects_over_budget_consume() {
        let asset = GitHubAsset {
            name: "bundle.tar.gz".to_string(),
            url: None,
            size: None,
            browser_download_url: "https://example.com/bundle".to_string(),
        };
        let mut budget = ArchiveScanBudget::new(3);

        budget.consume(&asset, 2).unwrap();
        let result = budget.consume(&asset, 2);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("archive scan budget exceeded"));
    }

    #[test]
    fn test_find_platform_archive_asset_found() {
        let version = "1.2.3";
        let release = GitHubRelease {
            tag_name: format!("v{version}"),
            draft: false,
            prerelease: false,
            assets: vec![
                GitHubAsset {
                    name: format!("bundle_{version}_Darwin_arm64.tar.gz"),
                    url: None,
                    size: None,
                    browser_download_url: "https://example.com/darwin-arm64".to_string(),
                },
                GitHubAsset {
                    name: format!("bundle_{version}_Darwin_x86_64.tar.gz"),
                    url: None,
                    size: None,
                    browser_download_url: "https://example.com/darwin-x86_64".to_string(),
                },
                GitHubAsset {
                    name: format!("bundle_{version}_Linux_arm64.tar.gz"),
                    url: None,
                    size: None,
                    browser_download_url: "https://example.com/linux-arm64".to_string(),
                },
                GitHubAsset {
                    name: format!("bundle_{version}_Linux_x86_64.tar.gz"),
                    url: None,
                    size: None,
                    browser_download_url: "https://example.com/linux-x86_64".to_string(),
                },
                GitHubAsset {
                    name: format!("bundle_{version}_Windows_x86_64.zip"),
                    url: None,
                    size: None,
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
    fn test_extension_names_from_archive_tar_gz_rejects_oversized_nonmatching_member() {
        let archive = make_tar_gz_with_declared_file_size("not-pup", 3);

        let result = extension_names_from_tar_gz_with_limits(&archive, 2, 4096);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("byte limit"));
    }

    #[test]
    fn test_extension_names_from_archive_tar_gz_uses_pax_size_override() {
        let archive = make_tar_gz_with_pax_size_override();

        let result = extension_names_from_tar_gz_with_limits(&archive, 2, 4096);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("byte limit"));
    }

    #[test]
    fn test_extension_names_from_archive_tar_gz_caps_decoded_longname_payloads() {
        let long_path = format!("not-pup-{}", "a".repeat(160));
        let archive = make_tar_gz(&[(&long_path, b"x")]);

        let result = extension_names_from_tar_gz_with_limits(&archive, 1024, 1024);

        assert!(result.is_err());
        assert!(format!("{:?}", result.unwrap_err()).contains("decoded limit"));
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
    fn test_extract_extension_from_archive_tar_gz_rejects_oversized_nonmatching_member() {
        let archive = make_tar_gz_with_declared_file_size("not-pup", 3);

        let result = extract_extension_from_tar_gz_with_limits(&archive, "foo", 2, 4096);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("byte limit"));
    }

    #[test]
    fn test_extract_extension_from_archive_tar_gz_rejects_decoded_limit() {
        let archive = make_tar_gz(&[("not-pup", b"abc"), ("pup-foo", b"foo")]);

        let result = extract_extension_from_tar_gz_with_limits(&archive, "foo", 10, 5);

        assert!(result.is_err());
        assert!(format!("{:?}", result.unwrap_err()).contains("decoded limit"));
    }

    #[test]
    fn test_extract_extension_from_archive_tar_gz_rejects_oversized_member() {
        let archive = make_tar_gz(&[("pup-foo", b"foo")]);

        let result = extract_extension_from_archive_with_limit(
            "bundle_1.2.3_Darwin_arm64.tar.gz",
            &archive,
            "foo",
            2,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("byte limit"));
    }

    #[test]
    fn test_archive_extension_payloads_rejects_too_many_selected_extensions() {
        let archive = test_archive_download(
            "bundle_1.2.3_Darwin_arm64.tar.gz",
            make_tar_gz(&[("pup-foo", b"foo")]),
            vec!["foo".to_string()],
        );
        let names = (0..3)
            .map(|index| format!("foo{index}"))
            .collect::<Vec<_>>();

        let result = archive_extension_payloads_with_limits(&archive, &names, 2, 1024);

        assert!(result.is_err());
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("more than 2 extensions"));
    }

    #[test]
    fn test_archive_extension_payloads_rejects_aggregate_payload_limit() {
        let archive = test_archive_download(
            "bundle_1.2.3_Darwin_arm64.tar.gz",
            make_tar_gz(&[("pup-foo", b"foo"), ("pup-bar", b"bar")]),
            vec!["foo".to_string(), "bar".to_string()],
        );
        let names = vec!["foo".to_string(), "bar".to_string()];

        let result = archive_extension_payloads_with_limits(&archive, &names, 10, 5);

        assert!(result.is_err());
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("aggregate limit"));
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
    fn test_extract_extension_from_archive_zip_rejects_oversized_member() {
        let archive = make_zip(&[("pup-foo.exe", b"foo")]);

        let result = extract_extension_from_archive_with_limit(
            "bundle_1.2.3_Windows_x86_64.zip",
            &archive,
            "foo",
            2,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("byte limit"));
    }

    fn write_test_file(path: &Path, contents: &[u8]) {
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn test_commit_staged_extensions_rolls_back_before_install() {
        let dir = std::env::temp_dir().join(format!(
            "pup-test-rollback-before-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let target_foo = dir.join("pup-foo");
        let target_bar = dir.join("pup-bar");
        let stage_foo = dir.join("stage-foo");
        let stage_bar = dir.join("stage-bar");
        let backup_foo = dir.join("backup-foo");
        let backup_bar = dir.join("backup-bar");
        for path in [&target_foo, &target_bar, &stage_foo, &stage_bar] {
            std::fs::create_dir_all(path).unwrap();
        }
        write_test_file(&target_foo.join("pup-foo"), b"old-foo");
        write_test_file(&target_bar.join("pup-bar"), b"old-bar");
        write_test_file(&stage_foo.join("pup-foo"), b"new-foo");
        write_test_file(&stage_bar.join("pup-bar"), b"new-bar");

        let staged = vec![
            StagedExtension {
                target_dir: target_foo.clone(),
                stage_dir: stage_foo,
                backup_dir: backup_foo,
            },
            StagedExtension {
                target_dir: target_bar.clone(),
                stage_dir: stage_bar,
                backup_dir: backup_bar,
            },
        ];

        let result = commit_staged_extensions_with_hook(&staged, true, |index| {
            if index == 0 {
                anyhow::bail!("injected failure");
            }
            Ok(())
        });

        assert!(result.is_err());
        assert_eq!(
            std::fs::read(target_foo.join("pup-foo")).unwrap(),
            b"old-foo"
        );
        assert_eq!(
            std::fs::read(target_bar.join("pup-bar")).unwrap(),
            b"old-bar"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_commit_staged_extensions_rolls_back_after_partial_install() {
        let dir = std::env::temp_dir().join(format!(
            "pup-test-rollback-after-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let target_foo = dir.join("pup-foo");
        let target_bar = dir.join("pup-bar");
        let stage_foo = dir.join("stage-foo");
        let stage_bar = dir.join("stage-bar");
        let backup_foo = dir.join("backup-foo");
        let backup_bar = dir.join("backup-bar");
        for path in [&target_foo, &target_bar, &stage_foo, &stage_bar] {
            std::fs::create_dir_all(path).unwrap();
        }
        write_test_file(&target_foo.join("pup-foo"), b"old-foo");
        write_test_file(&target_bar.join("pup-bar"), b"old-bar");
        write_test_file(&stage_foo.join("pup-foo"), b"new-foo");
        write_test_file(&stage_bar.join("pup-bar"), b"new-bar");

        let staged = vec![
            StagedExtension {
                target_dir: target_foo.clone(),
                stage_dir: stage_foo,
                backup_dir: backup_foo,
            },
            StagedExtension {
                target_dir: target_bar.clone(),
                stage_dir: stage_bar,
                backup_dir: backup_bar,
            },
        ];

        let result = commit_staged_extensions_with_hook(&staged, true, |index| {
            if index == 1 {
                anyhow::bail!("injected failure");
            }
            Ok(())
        });

        assert!(result.is_err());
        assert_eq!(
            std::fs::read(target_foo.join("pup-foo")).unwrap(),
            b"old-foo"
        );
        assert_eq!(
            std::fs::read(target_bar.join("pup-bar")).unwrap(),
            b"old-bar"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_rollback_staged_extensions_reports_unrestored_backup() {
        let dir = std::env::temp_dir().join(format!(
            "pup-test-rollback-report-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let target = dir.join("pup-foo");
        let backup = dir.join("backup-foo");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::create_dir_all(&backup).unwrap();
        write_test_file(&target.join("pup-foo"), b"new-foo");
        write_test_file(&backup.join("pup-foo"), b"old-foo");

        let result = rollback_staged_extensions(&[], &[(target.clone(), backup.clone())]);

        assert!(result.is_err());
        let message = result.unwrap_err().to_string();
        assert!(message.contains("rollback after failed install was incomplete"));
        assert!(message.contains("target still exists"));
        assert_eq!(std::fs::read(target.join("pup-foo")).unwrap(), b"new-foo");
        assert_eq!(std::fs::read(backup.join("pup-foo")).unwrap(), b"old-foo");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_save_github_payloads_preserves_backups_after_incomplete_rollback() {
        let dir = std::env::temp_dir().join(format!(
            "pup-test-save-preserve-backup-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let _guard = crate::test_utils::ENV_LOCK.blocking_lock();
        std::env::set_var("PUP_CONFIG_DIR", &dir);

        let artifacts = GitHubInstallArtifacts {
            version: "1.2.3".to_string(),
            source_kind: None,
            source_release_tag: None,
            source_asset: None,
            payloads: vec![ExtensionPayload {
                name: "foo".to_string(),
                bytes: b"new-foo".to_vec(),
            }],
        };

        let result =
            save_github_payloads_with_commit("owner/repo", artifacts, true, None, |staged, _| {
                std::fs::create_dir_all(&staged[0].backup_dir).unwrap();
                write_test_file(&staged[0].backup_dir.join("pup-foo"), b"old-foo");
                Err(CommitStagedError::rollback_incomplete(
                    anyhow::anyhow!("install failed"),
                    anyhow::anyhow!("rollback failed"),
                ))
            });

        assert!(result.is_err());
        let message = result.unwrap_err().to_string();
        assert!(message.contains("rollback failed"));
        assert!(message.contains("remaining extension backups were preserved"));

        let backup_dirs = std::fs::read_dir(dir.join("extensions"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(".pup-install-backup-"))
            })
            .collect::<Vec<_>>();
        assert_eq!(backup_dirs.len(), 1);
        assert_eq!(
            std::fs::read(backup_dirs[0].join("pup-foo").join("pup-foo")).unwrap(),
            b"old-foo"
        );

        std::env::remove_var("PUP_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&dir);
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

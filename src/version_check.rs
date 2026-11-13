//! Best-effort update notices for the native CLI, independent of Datadog auth.

mod cache;

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use semver::Version;
use serde::Deserialize;

const RELEASE_URL: &str = "https://api.github.com/repos/DataDog/pup/releases/latest";
const CHECK_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) async fn check() {
    if std::env::var("PUP_NO_UPDATE_CHECK").as_deref() == Ok("1") {
        return;
    }
    let Some(dir) = crate::settings::config_dir() else {
        return;
    };
    let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return;
    };
    if let Ok(Some(notice)) =
        check_at(&dir, RELEASE_URL, crate::version::VERSION, now.as_secs()).await
    {
        // A closed stderr must not affect the command's output or exit status.
        use std::io::Write;
        let _ = writeln!(std::io::stderr(), "{notice}");
    }
}

async fn check_at(
    dir: &Path,
    release_url: &str,
    current: &str,
    now: u64,
) -> anyhow::Result<Option<String>> {
    if !cache::claim_check(dir, now).context("claiming daily version check")? {
        return Ok(None);
    }

    // Use a separate unauthenticated client: Datadog credentials never go to GitHub.
    let client = reqwest::Client::builder()
        .timeout(CHECK_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(format!("pup/{current}"))
        .build()
        .context("building release client")?;
    let release = client
        .get(release_url)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .context("checking latest release")?
        .error_for_status()
        .context("fetching latest release")?
        .json::<Release>()
        .await
        .context("parsing latest release")?;

    Ok(update_notice(current, &release.tag_name))
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
}

fn update_notice(current: &str, tag: &str) -> Option<String> {
    let current = Version::parse(current).ok()?;
    let latest = Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()?;
    if !latest.pre.is_empty() || !latest.cmp_precedence(&current).is_gt() {
        return None;
    }
    Some(format!(
        "pup {latest} is available (installed: {current}). See https://github.com/DataDog/pup/releases/latest"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    const NOW: u64 = 1_800_000_000;

    #[test]
    fn compares_release_versions_by_semver_precedence() {
        for (current, latest) in [
            ("2.9.0", "v2.10.0"),
            ("2.4.4", "3.0.0"),
            ("2.4.4", "v2.4.5"),
            ("2.4.5-rc.1", "v2.4.5"),
        ] {
            let notice = update_notice(current, latest).unwrap();
            assert!(notice.contains(latest.trim_start_matches('v')));
            assert!(notice.contains(current));
            assert!(notice.contains("https://github.com/DataDog/pup/releases/latest"));
        }
    }

    #[test]
    fn ignores_current_older_prerelease_and_malformed_versions() {
        for (current, latest) in [
            ("2.4.4", "v2.4.4"),
            ("2.4.4", "v2.4.3"),
            ("3.0.0", "v2.99.0"),
            ("2.4.4", "v2.5.0-rc.1"),
            ("2.4.4", "v2.4.4+build.1"),
            ("2.4.4+build.1", "v2.4.4+build.2"),
            ("2.4.4", "invalid"),
            ("dev", "v2.5.0"),
            ("2.4.4", "v2.5.0\nmalicious output"),
        ] {
            assert_eq!(update_notice(current, latest), None, "{current}, {latest}");
        }
    }

    #[tokio::test]
    async fn fetches_once_and_checks_again_after_a_day() {
        let tmp = TempDir::new("version_check_success");
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/latest")
            .match_header("user-agent", "pup/2.4.4")
            .match_header("accept", "application/vnd.github+json")
            .match_header("authorization", mockito::Matcher::Missing)
            .with_header("content-type", "application/json")
            .with_body(r#"{"tag_name":"v2.5.0"}"#)
            .expect(2)
            .create_async()
            .await;
        let url = format!("{}/latest", server.url());

        let notice = check_at(tmp.path(), &url, "2.4.4", NOW).await.unwrap();
        assert_eq!(notice.as_deref(), Some("pup 2.5.0 is available (installed: 2.4.4). See https://github.com/DataDog/pup/releases/latest"));
        assert!(check_at(tmp.path(), &url, "2.4.4", NOW + 86_399)
            .await
            .unwrap()
            .is_none());
        assert!(check_at(tmp.path(), &url, "2.4.4", NOW + 86_400)
            .await
            .unwrap()
            .is_some());
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn failed_requests_and_bad_responses_are_throttled() {
        for (status, body) in [
            (403, "rate limited"),
            (500, "unavailable"),
            (200, "invalid json"),
            (200, r#"{"message":"missing tag"}"#),
        ] {
            let tmp = TempDir::new("version_check_failure");
            let mut server = mockito::Server::new_async().await;
            let mock = server
                .mock("GET", "/latest")
                .with_status(status)
                .with_body(body)
                .expect(1)
                .create_async()
                .await;
            let url = format!("{}/latest", server.url());
            assert!(check_at(tmp.path(), &url, "2.4.4", NOW).await.is_err());
            assert!(check_at(tmp.path(), &url, "2.4.4", NOW + 1)
                .await
                .unwrap()
                .is_none());
            mock.assert_async().await;
        }
    }

    #[tokio::test]
    async fn slow_requests_time_out_and_are_throttled() {
        let tmp = TempDir::new("version_check_timeout");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/latest", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (_connection, _) = listener.accept().await.unwrap();
            std::future::pending::<()>().await;
        });

        let error = tokio::time::timeout(
            CHECK_TIMEOUT + Duration::from_secs(1),
            check_at(tmp.path(), &url, "2.4.4", NOW),
        )
        .await
        .expect("release check must be bounded")
        .unwrap_err();
        server.abort();
        assert!(error.downcast_ref::<reqwest::Error>().unwrap().is_timeout());
        assert!(check_at(tmp.path(), &url, "2.4.4", NOW + 1)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn unavailable_state_skips_the_network_request() {
        let tmp = TempDir::new("version_check_no_state");
        std::fs::create_dir(tmp.path().join("version-check")).unwrap();
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("GET", "/latest").expect(0).create_async().await;
        assert!(check_at(
            tmp.path(),
            &format!("{}/latest", server.url()),
            "2.4.4",
            NOW
        )
        .await
        .is_err());
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn opt_out_does_not_create_state() {
        let _guard = crate::test_support::lock_env().await;
        let tmp = TempDir::new("version_check_disabled");
        let previous_dir = std::env::var_os("PUP_CONFIG_DIR");
        let previous_disabled = std::env::var_os("PUP_NO_UPDATE_CHECK");
        std::env::set_var("PUP_CONFIG_DIR", tmp.path());
        std::env::set_var("PUP_NO_UPDATE_CHECK", "1");
        check().await;
        assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 0);
        for (key, value) in [
            ("PUP_CONFIG_DIR", previous_dir),
            ("PUP_NO_UPDATE_CHECK", previous_disabled),
        ] {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

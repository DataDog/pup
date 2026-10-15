//! Runbook pup steps run real `pup` subprocesses: poll conditions and
//! captures see the raw payload even though the default output is enveloped.
use std::path::Path;
use std::process::Command;

fn write(path: &Path, contents: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn run_runbook(name: &str, runbook: &str) -> (bool, String, String) {
    run_runbook_with(name, runbook, &[])
}

fn run_runbook_with(name: &str, runbook: &str, globals: &[&str]) -> (bool, String, String) {
    let dir = std::env::temp_dir().join(format!("pup-runbook-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    write(&dir.join("runbooks").join(format!("{name}.yaml")), runbook);
    write(
        &dir.join("items.json"),
        r#"[{"name":"a"},{"name":"b"},{"name":"c"}]"#,
    );
    let runbook_text = std::fs::read_to_string(dir.join("runbooks").join(format!("{name}.yaml")))
        .unwrap()
        .replace("ITEMS", dir.join("items.json").to_str().unwrap());
    write(
        &dir.join("runbooks").join(format!("{name}.yaml")),
        &runbook_text,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_pup"))
        .args(globals)
        .args(["runbooks", "run", name])
        .env("PUP_CONFIG_DIR", &dir)
        .env("DD_TOKEN_STORAGE", "file")
        .env("DD_ACCESS_TOKEN", "local-fixture-token")
        .env("DD_SITE", "datadoghq.com")
        .env_remove("PUP_NO_ENVELOPE")
        .env_remove("PUP_OUTPUT")
        .env_remove("PUP_FILTER")
        .output()
        .expect("run pup");
    let _ = std::fs::remove_dir_all(&dir);
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

#[test]
fn poll_until_value_sees_the_raw_jq_result() {
    let (ok, _stdout, stderr) = run_runbook(
        "poll-value",
        r#"name: poll-value
steps:
  - name: Count items
    kind: pup
    run: format --input ITEMS --jq length
    poll:
      interval: 1s
      timeout: 5s
      until: value < 5
"#,
    );
    assert!(ok, "the poll must match the raw count 3: {stderr}");
    assert!(!stderr.contains("poll timeout"), "{stderr}");
}

#[test]
fn poll_until_value_still_times_out_when_the_condition_is_false() {
    let (ok, _stdout, stderr) = run_runbook(
        "poll-timeout",
        r#"name: poll-timeout
steps:
  - name: Count items
    kind: pup
    run: format --input ITEMS --jq length
    poll:
      interval: 1s
      timeout: 2s
      until: value < 2
"#,
    );
    assert!(!ok);
    assert!(stderr.contains("poll timeout"), "{stderr}");
}

#[test]
fn capture_stores_the_raw_payload_while_the_display_is_enveloped() {
    let (ok, stdout, stderr) = run_runbook(
        "capture",
        r#"name: capture
steps:
  - name: First name
    kind: pup
    run: format --input ITEMS --jq .[0].name
    capture: FIRST
  - name: Use it
    kind: shell
    run: echo captured={{FIRST}}
"#,
    );
    assert!(ok, "{stderr}");
    // The displayed step output follows the default envelope...
    assert!(stdout.contains(r#""status": "success""#), "{stdout}");
    // ...but the capture is the raw jq result (a JSON string), as
    // --no-envelope prints it: the rendered shell command shows it verbatim.
    assert!(stderr.contains(r#"$ echo captured="a""#), "{stderr}");
    assert!(stdout.contains("captured=a\n"), "{stdout}");
    assert!(
        !stderr.contains("captured={"),
        "an envelope was captured: {stderr}"
    );
}

#[test]
fn table_display_shows_rows_with_metadata_on_stderr() {
    let (ok, stdout, stderr) = run_runbook_with(
        "table",
        r#"name: table
steps:
  - name: List
    kind: pup
    run: format --input ITEMS
"#,
        &["--output", "table"],
    );
    assert!(ok, "{stderr}");
    assert!(stdout.contains("| a "), "rows on stdout: {stdout}");
    assert!(
        !stdout.contains("\"status\""),
        "no envelope in table: {stdout}"
    );
    assert!(
        stderr.contains(r#"metadata: command="format" count=3 truncated=false"#),
        "{stderr}"
    );
}

#[test]
fn plain_text_step_is_shown_and_captured_as_is() {
    let (ok, stdout, stderr) = run_runbook(
        "plain",
        r#"name: plain
steps:
  - name: Version
    kind: pup
    run: version
    capture: VERSION
  - name: Use it
    kind: shell
    run: echo 'v={{VERSION}}'
"#,
    );
    assert!(ok, "{stderr}");
    assert!(stdout.contains("Pup "), "{stdout}");
    assert!(stdout.contains("v=Pup "), "{stdout}");
}

#[test]
fn pup_payload_file_is_not_honored() {
    // The removed internal side channel must not come back as an
    // environment-driven write hook.
    let dir = std::env::temp_dir().join(format!("pup-payload-negative-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("items.json");
    let target = dir.join("payload.json");
    std::fs::write(&input, "[1]").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_pup"))
        .args(["format", "--input", input.to_str().unwrap()])
        .env("PUP_PAYLOAD_FILE", &target)
        .env("DD_TOKEN_STORAGE", "file")
        .output()
        .expect("run pup");
    let exists = target.exists();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(output.status.success());
    assert!(!exists, "PUP_PAYLOAD_FILE must not be written");
}

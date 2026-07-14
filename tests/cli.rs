//! CLI integration tests.
//!
//! These exercise the compiled binary end-to-end: exit codes, the
//! stdout/stderr contract (stdout = data, stderr = reports), and
//! mutation roundtrips on temp copies of the sample chronicle.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

const SAMPLE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/sample-chronicle.json"
);

/// Chronicle with a duplicate UUID (error) and an unknown category (warning).
const DUP_UUID: &str = r#"{
  "version": "1.0",
  "categories": [{ "id": "family", "label": "Family" }],
  "persons": [{
    "id": "alice", "name": "Alice",
    "facts": [
      { "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890", "date": "2020-01-01", "category": "family", "text": "Event 1" },
      { "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890", "date": "2020-01-02", "category": "family", "text": "Event 2" }
    ]
  }]
}"#;

/// Structurally fine chronicle with a warning-level issue only.
const UNKNOWN_CATEGORY: &str = r#"{
  "version": "1.0",
  "categories": [{ "id": "family", "label": "Family" }],
  "persons": [{
    "id": "alice", "name": "Alice",
    "facts": [
      { "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890", "date": "2020-01-01", "category": "travel", "text": "Event" }
    ]
  }]
}"#;

fn kinsaga() -> Command {
    let mut cmd = Command::cargo_bin("kinsaga").unwrap();
    cmd.env_remove("KINSAGA_INPUT");
    cmd
}

fn write_temp(json: &str) -> tempfile::NamedTempFile {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    fs::write(tmp.path(), json).unwrap();
    tmp
}

fn temp_copy_of_sample() -> tempfile::NamedTempFile {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    fs::copy(SAMPLE, tmp.path()).unwrap();
    tmp
}

#[test]
fn validate_clean_file_exits_zero_with_empty_stdout() {
    kinsaga()
        .args(["-i", SAMPLE, "validate"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn validate_duplicate_uuid_exits_nonzero() {
    let tmp = write_temp(DUP_UUID);
    kinsaga()
        .arg("-i")
        .arg(tmp.path())
        .arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Duplicate UUID"));
}

#[test]
fn validate_strict_fails_on_warnings_only() {
    let tmp = write_temp(UNKNOWN_CATEGORY);
    // Warnings alone are fine without --strict...
    kinsaga()
        .arg("-i")
        .arg(tmp.path())
        .arg("validate")
        .assert()
        .success();
    // ...but fail with it
    kinsaga()
        .arg("-i")
        .arg(tmp.path())
        .args(["validate", "--strict"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--strict"));
}

#[test]
fn validate_correct_stdout_is_pure_json_and_exits_zero() {
    let tmp = write_temp(DUP_UUID);
    let output = kinsaga()
        .arg("-i")
        .arg(tmp.path())
        .args(["validate", "--correct"])
        .output()
        .unwrap();

    // Corrections were applied, so the run counts as resolved
    assert!(output.status.success());

    // stdout must be nothing but the corrected chronicle JSON
    let stdout = String::from_utf8(output.stdout).unwrap();
    let corrected: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout must be valid JSON");
    assert!(corrected.get("persons").is_some());

    // The corrected JSON must itself validate cleanly
    let fixed = write_temp(&stdout);
    kinsaga()
        .arg("-i")
        .arg(fixed.path())
        .arg("validate")
        .assert()
        .success();
}

#[test]
fn validate_correct_emits_json_even_without_corrections() {
    // Redirect workflows must always yield a complete file
    let output = kinsaga()
        .args(["-i", SAMPLE, "validate", "--correct"])
        .output()
        .unwrap();
    assert!(output.status.success());
    serde_json::from_slice::<serde_json::Value>(&output.stdout)
        .expect("stdout must contain the full chronicle JSON");
}

#[test]
fn validate_correct_in_place_fixes_file() {
    let tmp = write_temp(DUP_UUID);
    kinsaga()
        .arg("-i")
        .arg(tmp.path())
        .args(["validate", "--correct", "--in-place"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
    // File on disk is fixed now
    kinsaga()
        .arg("-i")
        .arg(tmp.path())
        .arg("validate")
        .assert()
        .success();
}

#[test]
fn validate_in_place_requires_correct_or_apply() {
    kinsaga()
        .args(["-i", SAMPLE, "validate", "--in-place"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--in-place requires"));
}

#[test]
fn add_fact_roundtrip_with_quiet_stdout() {
    let tmp = temp_copy_of_sample();
    kinsaga()
        .arg("-i")
        .arg(tmp.path())
        .args([
            "add-fact",
            "alice",
            "-d",
            "2024-01-01",
            "-c",
            "family",
            "-t",
            "Integration test event",
        ])
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("added successfully"));

    let chronicle = kinsaga::load(tmp.path()).unwrap();
    let alice = chronicle.find_person("alice").unwrap();
    assert!(alice
        .facts
        .iter()
        .any(|f| f.text == "Integration test event"));
}

#[test]
fn add_fact_dry_run_does_not_modify_file() {
    let tmp = temp_copy_of_sample();
    let before = fs::read_to_string(tmp.path()).unwrap();
    kinsaga()
        .arg("-i")
        .arg(tmp.path())
        .args([
            "add-fact", "alice", "-d", "2024-01-01", "-c", "family", "-t", "Preview",
            "--dry-run",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Dry run"));
    assert_eq!(fs::read_to_string(tmp.path()).unwrap(), before);
}

#[test]
fn search_json_with_no_results_emits_empty_array() {
    kinsaga()
        .args(["-i", SAMPLE, "search", "zzz-no-match", "-f", "json"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("[]"));
}

#[test]
fn search_invalid_regex_reports_error() {
    kinsaga()
        .args(["-i", SAMPLE, "search", "[invalid", "--regex"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid regex"));
}

#[test]
fn search_csv_with_no_results_emits_header() {
    kinsaga()
        .args(["-i", SAMPLE, "search", "zzz-no-match", "-f", "csv"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with(
            "person_id,person_name,date,category,text,location,attachments",
        ));
}

#[test]
fn list_csv_outputs_header_and_rows() {
    kinsaga()
        .args(["-i", SAMPLE, "list", "-f", "csv"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("id,name,facts"))
        .stdout(predicate::str::contains("alice"));
}

#[test]
fn merge_dry_run_reports_on_stderr_only() {
    let target = temp_copy_of_sample();
    let source = temp_copy_of_sample();
    kinsaga()
        .arg("-i")
        .arg(target.path())
        .arg("merge")
        .arg(source.path())
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("Dry run"));
}

#[test]
fn schema_outputs_valid_json() {
    let output = kinsaga().arg("schema").output().unwrap();
    assert!(output.status.success());
    serde_json::from_slice::<serde_json::Value>(&output.stdout)
        .expect("schema must be valid JSON");
}

#[test]
fn missing_input_shows_helpful_error() {
    // Run from a temp dir so a developer's local .env can't interfere
    kinsaga()
        .current_dir(std::env::temp_dir())
        .arg("list")
        .assert()
        .failure()
        .stderr(predicate::str::contains("No chronicle file specified"));
}

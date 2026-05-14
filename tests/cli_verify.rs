use assert_cmd::Command;
use std::path::Path;

#[test]
fn test_verify_long() {
    let cargo_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

    let mut cmd = Command::cargo_bin("acta-records").unwrap();

    let expected_output =
        std::fs::read_to_string(cargo_dir.join("tests/fixtures/expected/cli_verify_long.txt"))
            .unwrap();

    let output = String::from_utf8(
        cmd.arg("verify")
            .arg("tests/fixtures/foo_foo2_record.json")
            .arg("--long")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap();

    assert_eq!(expected_output, output);
}

#[test]
fn test_verify_compact() {
    let cargo_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

    let mut cmd = Command::cargo_bin("acta-records").unwrap();

    let expected_output =
        std::fs::read_to_string(cargo_dir.join("tests/fixtures/expected/cli_verify_multi.txt"))
            .unwrap();

    let output = String::from_utf8(
        cmd.arg("verify")
            .arg("tests/fixtures/foo_record.json")
            .arg("tests/fixtures/foo_foo2_record.json")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap();

    assert_eq!(expected_output, output);
}

#[test]
fn test_verify_compact_stdin0() {
    let cargo_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

    let mut cmd = Command::cargo_bin("acta-records").unwrap();

    let expected_output =
        std::fs::read_to_string(cargo_dir.join("tests/fixtures/expected/cli_verify_multi.txt"))
            .unwrap();

    let output = String::from_utf8(
        cmd.arg("verify")
            .write_stdin("tests/fixtures/foo_record.json\0tests/fixtures/foo_foo2_record.json")
            .arg("--stdin0")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap();

    assert_eq!(expected_output, output);
}

#[test]
fn test_verify_compact_stdin_newline() {
    let cargo_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

    let mut cmd = Command::cargo_bin("acta-records").unwrap();

    let expected_output =
        std::fs::read_to_string(cargo_dir.join("tests/fixtures/expected/cli_verify_multi.txt"))
            .unwrap();

    let output = String::from_utf8(
        cmd.arg("verify")
            .write_stdin("tests/fixtures/foo_record.json\ntests/fixtures/foo_foo2_record.json")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap();

    assert_eq!(expected_output, output);
}

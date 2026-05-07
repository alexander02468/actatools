use assert_cmd::Command;
use std::path::Path;

#[test]
fn test_compare() {
    let cargo_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

    let mut cmd = Command::cargo_bin("acta-records").unwrap();

    let expected_output =
        std::fs::read_to_string(cargo_dir.join("tests/fixtures/expected/cli_compared_records.txt"))
            .unwrap();

    let output = String::from_utf8(
        cmd.arg("compare")
            .arg("tests/fixtures/expected/cli_record_root_rel_1.json")
            .arg("tests/fixtures/expected/cli_record_2.json")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap();

    assert_eq!(expected_output, output);
}

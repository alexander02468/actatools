// tests/cli_manifest.rs

use assert_cmd::{
    Command,
};
use std::path::Path;

use serde_json::Value;

fn normalize_record_json_for_test(mut value: Value) -> Value {
    if let Some(metadata) = value
        .get_mut("metadata")
        .and_then(|metadata| metadata.as_object_mut())
    {
        metadata.remove("generated_at_utc");
        metadata.remove("meta_digest");
    }
    value
}

#[test]
fn test_record_command_outputs_vector_input() {
    let cargo_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

    let expected_json = std::fs::read_to_string(
        cargo_dir.join("tests/fixtures/expected/cli_record_root_rel_1.json"),
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("acta-records").unwrap();

    let output = String::from_utf8(
        cmd.arg("record")
            .arg("tests/fixtures/foo.bar")
            .arg("tests/fixtures/foo2.bar")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .unwrap();

    let actual: Value = serde_json::from_str(&output).unwrap();
    let expected: Value = serde_json::from_str(&expected_json).unwrap();

    let actual = normalize_record_json_for_test(actual);
    let expected = normalize_record_json_for_test(expected);

    assert_eq!(actual, expected);
}

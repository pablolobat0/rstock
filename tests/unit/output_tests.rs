use serde::ser::{Error, Serializer};
use serde::Serialize;
use serde_json::{json, Value};

use super::write_json;

#[test]
fn writes_one_compact_envelope_with_empty_and_null_values() {
    let mut output = Vec::new();
    write_json(
        &mut output,
        "example.get",
        &json!({"items": [], "value": null}),
    )
    .expect("JSON should serialize");
    let text = String::from_utf8(output).expect("JSON should be UTF-8");
    assert_eq!(text.lines().count(), 1);
    assert!(!text.contains("\u{1b}["));
    let value: Value = serde_json::from_str(&text).expect("output should be valid JSON");
    assert_eq!(value["command"], "example.get");
    assert_eq!(value["data"]["items"], json!([]));
    assert!(value["data"]["value"].is_null());
}

#[test]
fn serialization_failure_writes_nothing() {
    let mut output = Vec::new();
    let error = write_json(&mut output, "example.get", &FailingValue)
        .expect_err("serialization should fail");
    assert!(error
        .to_string()
        .contains("failed to serialize JSON output"));
    assert!(output.is_empty());
}

struct FailingValue;

impl Serialize for FailingValue {
    fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Err(S::Error::custom("controlled failure"))
    }
}

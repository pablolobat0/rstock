use super::parse_price_history_response;

#[test]
fn empty_price_history_is_a_successful_empty_result() {
    let observations = parse_price_history_response(r#"[{"series":[]}]"#).unwrap();

    assert!(observations.is_empty());
}

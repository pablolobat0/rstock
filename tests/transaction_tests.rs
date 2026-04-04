use rstock::models::{cents_to_f64, f64_to_cents};

#[test]
fn test_cents_roundtrip() {
    assert_eq!(f64_to_cents(150.25), 1_502_500);
    assert_eq!(cents_to_f64(1_502_500), 150.25);
}

#[test]
fn test_f64_to_cents_rounds() {
    assert_eq!(f64_to_cents(1.0006), 10_006);
    assert_eq!(f64_to_cents(0.0), 0);
    assert_eq!(f64_to_cents(0.137), 1_370);
}

#[test]
fn test_cents_to_f64_negative() {
    assert_eq!(cents_to_f64(-50_000), -5.0);
}

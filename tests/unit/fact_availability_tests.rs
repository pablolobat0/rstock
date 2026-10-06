use super::FactAvailability;

#[test]
fn fact_availability_value_exposes_only_available_facts() {
    assert_eq!(FactAvailability::Available(7.0).value(), Some(&7.0));
    assert_eq!(FactAvailability::<f64>::Unavailable.value(), None);
    assert_eq!(FactAvailability::<f64>::NotApplicable.value(), None);
}

#[test]
fn fact_availability_states_are_distinguishable() {
    assert_ne!(
        FactAvailability::<f64>::Unavailable,
        FactAvailability::NotApplicable
    );
    assert_ne!(
        FactAvailability::Available(0.0),
        FactAvailability::<f64>::Unavailable
    );
}

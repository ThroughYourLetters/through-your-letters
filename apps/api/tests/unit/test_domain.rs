use api::domain::{
    lettering::value_objects::{ContributorTag, PinCode},
    shared::pagination::PaginationRequest,
};

#[test]
fn pin_code_accepts_international_formats_and_trims_whitespace() {
    for value in ["560001", "123456", "10001", "SW1A 1AA", "10115"] {
        let pin = PinCode::new(format!("  {value}  ")).expect("valid postal code");
        assert_eq!(pin.value, value);
    }
}

#[test]
fn pin_code_enforces_required_value_and_length_limit() {
    assert!(PinCode::new(String::new()).is_err());
    assert!(PinCode::new(" \t ".to_string()).is_err());
    assert!(PinCode::new("A".repeat(20)).is_ok());
    assert!(PinCode::new("A".repeat(21)).is_err());
}

#[test]
fn contributor_tag_enforces_length_bounds() {
    assert!(ContributorTag::new("abc".to_string()).is_ok());
    assert!(ContributorTag::new("a".to_string()).is_err());
    assert!(ContributorTag::new("ab".to_string()).is_err());
    assert!(ContributorTag::new("a".repeat(31)).is_err());
}

#[test]
fn pagination_defaults_are_safe_and_stable() {
    let p = PaginationRequest::default();
    assert_eq!(p.limit, 50);
    assert_eq!(p.offset, 0);
}

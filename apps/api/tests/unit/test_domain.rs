use api::domain::{
    lettering::value_objects::{ContributorTag, PinCode},
    shared::pagination::PaginationRequest,
};

#[test]
fn pin_code_accepts_expected_format() {
    let pin = PinCode::new("560001".to_string());
    assert!(pin.is_ok(), "expected 560001 to be valid");
}

#[test]
fn pin_code_rejects_wrong_prefix_or_length() {
    assert!(PinCode::new("123456".to_string()).is_err());
    assert!(PinCode::new("56001".to_string()).is_err());
    assert!(PinCode::new("5600011".to_string()).is_err());
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

use api::domain::{
    lettering::value_objects::{ContributorTag, PinCode},
    shared::pagination::PaginationRequest,
};

#[path = "../support/postal_cases.rs"]
mod postal_cases;

#[test]
fn postal_codes_accept_international_formats_and_reject_invalid_lengths() {
    for case in postal_cases::cases() {
        let parsed = PinCode::new(case.input).ok().map(|pin| pin.value);
        assert_eq!(parsed, case.normalized, "{}", case.name);
    }
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

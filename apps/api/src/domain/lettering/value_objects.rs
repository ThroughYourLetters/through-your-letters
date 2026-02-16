use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PinCode {
    pub value: String,
}

impl PinCode {
    /// Creates a new PinCode, validating it matches the pattern: 56xxxx (Bengaluru pin codes).
    pub fn new(value: String) -> Result<Self, String> {
        // Validate: must be exactly 6 digits starting with 56 (Bengaluru PIN codes)
        if value.len() == 6 && value.starts_with("56") && value.chars().all(|c| c.is_ascii_digit()) {
            Ok(Self { value })
        } else {
            Err("PIN code must be 6 digits starting with 56 (Bengaluru area)".to_string())
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ContributorTag {
    #[validate(length(min = 3, max = 30))]
    pub value: String,
}

impl ContributorTag {
    pub fn new(value: String) -> Result<Self, validator::ValidationErrors> {
        let tag = Self { value };
        tag.validate()?;
        Ok(tag)
    }
}

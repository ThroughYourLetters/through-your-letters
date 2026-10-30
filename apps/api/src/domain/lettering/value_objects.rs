use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PinCode {
    pub value: String,
}

impl PinCode {
    /// Trims Unicode whitespace and accepts 1–20 characters in any postal format.
    pub fn new(value: String) -> Result<Self, String> {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            Err("Postal code cannot be empty".to_string())
        } else if trimmed.chars().count() > 20 {
            Err("Postal code must be 20 characters or less".to_string())
        } else {
            Ok(Self { value: trimmed })
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

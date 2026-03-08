use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PinCode {
    pub value: String,
}

impl PinCode {
    /// Creates a new PinCode. Accepts any non-empty postal/zip code to support global formats
    /// (e.g. "560001" India, "10001" US, "SW1A 1AA" UK, "10115" Germany).
    pub fn new(value: String) -> Result<Self, String> {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            Err("Postal code cannot be empty".to_string())
        } else if trimmed.len() > 20 {
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

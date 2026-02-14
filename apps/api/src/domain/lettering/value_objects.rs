use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use validator::Validate;

lazy_static! {
    static ref PIN_CODE_REGEX: regex::Regex = regex::Regex::new(r"^56\d{4}$").unwrap();
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct PinCode {
    #[validate(regex(path = *PIN_CODE_REGEX))]
    pub value: String,
}

impl PinCode {
    pub fn new(value: String) -> Result<Self, validator::ValidationErrors> {
        let pin_code = Self { value };
        pin_code.validate()?;
        Ok(pin_code)
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

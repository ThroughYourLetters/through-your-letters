use super::{entity::Coordinates, errors::DomainError};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A contributor's estimate of where the photograph was taken.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ChosenLocation {
    pub latitude: f64,
    pub longitude: f64,
    pub place_name: String,
    /// Estimated radius in metres; absent when the contributor does not know.
    pub accuracy_m: Option<f64>,
}

impl ChosenLocation {
    pub fn validate(mut self) -> Result<Self, DomainError> {
        if !self.coordinates().is_valid() {
            return Err(DomainError::ValidationError(
                "Location coordinates are outside the allowed range".into(),
            ));
        }
        self.place_name = self.place_name.trim().to_owned();
        if !(1..=200).contains(&self.place_name.chars().count())
            || self.place_name.chars().any(char::is_control)
        {
            return Err(DomainError::ValidationError(
                "Place name must contain 1 to 200 characters without control characters".into(),
            ));
        }
        if self
            .accuracy_m
            .is_some_and(|value| !value.is_finite() || !(1.0..=1_000_000.0).contains(&value))
        {
            return Err(DomainError::ValidationError(
                "Location accuracy must be between 1 and 1000000 metres, or unknown".into(),
            ));
        }
        Ok(self)
    }

    pub fn coordinates(&self) -> Coordinates {
        Coordinates::new_point(self.longitude, self.latitude)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[serde(
    tag = "action",
    content = "point",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum LocationEdit {
    Set(ChosenLocation),
    Clear,
}

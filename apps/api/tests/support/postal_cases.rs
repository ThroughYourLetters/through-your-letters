#[derive(serde::Deserialize)]
pub struct PostalCodeCase {
    pub name: String,
    pub input: String,
    pub normalized: Option<String>,
}

pub fn cases() -> Vec<PostalCodeCase> {
    serde_json::from_str(include_str!(
        "../../../../packages/validation/fixtures/postal-codes.json"
    ))
    .unwrap()
}

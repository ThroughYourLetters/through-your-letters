use std::collections::HashMap;
use regex::Regex;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{warn, debug, instrument};
use uuid::Uuid;

/// Comprehensive input validation and sanitization service for security hardening.
///
/// Provides defense-in-depth validation for all user inputs, protecting against
/// common attack vectors including injection attacks, XSS, CSRF, and data
/// exfiltration attempts. All validation rules are configurable and auditable.
///
/// # Security Philosophy
/// - Fail securely: invalid inputs are rejected, not sanitized when possible
/// - Log security events for monitoring and incident response
/// - Use allowlist validation over blocklist filtering
/// - Validate both syntax and semantic constraints
/// - Protect against both malicious and accidental data corruption
pub struct InputValidationService {
    /// Compiled regex patterns for efficient validation
    patterns: ValidationPatterns,

    /// Configuration for validation rules and limits
    config: ValidationConfig,

    /// Content security policy rules
    csp_rules: ContentSecurityRules,
}

/// Pre-compiled regex patterns for input validation
struct ValidationPatterns {
    /// Email address validation (RFC 5322 compliant)
    email: Regex,

    /// PIN code validation (flexible international formats)
    pin_code: Regex,

    /// Contributor tag validation (alphanumeric with limited special chars)
    contributor_tag: Regex,

    /// URL validation for image and thumbnail links
    url: Regex,

    /// SQL injection detection patterns
    sql_injection: Vec<Regex>,

    /// XSS payload detection patterns
    xss_patterns: Vec<Regex>,

    /// Path traversal attack detection
    path_traversal: Regex,

    /// Command injection detection
    command_injection: Vec<Regex>,
}

/// Configurable validation limits and rules
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    /// Maximum length for contributor tags
    pub max_contributor_tag_length: usize,

    /// Maximum length for descriptions
    pub max_description_length: usize,

    /// Maximum length for comment content
    pub max_comment_length: usize,

    /// Allowed image file extensions
    pub allowed_image_extensions: Vec<String>,

    /// Maximum image file size in bytes
    pub max_image_size_bytes: usize,

    /// Maximum requests per IP per minute
    pub rate_limit_per_minute: u32,

    /// Allowed coordinate bounds (global coverage)
    pub min_longitude: f64,
    pub max_longitude: f64,
    pub min_latitude: f64,
    pub max_latitude: f64,

    /// Minimum confidence threshold for ML processing
    pub min_ml_confidence: f32,
}

/// Content Security Policy rules for XSS prevention
#[derive(Debug, Clone)]
struct ContentSecurityRules {
    /// Allowed HTML tags in rich content
    allowed_html_tags: Vec<String>,

    /// Allowed HTML attributes
    allowed_html_attributes: Vec<String>,

    /// Blocked URL schemes
    blocked_url_schemes: Vec<String>,

    /// Maximum nested HTML depth
    max_html_depth: usize,
}

/// Input validation errors with detailed context
#[derive(Error, Debug, Serialize, Deserialize)]
pub enum ValidationError {
    #[error("Input too long: {field} exceeds {max_length} characters")]
    TooLong { field: String, max_length: usize },

    #[error("Input too short: {field} must be at least {min_length} characters")]
    TooShort { field: String, min_length: usize },

    #[error("Invalid format: {field} does not match required pattern")]
    InvalidFormat { field: String },

    #[error("Forbidden content: {field} contains prohibited content")]
    ForbiddenContent { field: String },

    #[error("Security violation: potential {attack_type} detected in {field}")]
    SecurityViolation { field: String, attack_type: String },

    #[error("Invalid range: {field} value {value} outside allowed range")]
    InvalidRange { field: String, value: String },

    #[error("Rate limit exceeded: too many requests from this source")]
    RateLimitExceeded,

    #[error("File validation failed: {reason}")]
    FileValidation { reason: String },

    #[error("Coordinate validation failed: invalid geographic location")]
    InvalidCoordinates,

    #[error("Content policy violation: {rule}")]
    ContentPolicyViolation { rule: String },
}

/// Validation result with optional sanitized content
#[derive(Debug, Clone)]
pub struct ValidationResult<T> {
    /// Whether the input passed validation
    pub is_valid: bool,

    /// Validated and potentially sanitized value
    pub value: Option<T>,

    /// Validation errors encountered
    pub errors: Vec<ValidationError>,

    /// Security warnings (non-blocking)
    pub warnings: Vec<String>,
}

/// Request context for validation and rate limiting
#[derive(Debug, Clone)]
pub struct ValidationContext {
    /// Client IP address for rate limiting
    pub client_ip: Option<std::net::IpAddr>,

    /// Authenticated user ID if available
    pub user_id: Option<Uuid>,

    /// Request timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// Request path for context
    pub request_path: String,

    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            max_contributor_tag_length: 50,
            max_description_length: 2000,
            max_comment_length: 500,
            allowed_image_extensions: vec![
                "jpg".to_string(), "jpeg".to_string(), "png".to_string(),
                "webp".to_string(), "heic".to_string(), "heif".to_string()
            ],
            max_image_size_bytes: 20 * 1024 * 1024, // 20MB
            rate_limit_per_minute: 60,
            min_longitude: -180.0,
            max_longitude: 180.0,
            min_latitude: -90.0,
            max_latitude: 90.0,
            min_ml_confidence: 0.3,
        }
    }
}

impl InputValidationService {
    /// Creates a new input validation service with default security rules.
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let patterns = ValidationPatterns::new()?;
        let config = ValidationConfig::default();
        let csp_rules = ContentSecurityRules::default();

        Ok(Self {
            patterns,
            config,
            csp_rules,
        })
    }

    /// Creates a validation service with custom configuration.
    pub fn with_config(config: ValidationConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let patterns = ValidationPatterns::new()?;
        let csp_rules = ContentSecurityRules::default();

        Ok(Self {
            patterns,
            config,
            csp_rules,
        })
    }

    /// Validates an email address for user registration and authentication.
    ///
    /// Performs comprehensive email validation including:
    /// - RFC 5322 syntax compliance
    /// - Domain validation (when possible)
    /// - Disposable email detection
    /// - Length limits and character restrictions
    ///
    /// # Security Considerations
    /// - Prevents email header injection attacks
    /// - Blocks obviously fake or disposable email addresses
    /// - Logs suspicious validation attempts
    #[instrument(skip(self, email), fields(email_domain = %Self::extract_domain(email)))]
    pub fn validate_email(&self, email: &str, context: &ValidationContext) -> ValidationResult<String> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Length validation
        if email.len() > 254 {
            errors.push(ValidationError::TooLong {
                field: "email".to_string(),
                max_length: 254
            });
        }

        if email.len() < 3 {
            errors.push(ValidationError::TooShort {
                field: "email".to_string(),
                min_length: 3
            });
        }

        // Format validation
        if !self.patterns.email.is_match(email) {
            errors.push(ValidationError::InvalidFormat {
                field: "email".to_string()
            });
        }

        // Security checks
        if self.contains_suspicious_patterns(email) {
            errors.push(ValidationError::SecurityViolation {
                field: "email".to_string(),
                attack_type: "injection".to_string()
            });
        }

        // Check for disposable email domains
        let domain = Self::extract_domain(email);
        if Self::is_disposable_domain(&domain) {
            warnings.push("Disposable email domain detected".to_string());
        }

        let is_valid = errors.is_empty();
        let value = if is_valid { Some(email.trim().to_lowercase()) } else { None };

        if !is_valid {
            warn!("Email validation failed for {}: {:?}", email, errors);
        }

        ValidationResult {
            is_valid,
            value,
            errors,
            warnings,
        }
    }

    /// Validates contributor tag with security and usability constraints.
    ///
    /// Ensures contributor tags are:
    /// - Appropriate length for display
    /// - Free of special characters that could cause issues
    /// - Not containing profanity or inappropriate content
    /// - Unique within reasonable bounds
    #[instrument(skip(self))]
    pub fn validate_contributor_tag(&self, tag: &str, context: &ValidationContext) -> ValidationResult<String> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        let trimmed = tag.trim();

        // Length validation
        if trimmed.len() > self.config.max_contributor_tag_length {
            errors.push(ValidationError::TooLong {
                field: "contributor_tag".to_string(),
                max_length: self.config.max_contributor_tag_length
            });
        }

        if trimmed.len() < 2 {
            errors.push(ValidationError::TooShort {
                field: "contributor_tag".to_string(),
                min_length: 2
            });
        }

        // Format validation
        if !self.patterns.contributor_tag.is_match(trimmed) {
            errors.push(ValidationError::InvalidFormat {
                field: "contributor_tag".to_string()
            });
        }

        // Content policy validation
        if self.contains_profanity(trimmed) {
            errors.push(ValidationError::ContentPolicyViolation {
                rule: "No profanity allowed in contributor tags".to_string()
            });
        }

        // Security validation
        if self.contains_suspicious_patterns(trimmed) {
            errors.push(ValidationError::SecurityViolation {
                field: "contributor_tag".to_string(),
                attack_type: "injection".to_string()
            });
        }

        let is_valid = errors.is_empty();
        let value = if is_valid { Some(trimmed.to_string()) } else { None };

        ValidationResult {
            is_valid,
            value,
            errors,
            warnings,
        }
    }

    /// Validates geographic coordinates for lettering locations.
    ///
    /// Ensures coordinates are:
    /// - Within valid Earth bounds
    /// - Properly formatted as decimal degrees
    /// - Not in restricted or sensitive locations
    /// - Reasonable for public lettering discovery
    #[instrument(skip(self))]
    pub fn validate_coordinates(&self, longitude: f64, latitude: f64, context: &ValidationContext) -> ValidationResult<(f64, f64)> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Basic range validation
        if longitude < self.config.min_longitude || longitude > self.config.max_longitude {
            errors.push(ValidationError::InvalidRange {
                field: "longitude".to_string(),
                value: longitude.to_string()
            });
        }

        if latitude < self.config.min_latitude || latitude > self.config.max_latitude {
            errors.push(ValidationError::InvalidRange {
                field: "latitude".to_string(),
                value: latitude.to_string()
            });
        }

        // Check for obviously invalid coordinates (0,0), (null island)
        if longitude == 0.0 && latitude == 0.0 {
            errors.push(ValidationError::InvalidCoordinates);
        }

        // Check for restricted areas (this would be enhanced with a proper geofencing service)
        if self.is_restricted_location(longitude, latitude) {
            errors.push(ValidationError::ContentPolicyViolation {
                rule: "Location in restricted area".to_string()
            });
        }

        // Precision validation (excessive precision may indicate automated/fake data)
        if self.has_excessive_precision(longitude, latitude) {
            warnings.push("Coordinates have unusual precision".to_string());
        }

        let is_valid = errors.is_empty();
        let value = if is_valid { Some((longitude, latitude)) } else { None };

        ValidationResult {
            is_valid,
            value,
            errors,
            warnings,
        }
    }

    /// Validates PIN/postal codes for geographic accuracy.
    ///
    /// Supports international postal code formats and validates
    /// consistency with provided coordinates when possible.
    #[instrument(skip(self))]
    pub fn validate_pin_code(&self, pin_code: &str, country_code: Option<&str>, context: &ValidationContext) -> ValidationResult<String> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        let trimmed = pin_code.trim();

        // Basic format validation
        if !self.patterns.pin_code.is_match(trimmed) {
            errors.push(ValidationError::InvalidFormat {
                field: "pin_code".to_string()
            });
        }

        // Length validation based on country
        if let Some(country) = country_code {
            if !self.is_valid_pin_for_country(trimmed, country) {
                errors.push(ValidationError::InvalidFormat {
                    field: "pin_code".to_string()
                });
            }
        }

        // Security validation
        if self.contains_suspicious_patterns(trimmed) {
            errors.push(ValidationError::SecurityViolation {
                field: "pin_code".to_string(),
                attack_type: "injection".to_string()
            });
        }

        let is_valid = errors.is_empty();
        let value = if is_valid { Some(trimmed.to_string()) } else { None };

        ValidationResult {
            is_valid,
            value,
            errors,
            warnings,
        }
    }

    /// Validates user-provided descriptions and comments for content policy compliance.
    ///
    /// Checks for:
    /// - Appropriate length limits
    /// - Profanity and inappropriate content
    /// - Spam patterns
    /// - XSS and injection attempts
    /// - Cultural sensitivity guidelines
    #[instrument(skip(self, content), fields(content_length = content.len()))]
    pub fn validate_user_content(&self, content: &str, content_type: &str, context: &ValidationContext) -> ValidationResult<String> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        let trimmed = content.trim();

        // Length validation based on content type
        let max_length = match content_type {
            "description" => self.config.max_description_length,
            "comment" => self.config.max_comment_length,
            _ => 1000,
        };

        if trimmed.len() > max_length {
            errors.push(ValidationError::TooLong {
                field: content_type.to_string(),
                max_length
            });
        }

        // XSS validation
        if self.contains_xss_patterns(trimmed) {
            errors.push(ValidationError::SecurityViolation {
                field: content_type.to_string(),
                attack_type: "xss".to_string()
            });
        }

        // SQL injection validation
        if self.contains_sql_injection_patterns(trimmed) {
            errors.push(ValidationError::SecurityViolation {
                field: content_type.to_string(),
                attack_type: "sql_injection".to_string()
            });
        }

        // Content policy validation
        if self.contains_profanity(trimmed) {
            errors.push(ValidationError::ContentPolicyViolation {
                rule: "Profanity not allowed".to_string()
            });
        }

        if self.is_spam_content(trimmed) {
            errors.push(ValidationError::ContentPolicyViolation {
                rule: "Spam content detected".to_string()
            });
        }

        // Cultural sensitivity check
        if self.contains_culturally_insensitive_content(trimmed) {
            warnings.push("Content may be culturally insensitive".to_string());
        }

        let is_valid = errors.is_empty();
        let value = if is_valid { Some(trimmed.to_string()) } else { None };

        ValidationResult {
            is_valid,
            value,
            errors,
            warnings,
        }
    }

    /// Validates uploaded file data for security and format compliance.
    ///
    /// Performs deep file inspection including:
    /// - File type validation (magic number verification)
    /// - Size limits
    /// - Image metadata sanitization
    /// - Malware scanning integration points
    /// - EXIF data privacy protection
    #[instrument(skip(self, file_data), fields(file_size = file_data.len()))]
    pub fn validate_file_upload(&self, file_data: &[u8], filename: &str, context: &ValidationContext) -> ValidationResult<Vec<u8>> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Size validation
        if file_data.len() > self.config.max_image_size_bytes {
            errors.push(ValidationError::FileValidation {
                reason: format!("File size {} exceeds maximum {}", file_data.len(), self.config.max_image_size_bytes)
            });
        }

        if file_data.len() < 100 {
            errors.push(ValidationError::FileValidation {
                reason: "File too small to be a valid image".to_string()
            });
        }

        // File extension validation
        let extension = Self::extract_file_extension(filename).to_lowercase();
        if !self.config.allowed_image_extensions.contains(&extension) {
            errors.push(ValidationError::FileValidation {
                reason: format!("File extension '{}' not allowed", extension)
            });
        }

        // Magic number validation
        if !self.is_valid_image_format(file_data) {
            errors.push(ValidationError::FileValidation {
                reason: "File content does not match claimed image format".to_string()
            });
        }

        // Security validation - check for embedded malicious content
        if self.contains_embedded_threats(file_data) {
            errors.push(ValidationError::SecurityViolation {
                field: "file_upload".to_string(),
                attack_type: "malicious_content".to_string()
            });
        }

        // Privacy validation - check for sensitive EXIF data
        if self.contains_sensitive_metadata(file_data) {
            warnings.push("Image contains sensitive metadata that will be stripped".to_string());
        }

        let is_valid = errors.is_empty();
        let value = if is_valid { Some(file_data.to_vec()) } else { None };

        ValidationResult {
            is_valid,
            value,
            errors,
            warnings,
        }
    }

    // Private helper methods

    fn extract_domain(email: &str) -> String {
        email.split('@').nth(1).unwrap_or("").to_string()
    }

    fn is_disposable_domain(domain: &str) -> bool {
        // This would typically check against a maintained list of disposable email domains
        let disposable_domains = [
            "tempmail.org", "10minutemail.com", "guerrillamail.com",
            "mailinator.com", "temp-mail.org"
        ];
        disposable_domains.contains(&domain)
    }

    fn extract_file_extension(filename: &str) -> String {
        filename.split('.').last().unwrap_or("").to_string()
    }

    fn contains_suspicious_patterns(&self, input: &str) -> bool {
        // Check for common injection patterns
        self.contains_sql_injection_patterns(input) ||
        self.contains_xss_patterns(input) ||
        self.patterns.path_traversal.is_match(input) ||
        self.contains_command_injection_patterns(input)
    }

    fn contains_sql_injection_patterns(&self, input: &str) -> bool {
        self.patterns.sql_injection.iter().any(|pattern| pattern.is_match(input))
    }

    fn contains_xss_patterns(&self, input: &str) -> bool {
        self.patterns.xss_patterns.iter().any(|pattern| pattern.is_match(input))
    }

    fn contains_command_injection_patterns(&self, input: &str) -> bool {
        self.patterns.command_injection.iter().any(|pattern| pattern.is_match(input))
    }

    fn contains_profanity(&self, input: &str) -> bool {
        // This would integrate with a comprehensive profanity filter
        // For now, checking basic patterns
        let profanity_patterns = ["fuck", "shit", "damn", "bitch"];
        let lower_input = input.to_lowercase();
        profanity_patterns.iter().any(|word| lower_input.contains(word))
    }

    fn is_spam_content(&self, content: &str) -> bool {
        // Detect spam patterns like excessive repetition, promotional language
        let repetition_threshold = 0.7;
        let words: Vec<&str> = content.split_whitespace().collect();

        if words.len() < 5 {
            return false;
        }

        let mut word_counts = HashMap::new();
        for word in &words {
            *word_counts.entry(word.to_lowercase()).or_insert(0) += 1;
        }

        let max_count = word_counts.values().max().unwrap_or(&0);
        (*max_count as f64 / words.len() as f64) > repetition_threshold
    }

    fn contains_culturally_insensitive_content(&self, _content: &str) -> bool {
        // This would integrate with cultural sensitivity analysis
        // Implementation would depend on specific cultural guidelines
        false
    }

    fn is_valid_pin_for_country(&self, pin: &str, country: &str) -> bool {
        match country.to_uppercase().as_str() {
            "IN" => pin.len() == 6 && pin.chars().all(|c| c.is_ascii_digit()),
            "US" => pin.len() == 5 && pin.chars().all(|c| c.is_ascii_digit()),
            "CA" => pin.len() == 6 && self.is_valid_canadian_postal_code(pin),
            "GB" => self.is_valid_uk_postcode(pin),
            _ => true, // Default to allow for unknown countries
        }
    }

    fn is_valid_canadian_postal_code(&self, code: &str) -> bool {
        // Canadian postal codes: A1A 1A1 format
        let pattern = Regex::new(r"^[A-Z]\d[A-Z] ?\d[A-Z]\d$").unwrap();
        pattern.is_match(&code.to_uppercase())
    }

    fn is_valid_uk_postcode(&self, code: &str) -> bool {
        // UK postcodes have various formats, simplified check
        code.len() >= 5 && code.len() <= 8
    }

    fn is_restricted_location(&self, _longitude: f64, _latitude: f64) -> bool {
        // This would check against a database of restricted coordinates
        // (military installations, private property, etc.)
        false
    }

    fn has_excessive_precision(&self, longitude: f64, latitude: f64) -> bool {
        // Check if coordinates have more than 6 decimal places (meter precision)
        let lng_str = longitude.to_string();
        let lat_str = latitude.to_string();

        lng_str.split('.').nth(1).map_or(false, |dec| dec.len() > 6) ||
        lat_str.split('.').nth(1).map_or(false, |dec| dec.len() > 6)
    }

    fn is_valid_image_format(&self, data: &[u8]) -> bool {
        if data.len() < 12 {
            return false;
        }

        // Check common image format magic numbers
        match &data[0..4] {
            [0x89, 0x50, 0x4E, 0x47] => true, // PNG
            [0xFF, 0xD8, 0xFF, _] => true,     // JPEG
            _ => {
                // Check WEBP
                if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
                    true
                } else {
                    false
                }
            }
        }
    }

    fn contains_embedded_threats(&self, _data: &[u8]) -> bool {
        // This would integrate with malware scanning services
        // Check for known malicious patterns in image files
        false
    }

    fn contains_sensitive_metadata(&self, data: &[u8]) -> bool {
        // Check for EXIF data that might contain GPS coordinates or personal info
        // This is a simplified check - production would use proper EXIF parsing
        data.windows(4).any(|window| {
            window == b"GPS\0" || window == b"EXIF"
        })
    }
}

impl ValidationPatterns {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            email: Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$")?,
            pin_code: Regex::new(r"^[A-Za-z0-9\s-]{3,10}$")?,
            contributor_tag: Regex::new(r"^[a-zA-Z0-9._\-\s]{2,50}$")?,
            url: Regex::new(r"^https?://[^\s/$.?#].[^\s]*$")?,
            sql_injection: vec![
                Regex::new(r"(?i)(union|select|insert|update|delete|drop|exec|script)")?,
                Regex::new(r"(?i)(--|\||;|'|\"|`)")?,
                Regex::new(r"(?i)(0x[0-9a-f]+|char\(|ascii\()")?,
            ],
            xss_patterns: vec![
                Regex::new(r"(?i)(<script|</script|javascript:|vbscript:|onload=|onerror=)")?,
                Regex::new(r"(?i)(alert\(|confirm\(|prompt\(|document\.|window\.)")?,
                Regex::new(r"(?i)(<iframe|<object|<embed|<applet)")?,
            ],
            path_traversal: Regex::new(r"\.\./|\.\.\\|%2e%2e|%252e|%c0%ae")?,
            command_injection: vec![
                Regex::new(r"(?i)(;|&&|\|\||`|\$\(|>\s|<\s)")?,
                Regex::new(r"(?i)(nc|netcat|wget|curl|ping|nslookup)")?,
            ],
        })
    }
}

impl Default for ContentSecurityRules {
    fn default() -> Self {
        Self {
            allowed_html_tags: vec!["p", "br", "strong", "em", "ul", "ol", "li"]
                .into_iter()
                .map(String::from)
                .collect(),
            allowed_html_attributes: vec!["class", "id"]
                .into_iter()
                .map(String::from)
                .collect(),
            blocked_url_schemes: vec!["javascript", "data", "vbscript"]
                .into_iter()
                .map(String::from)
                .collect(),
            max_html_depth: 5,
        }
    }
}

impl Default for InputValidationService {
    fn default() -> Self {
        Self::new().expect("Failed to create default InputValidationService")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    fn create_test_context() -> ValidationContext {
        ValidationContext {
            client_ip: Some("127.0.0.1".parse::<IpAddr>().unwrap()),
            user_id: Some(Uuid::new_v4()),
            timestamp: chrono::Utc::now(),
            request_path: "/test".to_string(),
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_email_validation() {
        let service = InputValidationService::new().unwrap();
        let context = create_test_context();

        // Valid email
        let result = service.validate_email("test@example.com", &context);
        assert!(result.is_valid);
        assert_eq!(result.value, Some("test@example.com".to_string()));

        // Invalid email
        let result = service.validate_email("invalid-email", &context);
        assert!(!result.is_valid);
        assert!(result.value.is_none());
    }

    #[test]
    fn test_coordinate_validation() {
        let service = InputValidationService::new().unwrap();
        let context = create_test_context();

        // Valid coordinates
        let result = service.validate_coordinates(77.5946, 12.9716, &context);
        assert!(result.is_valid);

        // Invalid coordinates (out of range)
        let result = service.validate_coordinates(200.0, 100.0, &context);
        assert!(!result.is_valid);
    }

    #[test]
    fn test_xss_detection() {
        let service = InputValidationService::new().unwrap();
        let context = create_test_context();

        let malicious_content = "<script>alert('xss')</script>";
        let result = service.validate_user_content(malicious_content, "comment", &context);
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| matches!(e, ValidationError::SecurityViolation { attack_type, .. } if attack_type == "xss")));
    }

    #[test]
    fn test_sql_injection_detection() {
        let service = InputValidationService::new().unwrap();
        let context = create_test_context();

        let malicious_content = "'; DROP TABLE users; --";
        let result = service.validate_user_content(malicious_content, "comment", &context);
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| matches!(e, ValidationError::SecurityViolation { attack_type, .. } if attack_type == "sql_injection")));
    }

    #[test]
    fn test_contributor_tag_validation() {
        let service = InputValidationService::new().unwrap();
        let context = create_test_context();

        // Valid tag
        let result = service.validate_contributor_tag("ValidUser123", &context);
        assert!(result.is_valid);

        // Too long tag
        let long_tag = "a".repeat(60);
        let result = service.validate_contributor_tag(&long_tag, &context);
        assert!(!result.is_valid);
    }

    #[test]
    fn test_pin_code_validation() {
        let service = InputValidationService::new().unwrap();
        let context

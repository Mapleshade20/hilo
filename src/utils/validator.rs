//! # Text Input Validation Utilities
//!
//! This module provides validation utilities for user input, particularly
//! email address validation with configurable domain restrictions and
//! grade validation with allowed values.

use std::env;
use std::sync::LazyLock;

use regex::Regex;

/// Email validation regex pattern
///
/// This regex validates email addresses against a list of allowed domains
/// specified in the `ALLOWED_DOMAINS` environment variable. The domains
/// are separated by colons and are properly escaped for regex use.
///
/// # Environment Variables
///
/// - `ALLOWED_DOMAINS` - Colon-separated list of allowed email domains
///   (e.g., "example.com:university.edu")
///
/// # Examples
///
/// For `ALLOWED_DOMAINS="mails.tsinghua.edu.cn"`:
/// - `user@mails.tsinghua.edu.cn` ✓ Valid
/// - `user@gmail.com` ✗ Invalid
/// - `invalid-email` ✗ Invalid format
pub static EMAIL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    let allowed_domains_str =
        env::var("ALLOWED_DOMAINS").expect("Env variable `ALLOWED_DOMAINS` should be set");

    let escaped_domains: Vec<String> = allowed_domains_str
        .split(':')
        .map(regex::escape) // encode special chars like period
        .collect();
    let domains_pattern = escaped_domains.join("|");
    let pattern = format!(r"^[a-zA-Z0-9._%+-]+@({domains_pattern})$");

    // info!("Using email regex pattern: {}", pattern);

    Regex::new(&pattern).expect("Failed to compile email regex")
});

/// Validates if a grade is allowed based on the ALLOWED_GRADES environment variable.
///
/// # Arguments
///
/// * `grade` - The grade string to validate
///
/// # Returns
///
/// * `Ok(())` if the grade is valid
/// * `Err(String)` with error message if invalid
///
/// # Examples
///
/// For `ALLOWED_GRADES="undergraduate:graduate"`:
/// - `validate_grade("undergraduate")` ✓ Valid
/// - `validate_grade("graduate")` ✓ Valid
/// - `validate_grade("phd")` ✗ Invalid
pub fn validate_grade(grade: &str) -> Result<(), String> {
    let allowed_grades_str = env::var("ALLOWED_GRADES")
        .map_err(|_| "ALLOWED_GRADES environment variable not set".to_string())?;

    let allowed_grades: Vec<&str> = allowed_grades_str.split(':').collect();

    if allowed_grades.contains(&grade) {
        Ok(())
    } else {
        Err(format!(
            "Invalid grade. Allowed grades: {allowed_grades_str}"
        ))
    }
}

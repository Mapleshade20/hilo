use std::env;
use std::sync::LazyLock;

use regex::Regex;
use tracing::error;

use crate::models::TagSystem;

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
    let allowed_domains_str = env::var("ALLOWED_DOMAINS").unwrap_or_else(|_| {
        error!("Missing ALLOWED_DOMAINS env var, using fallback 'mails.tsinghua.edu.cn'");
        "mails.tsinghua.edu.cn".to_string()
    });

    let escaped_domains: Vec<String> = allowed_domains_str
        .split(':')
        .map(regex::escape) // encode special chars like period
        .collect();
    let domains_pattern = escaped_domains.join("|");
    let pattern = format!(r"^[a-zA-Z0-9._%+-]+@({domains_pattern})$");

    Regex::new(&pattern).unwrap_or_else(|e| {
        error!("Failed to compile email regex: {}", e);
        std::process::exit(1);
    })
});

pub static TAG_SYSTEM: LazyLock<TagSystem> = LazyLock::new(|| {
    let raw = std::fs::read_to_string("tags.json").unwrap_or_else(|_| {
        error!("Failed to read tags.json file");
        std::process::exit(1);
    });
    TagSystem::from_json(&raw).unwrap_or_else(|e| {
        error!("Failed to parse tags.json: {}", e);
        std::process::exit(1)
    })
});

pub static TAG_SCORE_DECAY_FACTOR: LazyLock<f64> = LazyLock::new(|| {
    env::var("TAG_SCORE_DECAY_FACTOR")
        .ok()
        .and_then(|val| val.parse().ok())
        .unwrap_or_else(|| {
            error!("Invalid or missing TAG_SCORE_DECAY_FACTOR env var, using fallback 0.5");
            0.5
        })
});

pub static COMPLEMENTARY_TAG_WEIGHT: LazyLock<f64> = LazyLock::new(|| {
    env::var("COMPLEMENTARY_TAG_WEIGHT")
        .ok()
        .and_then(|val| val.parse().ok())
        .unwrap_or_else(|| {
            error!("Invalid or missing COMPLEMENTARY_TAG_WEIGHT env var, using fallback 0.8");
            0.8
        })
});

pub static TRAIT_MATCH_POINTS: LazyLock<f64> = LazyLock::new(|| {
    env::var("TRAIT_MATCH_POINTS")
        .ok()
        .and_then(|val| val.parse().ok())
        .unwrap_or_else(|| {
            error!("Invalid or missing TRAIT_MATCH_POINTS env var, using fallback 2.0");
            2.0
        })
});

pub static TOTAL_TAGS: LazyLock<usize> = LazyLock::new(|| {
    env::var("TOTAL_TAGS")
        .ok()
        .and_then(|val| val.parse::<usize>().ok())
        .unwrap_or_else(|| {
            error!("Invalid or missing TOTAL_TAGS env var, using fallback value 10");
            10
        })
});

pub static MATCH_PREVIEW_INTERVAL_MINUTES: LazyLock<u64> = LazyLock::new(|| {
    env::var("MATCH_PREVIEW_INTERVAL_MINUTES")
        .ok()
        .and_then(|val| val.parse().ok())
        .unwrap_or_else(|| {
            error!("Invalid or missing MATCH_PREVIEW_INTERVAL_MINUTES env var, using fallback 20");
            20
        })
});

pub static UPLOAD_DIR: LazyLock<String> = LazyLock::new(|| {
    env::var("UPLOAD_DIR").unwrap_or_else(|_| {
        error!("Missing UPLOAD_DIR env var, using fallback './uploads'");
        "./uploads".to_string()
    })
});

pub static ALLOWED_GRADES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    let allowed_grades_str = env::var("ALLOWED_GRADES")
        .unwrap_or_else(|_| {
            error!("Missing ALLOWED_GRADES env var, using fallback 'undergraduate:graduate'");
            "undergraduate:graduate".to_string()
        })
        .leak();

    allowed_grades_str.split(':').collect()
});

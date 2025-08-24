use std::env;
use std::sync::LazyLock;

use regex::Regex;

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

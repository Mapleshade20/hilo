use std::time::Duration;

pub const EMAIL_RATE_LIMIT: Duration = Duration::from_secs(180);
pub const VERIFICATION_CODE_EXPIRY: Duration = Duration::from_secs(300);
pub const CODE_CACHE_CAPACITY: usize = 300;

pub const ACCESS_TOKEN_EXPIRY: Duration = Duration::from_secs(15 * 60); // 15 minutes
pub const REFRESH_TOKEN_EXPIRY: Duration = Duration::from_secs(7 * 24 * 60 * 60); // 7 days

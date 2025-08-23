use std::time::Duration;

pub const EMAIL_RATE_LIMIT: Duration = Duration::from_secs(180);
pub const VERIFICATION_CODE_EXPIRY: Duration = Duration::from_secs(300);

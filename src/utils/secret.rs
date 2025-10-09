use std::{env, fs};

use tracing::error;

pub fn get_secret(file_env_var_name: &str, env_var_name: &str) -> Option<String> {
    if let Ok(secret_file_path) = env::var(file_env_var_name) {
        // Found a file path, try to read the file
        match fs::read_to_string(&secret_file_path) {
            Ok(content) => Some(content.trim().to_string()),
            Err(e) => {
                error!(%secret_file_path, ?e, "Error reading secret file");
                None
            }
        }
    } else {
        env::var(env_var_name).ok()
    }
}

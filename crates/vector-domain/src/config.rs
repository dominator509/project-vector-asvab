use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppConfig {
    pub app_name: String,
    pub environment: String,
    pub database_url: String,
    pub log_level: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            app_name: "Project VECTOR".to_string(),
            environment: "development".to_string(),
            database_url: "sqlite:vector.db".to_string(),
            log_level: "info".to_string(),
        }
    }
}

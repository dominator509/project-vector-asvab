#[cfg(test)]
mod tests {
    use crate::redaction::RedactedLogger;
    use serde_json::json;

    #[test]
    fn test_redaction_sanitizes_sensitive_keys() {
        let mut sample = json!({
            "user": "jules",
            "secret_token": "supersecret123",
            "nested": {
                "password": "mypassword",
                "normal_field": "hello"
            },
            "auth_key": "bearer-xyz"
        });

        RedactedLogger::redact_value(&mut sample);

        assert_eq!(sample["user"], "jules");
        assert_eq!(sample["secret_token"], "[REDACTED]");
        assert_eq!(sample["nested"]["password"], "[REDACTED]");
        assert_eq!(sample["nested"]["normal_field"], "hello");
        assert_eq!(sample["auth_key"], "[REDACTED]");
    }

    #[test]
    fn test_sanitize_json_str() {
        let raw_json = r#"{"db_host":"localhost","db_password":"my_secret_pass_value"}"#;
        let sanitized = RedactedLogger::sanitize_json_str(raw_json);
        assert!(sanitized.contains("[REDACTED]"));
        assert!(!sanitized.contains("my_secret_pass_value"));
    }
}

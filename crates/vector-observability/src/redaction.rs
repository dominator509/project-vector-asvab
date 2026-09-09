use serde_json::Value;

pub struct RedactedLogger;

impl RedactedLogger {
    pub fn redact_value(value: &mut Value) {
        match value {
            Value::Object(map) => {
                for (key, val) in map.iter_mut() {
                    let key_lower = key.to_lowercase();
                    if key_lower.contains("secret")
                        || key_lower.contains("password")
                        || key_lower.contains("token")
                        || key_lower.contains("key")
                        || key_lower.contains("auth")
                    {
                        *val = Value::String("[REDACTED]".to_string());
                    } else {
                        Self::redact_value(val);
                    }
                }
            }
            Value::Array(arr) => {
                for val in arr.iter_mut() {
                    Self::redact_value(val);
                }
            }
            _ => {}
        }
    }

    pub fn sanitize_json_str(input: &str) -> String {
        if let Ok(mut val) = serde_json::from_str::<Value>(input) {
            Self::redact_value(&mut val);
            serde_json::to_string(&val).unwrap_or_else(|_| "[REDACTED_ERROR]".to_string())
        } else {
            input.to_string()
        }
    }
}

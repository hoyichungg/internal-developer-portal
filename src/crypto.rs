use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

const ENCRYPTED_PREFIX: &str = "enc:v1:";
const REDACTED_SECRET: &str = "***redacted***";
const DEV_SECRET_KEY: &str = "development-only-connector-secret-key";

pub fn encrypt_connector_config(config: &str) -> Result<String, String> {
    let mut value = serde_json::from_str::<Value>(config)
        .map_err(|error| format!("connector config is not valid JSON: {error}"))?;

    encrypt_json_secrets(&mut value)?;

    serde_json::to_string(&value)
        .map_err(|error| format!("connector config could not be encoded: {error}"))
}

pub fn decrypt_connector_config(config: &str) -> Result<String, String> {
    let mut value = serde_json::from_str::<Value>(config)
        .map_err(|error| format!("connector config is not valid JSON: {error}"))?;

    decrypt_json_secrets(&mut value)?;

    serde_json::to_string(&value)
        .map_err(|error| format!("connector config could not be encoded: {error}"))
}

pub fn redact_connector_config(config: &str) -> String {
    let Ok(mut value) = serde_json::from_str::<Value>(config) else {
        return config.to_owned();
    };

    redact_json_secrets(&mut value);

    serde_json::to_string(&value).unwrap_or_else(|_| config.to_owned())
}

pub fn sanitized_json_snapshot<T: Serialize>(value: &T, max_chars: usize) -> Option<String> {
    let mut value = serde_json::to_value(value).ok()?;

    redact_json_secrets(&mut value);
    let snapshot = serde_json::to_string(&value).ok()?;

    if snapshot.chars().count() <= max_chars {
        return Some(snapshot);
    }

    let mut preview_len = max_chars.saturating_sub(128).max(1);

    loop {
        let preview = snapshot.chars().take(preview_len).collect::<String>();
        let wrapped = serde_json::to_string(&serde_json::json!({
            "_truncated": true,
            "_max_chars": max_chars,
            "_preview": preview,
        }))
        .ok()?;
        let wrapped_len = wrapped.chars().count();

        if wrapped_len <= max_chars || preview_len == 1 {
            return Some(wrapped);
        }

        preview_len = preview_len.saturating_sub(wrapped_len - max_chars).max(1);
    }
}

pub fn preserve_redacted_connector_config(
    config: &str,
    existing_config: Option<&str>,
) -> Result<String, String> {
    let mut value = serde_json::from_str::<Value>(config)
        .map_err(|error| format!("connector config is not valid JSON: {error}"))?;
    let existing_value = existing_config
        .map(|config| {
            serde_json::from_str::<Value>(config)
                .map_err(|error| format!("existing connector config is not valid JSON: {error}"))
        })
        .transpose()?;

    preserve_redacted_json_secrets(&mut value, existing_value.as_ref())?;

    serde_json::to_string(&value)
        .map_err(|error| format!("connector config could not be encoded: {error}"))
}

fn preserve_redacted_json_secrets(
    value: &mut Value,
    existing_value: Option<&Value>,
) -> Result<(), String> {
    match value {
        Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                let existing_child = existing_value.and_then(|existing| existing.get(key));

                if is_secret_key(key) {
                    if value.as_str() == Some(REDACTED_SECRET) {
                        let Some(existing_child) = existing_child else {
                            return Err(format!(
                                "{key} is redacted but no existing secret is stored"
                            ));
                        };

                        *value = existing_child.clone();
                    }
                } else {
                    preserve_redacted_json_secrets(value, existing_child)?;
                }
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter_mut().enumerate() {
                let existing_child = existing_value
                    .and_then(Value::as_array)
                    .and_then(|items| items.get(index));
                preserve_redacted_json_secrets(item, existing_child)?;
            }
        }
        _ => {}
    }

    Ok(())
}

fn encrypt_json_secrets(value: &mut Value) -> Result<(), String> {
    match value {
        Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if is_secret_key(key) {
                    if let Value::String(secret) = value {
                        if !secret.starts_with(ENCRYPTED_PREFIX) && !secret.is_empty() {
                            *secret = encrypt_secret(secret)?;
                        }
                    }
                } else {
                    encrypt_json_secrets(value)?;
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                encrypt_json_secrets(item)?;
            }
        }
        _ => {}
    }

    Ok(())
}

fn decrypt_json_secrets(value: &mut Value) -> Result<(), String> {
    match value {
        Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if is_secret_key(key) {
                    if let Value::String(secret) = value {
                        if secret.starts_with(ENCRYPTED_PREFIX) {
                            *secret = decrypt_secret(secret)?;
                        }
                    }
                } else {
                    decrypt_json_secrets(value)?;
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                decrypt_json_secrets(item)?;
            }
        }
        _ => {}
    }

    Ok(())
}

fn redact_json_secrets(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if is_secret_key(key) {
                    *value = Value::String(REDACTED_SECRET.to_owned());
                } else {
                    redact_json_secrets(value);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_json_secrets(item);
            }
        }
        _ => {}
    }
}

fn encrypt_secret(secret: &str) -> Result<String, String> {
    let cipher = cipher()?;
    let nonce_bytes = uuid::Uuid::new_v4();
    let nonce = &nonce_bytes.as_bytes()[..12];
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(nonce), secret.as_bytes())
        .map_err(|_| "connector secret could not be encrypted".to_owned())?;

    Ok(format!(
        "{ENCRYPTED_PREFIX}{}:{}",
        STANDARD_NO_PAD.encode(nonce),
        STANDARD_NO_PAD.encode(ciphertext)
    ))
}

fn decrypt_secret(secret: &str) -> Result<String, String> {
    let encrypted = secret
        .strip_prefix(ENCRYPTED_PREFIX)
        .ok_or_else(|| "connector secret is not encrypted".to_owned())?;
    let (nonce, ciphertext) = encrypted
        .split_once(':')
        .ok_or_else(|| "connector secret has an invalid encrypted format".to_owned())?;
    let nonce = STANDARD_NO_PAD
        .decode(nonce)
        .map_err(|_| "connector secret nonce is invalid".to_owned())?;
    let ciphertext = STANDARD_NO_PAD
        .decode(ciphertext)
        .map_err(|_| "connector secret ciphertext is invalid".to_owned())?;

    if nonce.len() != 12 {
        return Err("connector secret nonce has an invalid length".to_owned());
    }

    let plaintext = cipher()?
        .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
        .map_err(|_| "connector secret could not be decrypted".to_owned())?;

    String::from_utf8(plaintext).map_err(|_| "connector secret is not valid UTF-8".to_owned())
}

fn cipher() -> Result<Aes256Gcm, String> {
    let key_material = connector_secret_key()?;
    let digest = Sha256::digest(key_material.as_bytes());

    Aes256Gcm::new_from_slice(&digest)
        .map_err(|_| "connector secret key could not initialize AES-256-GCM".to_owned())
}

fn connector_secret_key() -> Result<String, String> {
    if let Ok(secret_key) = std::env::var("CONNECTOR_SECRET_KEY") {
        if !secret_key.trim().is_empty() {
            return Ok(secret_key);
        }
    }

    if std::env::var("APP_ENV").as_deref() == Ok("production") {
        return Err("CONNECTOR_SECRET_KEY must be set in production".to_owned());
    }

    Ok(DEV_SECRET_KEY.to_owned())
}

fn is_secret_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "personal_access_token"
            | "pat"
            | "token"
            | "password"
            | "secret"
            | "client_secret"
            | "api_key"
            | "x-api-key"
            | "bearer_token"
            | "access_token"
            | "refresh_token"
            | "authorization"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connector_config_secrets_are_encrypted_decrypted_and_redacted() {
        let config = r#"{"adapter":"azure_devops","personal_access_token":"test-pat","nested":{"client_secret":"client-value"}}"#;

        let encrypted = encrypt_connector_config(config).unwrap();
        assert!(!encrypted.contains("test-pat"));
        assert!(!encrypted.contains("client-value"));
        assert!(encrypted.contains(ENCRYPTED_PREFIX));

        let decrypted = decrypt_connector_config(&encrypted).unwrap();
        assert!(decrypted.contains("test-pat"));
        assert!(decrypted.contains("client-value"));

        let redacted = redact_connector_config(&encrypted);
        assert!(!redacted.contains(ENCRYPTED_PREFIX));
        assert!(!redacted.contains("test-pat"));
        assert!(redacted.contains(REDACTED_SECRET));
    }

    #[test]
    fn redacted_connector_config_preserves_existing_secrets() {
        let original = r#"{"adapter":"azure_devops","personal_access_token":"test-pat","nested":{"client_secret":"client-value"},"timeout_seconds":15}"#;
        let encrypted = encrypt_connector_config(original).unwrap();
        let incoming = r#"{"adapter":"azure_devops","personal_access_token":"***redacted***","nested":{"client_secret":"***redacted***"},"timeout_seconds":30}"#;

        let preserved = preserve_redacted_connector_config(incoming, Some(&encrypted)).unwrap();
        let encrypted_again = encrypt_connector_config(&preserved).unwrap();
        let decrypted = decrypt_connector_config(&encrypted_again).unwrap();
        let decrypted: Value = serde_json::from_str(&decrypted).unwrap();

        assert_eq!(decrypted["personal_access_token"], "test-pat");
        assert_eq!(decrypted["nested"]["client_secret"], "client-value");
        assert_eq!(decrypted["timeout_seconds"], 30);
    }

    #[test]
    fn redacted_connector_config_without_existing_secret_is_rejected() {
        let incoming = r#"{"personal_access_token":"***redacted***"}"#;

        let error = preserve_redacted_connector_config(incoming, None).unwrap_err();

        assert!(error.contains("personal_access_token is redacted"));
    }

    #[test]
    fn sanitized_json_snapshot_redacts_and_caps_payloads() {
        let payload = serde_json::json!({
            "token": "secret-token",
            "zz_body": "x".repeat(1000),
        });

        let snapshot = sanitized_json_snapshot(&payload, 256).unwrap();

        assert!(!snapshot.contains("secret-token"));
        assert!(snapshot.contains(REDACTED_SECRET));
        assert!(snapshot.contains("_truncated"));
    }
}

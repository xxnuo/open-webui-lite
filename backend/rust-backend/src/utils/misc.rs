use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use uuid::Uuid;

/// Deep update a JSON value with another (merges nested objects)
#[allow(dead_code)]
pub fn deep_update(target: &mut Value, source: &Value) {
    if let (Some(target_obj), Some(source_obj)) = (target.as_object_mut(), source.as_object()) {
        for (key, value) in source_obj {
            if let Some(target_value) = target_obj.get_mut(key) {
                if target_value.is_object() && value.is_object() {
                    deep_update(target_value, value);
                } else {
                    *target_value = value.clone();
                }
            } else {
                target_obj.insert(key.clone(), value.clone());
            }
        }
    }
}

/// Get message list from messages map by following parent IDs
#[allow(dead_code)]
pub fn get_message_list(messages_map: &HashMap<String, Value>, message_id: &str) -> Vec<Value> {
    let mut messages = Vec::new();
    let mut current_id = Some(message_id.to_string());

    while let Some(id) = current_id {
        if let Some(message) = messages_map.get(&id) {
            messages.push(message.clone());
            current_id = message
                .get("parentId")
                .and_then(|v| v.as_str())
                .map(String::from);
        } else {
            break;
        }
    }

    messages.reverse();
    messages
}

/// Generate a SHA256 hash of a string
#[allow(dead_code)]
pub fn sha256_hash(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Generate a random UUID v4
#[allow(dead_code)]
pub fn generate_uuid() -> String {
    Uuid::new_v4().to_string()
}

/// Calculate MD5 hash (for legacy compatibility)
#[allow(dead_code)]
pub fn md5_hash(input: &str) -> String {
    format!("{:x}", md5::compute(input.as_bytes()))
}

/// Parse duration string (e.g., "30m", "1h", "2d") to seconds
#[allow(dead_code)]
pub fn parse_duration_to_seconds(duration: &str) -> Option<i64> {
    let duration = duration.trim();
    if duration.is_empty() {
        return None;
    }

    let (num_str, unit) = duration.split_at(duration.len() - 1);
    let num: i64 = num_str.parse().ok()?;

    match unit {
        "s" => Some(num),
        "m" => Some(num * 60),
        "h" => Some(num * 3600),
        "d" => Some(num * 86400),
        "w" => Some(num * 604800),
        _ => None,
    }
}

/// Format seconds to duration string
#[allow(dead_code)]
pub fn format_seconds_to_duration(seconds: i64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86400 {
        format!("{}h", seconds / 3600)
    } else {
        format!("{}d", seconds / 86400)
    }
}

/// Extract file extension from filename
#[allow(dead_code)]
pub fn get_file_extension(filename: &str) -> Option<String> {
    std::path::Path::new(filename)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase())
}

/// Check if file extension is allowed
#[allow(dead_code)]
pub fn is_file_extension_allowed(filename: &str, allowed_extensions: &[String]) -> bool {
    if allowed_extensions.is_empty() {
        return true;
    }

    if let Some(ext) = get_file_extension(filename) {
        allowed_extensions
            .iter()
            .any(|allowed| allowed.trim_start_matches('.').eq_ignore_ascii_case(&ext))
    } else {
        false
    }
}

/// Sanitize filename to prevent path traversal
#[allow(dead_code)]
pub fn sanitize_filename(filename: &str) -> String {
    filename
        .replace("..", "_") // Replace .. first before replacing /
        .replace(['/', '\\'], "_")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '_' || *c == '-')
        .collect()
}

/// Extract base64 data and mime type from data URL
#[allow(dead_code)]
pub fn parse_data_url(data_url: &str) -> Option<(String, Vec<u8>)> {
    if !data_url.starts_with("data:") {
        return None;
    }

    let parts: Vec<&str> = data_url.splitn(2, ',').collect();
    if parts.len() != 2 {
        return None;
    }

    let header = parts[0];
    let data = parts[1];

    // Extract mime type
    let mime_type = header.strip_prefix("data:")?.split(';').next()?.to_string();

    // Decode base64
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let decoded = STANDARD.decode(data).ok()?;

    Some((mime_type, decoded))
}

/// Truncate string to max length with ellipsis
#[allow(dead_code)]
pub fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Check if user has a specific permission based on config
pub fn has_permission(
    user_id: &str,
    permission: &str,
    user_permissions: &serde_json::Value,
) -> bool {
    if let Some(perms) = user_permissions.as_object() {
        if let Some(user_perms) = perms.get(user_id) {
            if let Some(user_perms_obj) = user_perms.as_object() {
                if let Some(perm_value) = user_perms_obj.get(permission) {
                    return perm_value.as_bool().unwrap_or(false);
                }
            }
        }
    }
    false
}

/// Check if user has access based on access control
pub fn has_access(
    user_id: &str,
    access_type: &str,
    access_control: &Option<serde_json::Value>,
    user_group_ids: &std::collections::HashSet<String>,
) -> bool {
    // If access_control is None, it's public
    let access_control = match access_control {
        Some(ac) => ac,
        None => return true, // Public access
    };

    // If access_control is empty object {}, it's private (only owner)
    if access_control.is_object() && access_control.as_object().unwrap().is_empty() {
        return false;
    }

    // Check for specific access type (read/write)
    if let Some(access_obj) = access_control.as_object() {
        if let Some(type_access) = access_obj.get(access_type) {
            if let Some(type_access_obj) = type_access.as_object() {
                // Check group_ids
                if let Some(group_ids) = type_access_obj.get("group_ids") {
                    if let Some(group_ids_arr) = group_ids.as_array() {
                        for group_id in group_ids_arr {
                            if let Some(gid) = group_id.as_str() {
                                if user_group_ids.contains(gid) {
                                    return true;
                                }
                            }
                        }
                    }
                }

                // Check user_ids
                if let Some(user_ids) = type_access_obj.get("user_ids") {
                    if let Some(user_ids_arr) = user_ids.as_array() {
                        for uid in user_ids_arr {
                            if let Some(uid_str) = uid.as_str() {
                                if uid_str == user_id {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_deep_update() {
        let mut target = json!({
            "a": 1,
            "b": {
                "c": 2,
                "d": 3
            }
        });

        let source = json!({
            "b": {
                "c": 5,
                "e": 6
            },
            "f": 7
        });

        deep_update(&mut target, &source);

        assert_eq!(target["a"], 1);
        assert_eq!(target["b"]["c"], 5);
        assert_eq!(target["b"]["d"], 3);
        assert_eq!(target["b"]["e"], 6);
        assert_eq!(target["f"], 7);
    }

    #[test]
    fn test_sha256_hash() {
        let hash = sha256_hash("hello");
        assert_eq!(
            hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration_to_seconds("30s"), Some(30));
        assert_eq!(parse_duration_to_seconds("5m"), Some(300));
        assert_eq!(parse_duration_to_seconds("2h"), Some(7200));
        assert_eq!(parse_duration_to_seconds("1d"), Some(86400));
        assert_eq!(parse_duration_to_seconds("invalid"), None);
    }

    #[test]
    fn test_get_file_extension() {
        assert_eq!(get_file_extension("file.txt"), Some("txt".to_string()));
        assert_eq!(get_file_extension("file.TAR.GZ"), Some("gz".to_string()));
        assert_eq!(get_file_extension("noext"), None);
    }

    #[test]
    fn test_sanitize_filename() {
        // "../../../etc/passwd" -> "_/_/_/etc/passwd" (after ..) -> "______etc_passwd" (after /)
        assert_eq!(sanitize_filename("../../../etc/passwd"), "______etc_passwd");
        assert_eq!(
            sanitize_filename("valid-file_name.txt"),
            "valid-file_name.txt"
        );
        assert_eq!(
            sanitize_filename("file with spaces.txt"),
            "filewithspaces.txt"
        );
    }

    #[test]
    fn test_truncate_string() {
        assert_eq!(truncate_string("short", 10), "short");
        assert_eq!(
            truncate_string("this is a very long string", 10),
            "this is..."
        );
    }
}

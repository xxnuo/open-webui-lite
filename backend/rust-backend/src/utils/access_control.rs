use serde_json::Value as JsonValue;
use std::collections::HashSet;

use crate::db::Database;
use crate::error::AppResult;
use crate::models::user::User;
use crate::services::group::GroupService;
use crate::services::user::UserService;

/// Check if a user has access to a resource based on access control settings
pub async fn has_access(
    db: &Database,
    user_id: &str,
    access_type: &str, // "read" or "write"
    access_control: Option<&JsonValue>,
    strict: bool,
) -> AppResult<bool> {
    tracing::debug!(
        "has_access: user_id={}, access_type={}, strict={}",
        user_id,
        access_type,
        strict
    );
    tracing::debug!("  access_control: {:?}", access_control);

    // If no access control is set
    if access_control.is_none() {
        if strict {
            // In strict mode, only allow read access
            let result = access_type == "read";
            tracing::debug!("  No access_control, strict=true -> returning {}", result);
            return Ok(result);
        } else {
            // In non-strict mode, allow all access
            tracing::debug!("  No access_control, strict=false -> returning true");
            return Ok(true);
        }
    }

    let access_control = access_control.unwrap();

    // Get user's group IDs
    let group_service = GroupService::new(db);
    let user_groups = group_service.get_groups_by_member_id(user_id).await?;
    tracing::debug!("  User groups: {} groups found", user_groups.len());
    for group in &user_groups {
        tracing::debug!(
            "    - Group '{}' (id={}), user_ids={:?}",
            group.name,
            group.id,
            group.user_ids
        );
    }
    let user_group_ids: HashSet<String> = user_groups.iter().map(|g| g.id.clone()).collect();
    tracing::debug!("  User group IDs: {:?}", user_group_ids);

    // Get permission access for the specified type
    // If the access_type key doesn't exist, treat it as an empty permission object
    // This matches Python backend behavior: access_control.get(type, {})
    let empty_permissions = serde_json::json!({});
    let permission_access = access_control
        .get(access_type)
        .unwrap_or(&empty_permissions);
    tracing::debug!(
        "  Permission access for '{}': {:?}",
        access_type,
        permission_access
    );

    // Get permitted group IDs and user IDs
    let permitted_group_ids: Vec<String> = permission_access
        .get("group_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let permitted_user_ids: Vec<String> = permission_access
        .get("user_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    tracing::debug!("  Permitted group IDs: {:?}", permitted_group_ids);
    tracing::debug!("  Permitted user IDs: {:?}", permitted_user_ids);

    // Check if user ID is in permitted user IDs
    if permitted_user_ids.contains(&user_id.to_string()) {
        tracing::info!("  ✓ User ID found in permitted_user_ids -> granting access");
        return Ok(true);
    }

    // Check if any of user's group IDs are in permitted group IDs
    for group_id in permitted_group_ids {
        if user_group_ids.contains(&group_id) {
            tracing::info!(
                "  ✓ User's group '{}' found in permitted_group_ids -> granting access",
                group_id
            );
            return Ok(true);
        }
    }

    tracing::warn!("  ✗ No matching groups or user IDs found -> denying access");
    Ok(false)
}

/// Check if user has permission based on hierarchical permission key
pub async fn has_permission(
    db: &Database,
    user_id: &str,
    permission_key: &str,
    default_permissions: &JsonValue,
) -> AppResult<bool> {
    let permission_hierarchy: Vec<&str> = permission_key.split('.').collect();

    // Get user's groups
    let group_service = GroupService::new(db);
    let user_groups = group_service.get_groups_by_member_id(user_id).await?;

    // Check group permissions
    for group in user_groups {
        if let Some(ref permissions) = group.permissions {
            if get_permission_value(permissions, &permission_hierarchy) {
                return Ok(true);
            }
        }
    }

    // Fall back to default permissions
    Ok(get_permission_value(
        default_permissions,
        &permission_hierarchy,
    ))
}

/// Traverse permissions object using hierarchical keys
fn get_permission_value(permissions: &JsonValue, keys: &[&str]) -> bool {
    let mut current = permissions;

    for key in keys {
        match current.get(key) {
            Some(value) => current = value,
            None => return false,
        }
    }

    current.as_bool().unwrap_or(false)
}

/// Get all users with access to a resource
pub async fn get_users_with_access(
    db: &Database,
    access_type: &str, // "read" or "write"
    access_control: Option<&JsonValue>,
) -> AppResult<Vec<User>> {
    let user_service = UserService::new(db);

    // If no access control, return empty list for now
    // TODO: Implement get_all_users if needed
    if access_control.is_none() {
        return Ok(Vec::new());
    }

    let access_control = access_control.unwrap();
    let permission_access = access_control.get(access_type);

    if permission_access.is_none() {
        return Ok(Vec::new());
    }

    let permission_access = permission_access.unwrap();

    // Get permitted group IDs and user IDs
    let permitted_group_ids: Vec<String> = permission_access
        .get("group_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let permitted_user_ids: Vec<String> = permission_access
        .get("user_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let mut user_ids_with_access: HashSet<String> = permitted_user_ids.into_iter().collect();

    // Add users from permitted groups
    let group_service = GroupService::new(db);
    for group_id in permitted_group_ids {
        if let Ok(Some(group)) = group_service.get_group_by_id(&group_id).await {
            // Groups have user_ids as Vec<String>
            user_ids_with_access.extend(group.user_ids);
        }
    }

    // Get users by IDs - for now just return matching user IDs as Users
    // TODO: Implement batch user retrieval if needed
    let mut users = Vec::new();
    for user_id in user_ids_with_access {
        if let Ok(Some(user)) = user_service.get_user_by_id(&user_id).await {
            users.push(user);
        }
    }

    Ok(users)
}

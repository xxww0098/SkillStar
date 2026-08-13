//! Tolerant readers for registry JSON.
//!
//! The same logical field arrives under two spellings depending on where the
//! payload came from: the `2025-12-11` `server.json` schema is camelCase
//! (`registryType`, `environmentVariables`, `isRequired`), while the legacy
//! `/v0` GitHub responses and our hand-written curated seeds use snake_case
//! (`registry_type`, `environment_variables`, `is_required`). Every reader here
//! takes a key list and returns the first spelling that is present, so one
//! parser serves both eras without duplicating the field table.

use serde_json::Value;

/// First non-empty string among `keys` (trimmed).
pub(crate) fn str_field(obj: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = obj
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Some(value.to_string());
        }
    }
    None
}

/// First boolean among `keys`. Accepts the string forms `"true"` / `"false"`
/// because some publishers hand-write them that way.
pub(crate) fn bool_field(obj: &Value, keys: &[&str]) -> Option<bool> {
    for key in keys {
        match obj.get(key) {
            Some(Value::Bool(b)) => return Some(*b),
            Some(Value::String(s)) => match s.trim().to_ascii_lowercase().as_str() {
                "true" => return Some(true),
                "false" => return Some(false),
                _ => continue,
            },
            _ => continue,
        }
    }
    None
}

/// First array among `keys`.
pub(crate) fn arr_field<'a>(obj: &'a Value, keys: &[&str]) -> Option<&'a Vec<Value>> {
    for key in keys {
        if let Some(arr) = obj.get(key).and_then(Value::as_array) {
            return Some(arr);
        }
    }
    None
}

/// First object among `keys`.
pub(crate) fn obj_field<'a>(obj: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    for key in keys {
        if let Some(inner) = obj.get(key).filter(|v| v.is_object()) {
            return Some(inner);
        }
    }
    None
}

/// Scalar → display string. `default` / `choices` / `value` are typed `string`
/// in the schema but publishers do ship raw booleans and numbers, and dropping
/// those would silently blank out a form field's default.
pub(crate) fn scalar_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => {
            let trimmed = s.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// First scalar-as-string among `keys`.
pub(crate) fn scalar_field(obj: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(found) = obj.get(key).and_then(scalar_to_string) {
            return Some(found);
        }
    }
    None
}

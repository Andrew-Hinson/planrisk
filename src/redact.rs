//! Hiding sensitive values before anything is printed.
//!
//! `terraform show -json` writes sensitive values in plain text. The
//! [`terraform show` docs](https://developer.hashicorp.com/terraform/cli/commands/show)
//! say: "When using the `-json` command-line flag, any sensitive values in
//! Terraform state will be displayed in plain text." Alongside each value,
//! the plan has a *sensitivity mask* (`before_sensitive`, `after_sensitive`)
//! of the same shape, with `true` wherever a value is sensitive. The JSON
//! format docs say the two "should be combined ... to prevent accidental
//! display of sensitive values in user interfaces". This module does that.

use serde_json::{Map, Value};

/// What replaces a sensitive value. The same text `terraform plan` prints.
pub const PLACEHOLDER: &str = "(sensitive value)";

/// Returns a copy of `value` with every sensitive part replaced by
/// [`PLACEHOLDER`], according to `mask`.
///
/// - A mask of `true` hides the value and everything inside it.
/// - A mask object or array is walked alongside the value; parts the mask
///   doesn't mention are kept.
/// - A missing, `null` or `false` mask means nothing is sensitive.
/// - If the mask and value have different shapes (e.g. the mask is an object
///   but the value is a string), the value is hidden: when unsure, don't print.
pub fn redact(value: &Value, mask: &Value) -> Value {
    match (mask, value) {
        // Nothing marked sensitive here.
        (Value::Null | Value::Bool(false), _) => value.clone(),
        // Everything here is sensitive.
        (Value::Bool(true), _) => Value::String(PLACEHOLDER.to_string()),

        (Value::Object(mask_fields), Value::Object(fields)) => {
            let redacted: Map<String, Value> = fields
                .iter()
                .map(|(key, field)| {
                    let field_mask = mask_fields.get(key).unwrap_or(&Value::Null);
                    (key.clone(), redact(field, field_mask))
                })
                .collect();
            Value::Object(redacted)
        }

        (Value::Array(mask_items), Value::Array(items)) if mask_items.len() <= items.len() => {
            let redacted = items
                .iter()
                .enumerate()
                .map(|(i, item)| redact(item, mask_items.get(i).unwrap_or(&Value::Null)))
                .collect();
            Value::Array(redacted)
        }

        // A null value has nothing to leak, whatever the mask says.
        (_, Value::Null) => Value::Null,

        // Shapes don't line up (or the mask has more items than the value):
        // hide the whole value rather than risk printing part of a secret.
        _ => Value::String(PLACEHOLDER.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn true_mask_hides_the_value() {
        assert_eq!(redact(&json!("hunter2"), &json!(true)), json!(PLACEHOLDER));
        assert_eq!(redact(&json!({"a": 1}), &json!(true)), json!(PLACEHOLDER));
    }

    #[test]
    fn missing_or_false_mask_keeps_the_value() {
        let value = json!({"name": "db", "port": 5432});
        assert_eq!(redact(&value, &json!(null)), value);
        assert_eq!(redact(&value, &json!(false)), value);
        assert_eq!(redact(&value, &json!({})), value);
    }

    #[test]
    fn only_marked_fields_are_hidden() {
        let value = json!({"username": "admin", "password": "hunter2", "port": 5432});
        let mask = json!({"password": true});
        assert_eq!(
            redact(&value, &mask),
            json!({"username": "admin", "password": PLACEHOLDER, "port": 5432})
        );
    }

    #[test]
    fn nested_objects_and_lists_are_walked() {
        let value = json!({
            "tags": {"team": "data", "token": "abc"},
            "rules": [{"cidr": "10.0.0.0/8"}, {"cidr": "secret-cidr"}]
        });
        let mask = json!({
            "tags": {"token": true},
            "rules": [{}, {"cidr": true}]
        });
        assert_eq!(
            redact(&value, &mask),
            json!({
                "tags": {"team": "data", "token": PLACEHOLDER},
                "rules": [{"cidr": "10.0.0.0/8"}, {"cidr": PLACEHOLDER}]
            })
        );
    }

    #[test]
    fn true_hides_everything_below_it() {
        let value = json!({"block": {"a": "x", "nested": {"b": "y"}}});
        assert_eq!(
            redact(&value, &json!({"block": true})),
            json!({"block": PLACEHOLDER})
        );
    }

    #[test]
    fn mismatched_shapes_are_hidden() {
        // Mask expects an object, but the value is a string.
        assert_eq!(
            redact(&json!("hunter2"), &json!({"a": true})),
            json!(PLACEHOLDER)
        );
        // Mask expects a list, but the value is an object.
        assert_eq!(
            redact(&json!({"a": "x"}), &json!([true])),
            json!(PLACEHOLDER)
        );
        // Mask has more items than the value.
        assert_eq!(
            redact(&json!(["a"]), &json!([false, true])),
            json!(PLACEHOLDER)
        );
    }

    #[test]
    fn nulls_stay_null() {
        assert_eq!(redact(&json!(null), &json!({"a": true})), json!(null));
        assert_eq!(redact(&json!(null), &json!(true)), json!(PLACEHOLDER));
    }
}

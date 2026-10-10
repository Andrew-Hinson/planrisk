//! What Terraform will do to a resource, from the `actions` list in plan JSON.
//!
//! Terraform's source lists the valid values (see PLAN.md §6):
//!
//! | `actions`                                   | `Action`                                   |
//! |---------------------------------------------|--------------------------------------------|
//! | `["no-op"]`                                 | `NoOp`                                     |
//! | `["create"]`                                | `Create`                                   |
//! | `["read"]`                                  | `Read`                                     |
//! | `["update"]`                                | `Update`                                   |
//! | `["delete"]`                                | `Delete`                                   |
//! | `["delete", "create"]`                      | `Replace { create_first: false }`          |
//! | `["create", "delete"]`                      | `Replace { create_first: true }`           |
//! | `["forget"]`                                | `Forget`                                   |
//! | `["forget", "create"]`                      | `ForgetReplace { create_first: false }`    |
//! | `["create", "forget"]`                      | `ForgetReplace { create_first: true }`     |
//! | anything else                               | `Unknown(list)`                            |

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
// Deserialize the JSON list into a Vec<String> first, then convert it with
// the `From<Vec<String>>` impl below. Any list converts, so an action
// Terraform adds in the future can never make parsing fail.
#[serde(from = "Vec<String>")]
pub enum Action {
    /// Nothing changes.
    NoOp,
    Create,
    /// A data source is read during apply.
    Read,
    /// Changed in place.
    Update,
    Delete,
    /// The object is destroyed and a new one created. `create_first` is true
    /// for `create_before_destroy` resources, where the new object exists
    /// before the old one is destroyed.
    Replace {
        create_first: bool,
    },
    /// Removed from Terraform's state but not destroyed (a `removed` block
    /// with `destroy = false`).
    Forget,
    /// Replaced, but the old object is forgotten rather than destroyed.
    ForgetReplace {
        create_first: bool,
    },
    /// A list this version of planrisk doesn't recognise, kept as-is so it can
    /// be reported.
    Unknown(Vec<String>),
}

impl From<Vec<String>> for Action {
    fn from(actions: Vec<String>) -> Self {
        // Borrow each String as &str so the match can compare against literals.
        let as_strs: Vec<&str> = actions.iter().map(String::as_str).collect();
        match as_strs.as_slice() {
            ["no-op"] => Action::NoOp,
            ["create"] => Action::Create,
            ["read"] => Action::Read,
            ["update"] => Action::Update,
            ["delete"] => Action::Delete,
            ["delete", "create"] => Action::Replace {
                create_first: false,
            },
            ["create", "delete"] => Action::Replace { create_first: true },
            ["forget"] => Action::Forget,
            ["forget", "create"] => Action::ForgetReplace {
                create_first: false,
            },
            ["create", "forget"] => Action::ForgetReplace { create_first: true },
            _ => Action::Unknown(actions),
        }
    }
}

impl Action {
    /// True when the existing object will be destroyed.
    ///
    /// Terraform's source says replaces are lists so callers can "just scan
    /// the list for 'delete' to recognize all … situations where the object
    /// will be deleted, allowing for any new deletion combinations that might
    /// be added in future". So an unknown list counts if it contains "delete".
    pub fn deletes_object(&self) -> bool {
        match self {
            Action::Delete | Action::Replace { .. } => true,
            Action::Unknown(actions) => actions.iter().any(|a| a == "delete"),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an Action the same way deserialization does.
    fn action(list: &[&str]) -> Action {
        Action::from(list.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn single_actions() {
        assert_eq!(action(&["no-op"]), Action::NoOp);
        assert_eq!(action(&["create"]), Action::Create);
        assert_eq!(action(&["read"]), Action::Read);
        assert_eq!(action(&["update"]), Action::Update);
        assert_eq!(action(&["delete"]), Action::Delete);
        assert_eq!(action(&["forget"]), Action::Forget);
    }

    #[test]
    fn replaces_keep_their_order() {
        assert_eq!(
            action(&["delete", "create"]),
            Action::Replace {
                create_first: false
            }
        );
        assert_eq!(
            action(&["create", "delete"]),
            Action::Replace { create_first: true }
        );
        assert_eq!(
            action(&["forget", "create"]),
            Action::ForgetReplace {
                create_first: false
            }
        );
        assert_eq!(
            action(&["create", "forget"]),
            Action::ForgetReplace { create_first: true }
        );
    }

    #[test]
    fn unrecognised_lists_are_kept() {
        assert_eq!(
            action(&["teleport"]),
            Action::Unknown(vec!["teleport".to_string()])
        );
        assert_eq!(action(&[]), Action::Unknown(vec![]));
        // Known words in an unknown combination are still unknown.
        assert_eq!(
            action(&["update", "delete"]),
            Action::Unknown(vec!["update".to_string(), "delete".to_string()])
        );
    }

    #[test]
    fn deletes_object_for_deletes_and_replaces_only() {
        assert!(action(&["delete"]).deletes_object());
        assert!(action(&["delete", "create"]).deletes_object());
        assert!(action(&["create", "delete"]).deletes_object());

        for kept in [
            &["no-op"][..],
            &["create"],
            &["read"],
            &["update"],
            &["forget"],
            &["forget", "create"],
            &["create", "forget"],
        ] {
            assert!(!action(kept).deletes_object(), "{kept:?}");
        }
    }

    #[test]
    fn unknown_list_containing_delete_deletes_object() {
        assert!(action(&["update", "delete"]).deletes_object());
        assert!(!action(&["teleport"]).deletes_object());
    }

    #[test]
    fn deserializes_from_json_list() {
        let parsed: Action = serde_json::from_str(r#"["create", "delete"]"#).unwrap();
        assert_eq!(parsed, Action::Replace { create_first: true });
    }
}

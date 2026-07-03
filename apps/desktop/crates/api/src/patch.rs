//! Tri-state mutation field: absent vs `null` vs value (masterplan-v3 §4.1).
//!
//! tRPC update inputs use zod `.nullable().optional()` fields where the three
//! states mean different things: **omitted** = leave unchanged, **null** =
//! clear, **value** = set. A plain `Option<T>` cannot express all three, so
//! typed inputs use [`Patch<T>`] with
//! `#[serde(skip_serializing_if = "Patch::is_omit")]`.

use serde::{Serialize, Serializer};

/// A field of a tRPC update input: leave unchanged / clear to null / set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Patch<T> {
    /// Field is not sent — the server leaves the column unchanged.
    #[default]
    Omit,
    /// Field is sent as JSON `null` — the server clears the column.
    Null,
    /// Field is sent with a value.
    Set(T),
}

impl<T> Patch<T> {
    /// For `#[serde(skip_serializing_if = "Patch::is_omit")]`.
    pub fn is_omit(&self) -> bool {
        matches!(self, Patch::Omit)
    }

    /// `Some(v) → Set(v)`, `None → Null` — for callers holding an
    /// `Option` that means "set or clear" (never "leave unchanged").
    pub fn set_or_null(value: Option<T>) -> Self {
        match value {
            Some(value) => Patch::Set(value),
            None => Patch::Null,
        }
    }
}

impl<T: Serialize> Serialize for Patch<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            // Omit only reaches here if a caller forgot skip_serializing_if;
            // serializing null is the least-wrong fallback (it degrades to an
            // explicit clear rather than corrupt JSON).
            Patch::Omit | Patch::Null => serializer.serialize_none(),
            Patch::Set(value) => value.serialize(serializer),
        }
    }
}

impl<T> From<T> for Patch<T> {
    fn from(value: T) -> Self {
        Patch::Set(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input {
        id: String,
        #[serde(skip_serializing_if = "Patch::is_omit")]
        assignee_id: Patch<String>,
        #[serde(skip_serializing_if = "Patch::is_omit")]
        due_date: Patch<String>,
    }

    #[test]
    fn omit_null_and_set_serialize_distinctly() {
        let json = serde_json::to_string(&Input {
            id: "i-1".to_string(),
            assignee_id: Patch::Omit,
            due_date: Patch::Null,
        })
        .unwrap();
        assert_eq!(json, r#"{"id":"i-1","dueDate":null}"#);

        let json = serde_json::to_string(&Input {
            id: "i-1".to_string(),
            assignee_id: Patch::Set("u-1".to_string()),
            due_date: Patch::Omit,
        })
        .unwrap();
        assert_eq!(json, r#"{"id":"i-1","assigneeId":"u-1"}"#);
    }

    #[test]
    fn set_or_null_maps_option() {
        assert_eq!(Patch::set_or_null(Some(1)), Patch::Set(1));
        assert_eq!(Patch::<i32>::set_or_null(None), Patch::Null);
    }
}

//! Typed domain enums over the generated contract values (masterplan-v3 §5.5).
//!
//! The canonical string values + display orders live in
//! `contract.generated.rs` (emitted from `@exp/domain-contract` — never
//! hand-edit). These enums layer type safety and the tolerant-unknown rule on
//! top: an unknown wire value deserializes to `Unknown` rather than dropping
//! the row (forward-compat with a server that adds an enum value before the
//! desktop updates, §5.5). Tests below lock the variant lists to the generated
//! constants so a contract regen that adds a value fails loudly here.
//!
//! Icon/color option tables for the UI land with the Phase-3 screens (§4.7);
//! Phase 2 needs only value identity + display order + labels.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// `issue_status` — canonical values in `ISSUE_STATUS_VALUES`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IssueStatus {
    Backlog,
    Todo,
    InProgress,
    Done,
    Cancelled,
    Duplicate,
    /// Forward-compat fallback (§5.5): an unknown value must never drop the row.
    Unknown,
}

impl IssueStatus {
    /// Board display order — mirrors the generated
    /// `ISSUE_STATUS_DISPLAY_ORDER` (locked by test).
    pub const DISPLAY_ORDER: [IssueStatus; 6] = [
        IssueStatus::InProgress,
        IssueStatus::Todo,
        IssueStatus::Backlog,
        IssueStatus::Done,
        IssueStatus::Cancelled,
        IssueStatus::Duplicate,
    ];

    pub fn from_wire(value: &str) -> IssueStatus {
        match value {
            "backlog" => IssueStatus::Backlog,
            "todo" => IssueStatus::Todo,
            "in_progress" => IssueStatus::InProgress,
            "done" => IssueStatus::Done,
            "cancelled" => IssueStatus::Cancelled,
            "duplicate" => IssueStatus::Duplicate,
            _ => IssueStatus::Unknown,
        }
    }

    /// The canonical wire value (`Unknown` has none).
    pub fn as_wire(&self) -> Option<&'static str> {
        match self {
            IssueStatus::Backlog => Some("backlog"),
            IssueStatus::Todo => Some("todo"),
            IssueStatus::InProgress => Some("in_progress"),
            IssueStatus::Done => Some("done"),
            IssueStatus::Cancelled => Some("cancelled"),
            IssueStatus::Duplicate => Some("duplicate"),
            IssueStatus::Unknown => None,
        }
    }

    /// Human label (web parity).
    pub fn label(&self) -> &'static str {
        match self {
            IssueStatus::Backlog => "Backlog",
            IssueStatus::Todo => "Todo",
            IssueStatus::InProgress => "In Progress",
            IssueStatus::Done => "Done",
            IssueStatus::Cancelled => "Cancelled",
            IssueStatus::Duplicate => "Duplicate",
            IssueStatus::Unknown => "Unknown",
        }
    }
}

/// `issue_priority` — canonical values in `ISSUE_PRIORITY_VALUES`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IssuePriority {
    None,
    Urgent,
    High,
    Medium,
    Low,
    /// Forward-compat fallback (§5.5).
    Unknown,
}

impl IssuePriority {
    /// Mirrors the generated `ISSUE_PRIORITY_DISPLAY_ORDER` (locked by test).
    pub const DISPLAY_ORDER: [IssuePriority; 5] = [
        IssuePriority::Urgent,
        IssuePriority::High,
        IssuePriority::Medium,
        IssuePriority::Low,
        IssuePriority::None,
    ];

    pub fn from_wire(value: &str) -> IssuePriority {
        match value {
            "none" => IssuePriority::None,
            "urgent" => IssuePriority::Urgent,
            "high" => IssuePriority::High,
            "medium" => IssuePriority::Medium,
            "low" => IssuePriority::Low,
            _ => IssuePriority::Unknown,
        }
    }

    pub fn as_wire(&self) -> Option<&'static str> {
        match self {
            IssuePriority::None => Some("none"),
            IssuePriority::Urgent => Some("urgent"),
            IssuePriority::High => Some("high"),
            IssuePriority::Medium => Some("medium"),
            IssuePriority::Low => Some("low"),
            IssuePriority::Unknown => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            IssuePriority::None => "No priority",
            IssuePriority::Urgent => "Urgent",
            IssuePriority::High => "High",
            IssuePriority::Medium => "Medium",
            IssuePriority::Low => "Low",
            IssuePriority::Unknown => "Unknown",
        }
    }
}

// Explicit serde impls (not derive + #[serde(other)]) so the tolerant-unknown
// rule is unambiguous: deserialize from the canonical string, anything
// unrecognized → Unknown; serialize back to the canonical string ("unknown"
// for the fallback — it never travels to the server, mutations are typed).
macro_rules! wire_enum_serde {
    ($ty:ty) => {
        impl<'de> Deserialize<'de> for $ty {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let raw = String::deserialize(deserializer)?;
                Ok(<$ty>::from_wire(&raw))
            }
        }

        impl Serialize for $ty {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(self.as_wire().unwrap_or("unknown"))
            }
        }
    };
}

wire_enum_serde!(IssueStatus);
wire_enum_serde!(IssuePriority);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract;

    #[test]
    fn issue_status_matches_generated_contract() {
        // Every canonical value round-trips; the lists stay in lockstep with
        // the generated contract (a regen that adds a value fails here).
        for value in contract::ISSUE_STATUS_VALUES {
            let status = IssueStatus::from_wire(value);
            assert_ne!(status, IssueStatus::Unknown, "unmapped status {value}");
            assert_eq!(status.as_wire(), Some(*value));
        }
        let order: Vec<&str> = IssueStatus::DISPLAY_ORDER
            .iter()
            .map(|s| s.as_wire().unwrap())
            .collect();
        assert_eq!(order, contract::ISSUE_STATUS_DISPLAY_ORDER);
        assert_eq!(IssueStatus::from_wire("triaged"), IssueStatus::Unknown);
    }

    #[test]
    fn issue_priority_matches_generated_contract() {
        for value in contract::ISSUE_PRIORITY_VALUES {
            let priority = IssuePriority::from_wire(value);
            assert_ne!(priority, IssuePriority::Unknown, "unmapped priority {value}");
            assert_eq!(priority.as_wire(), Some(*value));
        }
        let order: Vec<&str> = IssuePriority::DISPLAY_ORDER
            .iter()
            .map(|p| p.as_wire().unwrap())
            .collect();
        assert_eq!(order, contract::ISSUE_PRIORITY_DISPLAY_ORDER);
        assert_eq!(IssuePriority::from_wire("blocker"), IssuePriority::Unknown);
    }

    #[test]
    fn enums_deserialize_tolerantly() {
        let status: IssueStatus = serde_json::from_str("\"in_progress\"").unwrap();
        assert_eq!(status, IssueStatus::InProgress);
        let status: IssueStatus = serde_json::from_str("\"brand_new_state\"").unwrap();
        assert_eq!(status, IssueStatus::Unknown);
        let priority: IssuePriority = serde_json::from_str("\"urgent\"").unwrap();
        assert_eq!(priority, IssuePriority::Urgent);
    }
}

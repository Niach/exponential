import ExpCore
import Foundation

// Rendering helpers for the issue activity timeline (status/assignee/label/PR
// events), shared by CommentThreadView. Relocated out of the deleted agent Plan
// Panel; SwiftUI-free so they can live next to the views that use them.

/// Human-readable verb for an issue event type (the generic fallback used when
/// an event has no payload to render richly).
func eventVerb(_ type: String) -> String {
    switch type {
    case "status_changed": return "changed the status"
    case "assignee_changed": return "changed the assignee"
    case "label_added": return "added a label"
    case "label_removed": return "removed a label"
    case "pr_opened": return "opened a pull request"
    case "pr_merged": return "merged the pull request"
    default: return type.replacingOccurrences(of: "_", with: " ")
    }
}

/// Human label for an issue_status enum value.
func statusLabel(_ s: String) -> String {
    switch s {
    case "backlog": return "Backlog"
    case "todo": return "Todo"
    case "in_progress": return "In Progress"
    case "done": return "Done"
    case "cancelled": return "Cancelled"
    case "duplicate": return "Duplicate"
    default: return s.replacingOccurrences(of: "_", with: " ").capitalized
    }
}

/// Pull a string or integer scalar out of an issue_event's JSON payload (stored
/// as stringified JSON). Returns nil for missing/null/empty values.
func eventField(_ payload: String?, _ key: String) -> String? {
    guard let payload, let data = payload.data(using: .utf8),
          let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
          let value = obj[key], !(value is NSNull) else { return nil }
    if let s = value as? String { return s.isEmpty ? nil : s }
    if let i = value as? Int { return String(i) }
    if let d = value as? Double { return String(Int(d)) }
    return nil
}

/// A rich activity phrase from the event type + payload (status from→to, PR #N,
/// assigned/unassigned, label name). Resolves user/label names when the maps are
/// supplied; falls back to the generic verb for events without a payload.
/// Mirrors the web activity timeline.
func eventPhrase(
    _ event: IssueEventEntity,
    users: [String: UserEntity],
    labels: [String: LabelEntity]?
) -> String {
    switch event.type {
    case "status_changed":
        guard let to = eventField(event.payload, "to") else { return "changed the status" }
        if let from = eventField(event.payload, "from") {
            return "changed status from \(statusLabel(from)) to \(statusLabel(to))"
        }
        return "changed status to \(statusLabel(to))"
    case "assignee_changed":
        guard let to = eventField(event.payload, "to") else { return "unassigned this issue" }
        if let name = users[to].map({ $0.name ?? $0.email }) {
            return "assigned \(name)"
        }
        return "assigned this issue"
    case "label_added":
        if let id = eventField(event.payload, "labelId"), let name = labels?[id]?.name {
            return "added label \(name)"
        }
        return "added a label"
    case "label_removed":
        if let id = eventField(event.payload, "labelId"), let name = labels?[id]?.name {
            return "removed label \(name)"
        }
        return "removed a label"
    case "pr_opened":
        if let n = eventField(event.payload, "prNumber") { return "opened PR #\(n)" }
        return "opened a pull request"
    case "pr_merged":
        if let n = eventField(event.payload, "prNumber") { return "merged PR #\(n)" }
        return "merged the pull request"
    default:
        return eventVerb(event.type)
    }
}

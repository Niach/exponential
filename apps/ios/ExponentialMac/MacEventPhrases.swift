import ExpCore
import Foundation

// Rendering helpers for the macOS issue activity timeline (status/assignee/
// label/PR events), used by MacIssueDetailView. Relocated out of the deleted
// macOS Plan Panel; no SwiftUI dependency.

/// Human-readable verb for an issue event type (the generic fallback used when
/// an event has no payload to render richly).
func macEventVerb(_ type: String) -> String {
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
func macStatusLabel(_ s: String) -> String {
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

/// Pull a string or integer scalar out of an issue_event's JSON payload.
func macEventField(_ payload: String?, _ key: String) -> String? {
    guard let payload, let data = payload.data(using: .utf8),
          let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
          let value = obj[key], !(value is NSNull) else { return nil }
    if let s = value as? String { return s.isEmpty ? nil : s }
    if let i = value as? Int { return String(i) }
    if let d = value as? Double { return String(Int(d)) }
    return nil
}

/// A rich activity phrase from the event type + payload (status from→to, PR #N,
/// assigned/unassigned, label name). Resolves names via the supplied closures;
/// falls back to the generic verb for events without a payload. Mirrors the web
/// activity timeline.
func macEventPhrase(
    _ event: IssueEventEntity,
    user: (String?) -> UserEntity?,
    labelName: (String) -> String?
) -> String {
    switch event.type {
    case "status_changed":
        guard let to = macEventField(event.payload, "to") else { return "changed the status" }
        if let from = macEventField(event.payload, "from") {
            return "changed status from \(macStatusLabel(from)) to \(macStatusLabel(to))"
        }
        return "changed status to \(macStatusLabel(to))"
    case "assignee_changed":
        guard let to = macEventField(event.payload, "to") else { return "unassigned this issue" }
        if let name = user(to).map({ $0.name ?? $0.email }) {
            return "assigned \(name)"
        }
        return "assigned this issue"
    case "label_added":
        if let id = macEventField(event.payload, "labelId"), let name = labelName(id) {
            return "added label \(name)"
        }
        return "added a label"
    case "label_removed":
        if let id = macEventField(event.payload, "labelId"), let name = labelName(id) {
            return "removed label \(name)"
        }
        return "removed a label"
    case "pr_opened":
        if let n = macEventField(event.payload, "prNumber") { return "opened PR #\(n)" }
        return "opened a pull request"
    case "pr_merged":
        if let n = macEventField(event.payload, "prNumber") { return "merged PR #\(n)" }
        return "merged the pull request"
    default:
        return macEventVerb(event.type)
    }
}

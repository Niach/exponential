import ExpCore
import ExpUI
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
    case "board_moved": return "moved this to another board"
    default: return type.replacingOccurrences(of: "_", with: " ")
    }
}

/// Human label for an issue_status enum value.
func statusLabel(_ s: String) -> String {
    switch s {
    case "backlog": return "Backlog"
    // EXP-685: `todo` is retired from the vocabulary, but HISTORIC
    // status_changed events carry the bare enum payload and must still read
    // "Todo" instead of falling through to the generic capitalizer.
    case "todo": return "Todo"
    case "in_progress": return "In Progress"
    case "in_review": return "In Review"
    case "done": return "Done"
    case "cancelled": return "Cancelled"
    case "duplicate": return "Duplicate"
    default: return s.replacingOccurrences(of: "_", with: " ").capitalized
    }
}

/// Capitalized wire priority value for the timeline row ("urgent" → "Urgent").
func priorityLabel(_ wire: String) -> String {
    wire.prefix(1).uppercased() + wire.dropFirst()
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

/// The leading glyph of one activity row (EXP-595 — web `EventRow` / desktop
/// `EventGlyph` parity): every known event type leads with its shared-registry
/// concept icon in the muted text color, and a status change leads with the
/// TARGET status's real resolved icon in its color (EXP-525).
enum EventGlyph {
    /// Shared-registry icon name, rendered in the muted event-text color.
    case plain(String)
    /// A resolved team status — rendered via its own icon + color.
    case status(ResolvedIssueStatus)
}

/// Glyph for one issue event, mirroring the web `EventRow` icon switch. Nil =
/// no glyph (unknown event types keep the plain timeline dot). `statuses` is
/// the issue's team vocabulary in render order — status changes resolve the
/// payload's `toStatusId` (or the legacy anchor) against it, and the shared
/// fallback chain never fails.
func eventGlyph(
    _ event: IssueEventEntity,
    statuses: [ResolvedIssueStatus]
) -> EventGlyph? {
    switch event.type {
    case "status_changed":
        return .status(IssueStatusResolver.resolve(
            statusId: eventField(event.payload, "toStatusId"),
            anchor: eventField(event.payload, "to"),
            team: statuses
        ))
    case "assignee_changed": return .plain(AppIcons.eventAssigneeChanged)
    case "label_added", "label_removed": return .plain(AppIcons.eventLabelAdded)
    case "board_moved": return .plain(AppIcons.eventBoardMoved)
    case "pr_opened": return .plain(AppIcons.prOpen)
    case "pr_merged": return .plain(AppIcons.prMerged)
    case "priority_changed": return .plain(AppIcons.eventPriorityChanged)
    default: return nil
    }
}

/// A rich activity phrase from the event type + payload (status from→to, PR #N,
/// assigned/unassigned, label name). Resolves user/label names when the maps are
/// supplied; falls back to the generic verb for events without a payload.
/// Mirrors the web activity timeline. Nil = SUPPRESS the row entirely
/// (EXP-530: server `created` events duplicate the locally synthesized
/// "created the issue" row and must never render — not even as the munged
/// verb fallback).
func eventPhrase(
    _ event: IssueEventEntity,
    users: [String: UserEntity],
    labels: [String: LabelEntity]?,
    boards: [String: BoardEntity]? = nil
) -> String? {
    switch event.type {
    case "created":
        // Suppressed: the timeline synthesizes its own creation row from the
        // issue itself (which predates these events and covers old issues).
        return nil
    case "status_changed":
        // EXP-314: newer events carry the real status NAMES (custom statuses
        // have no enum label at all); older rows only have the enum anchors, so
        // fall back to the munge.
        let toName = eventField(event.payload, "toName")
        let fromName = eventField(event.payload, "fromName")
        guard let to = toName ?? eventField(event.payload, "to").map(statusLabel) else {
            return "changed the status"
        }
        if let from = fromName ?? eventField(event.payload, "from").map(statusLabel) {
            return "changed status from \(from) to \(to)"
        }
        return "changed status to \(to)"
    case "assignee_changed":
        guard let to = eventField(event.payload, "to") else { return "unassigned this issue" }
        return "assigned \(memberDisplayName(users[to], id: to))"
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
    case "priority_changed":
        // EXP-530 — mirrors the web row byte-for-byte: capitalized wire
        // values, a missing side reading "None" (web/Android/desktop render
        // both sides unconditionally).
        let from = eventField(event.payload, "from").map(priorityLabel) ?? "None"
        let to = eventField(event.payload, "to").map(priorityLabel) ?? "None"
        return "changed priority from \(from) to \(to)"
    case "pr_opened":
        if let n = eventField(event.payload, "prNumber") { return "opened PR #\(n)" }
        return "opened a pull request"
    case "pr_merged":
        if let n = eventField(event.payload, "prNumber") { return "merged PR #\(n)" }
        return "merged the pull request"
    case "board_moved":
        // EXP-57 — mirrors the web row: a deleted source board leaves no
        // name behind, so fall back generically; the payload's fromIdentifier
        // keeps the row useful either way.
        let fromName = eventField(event.payload, "fromBoardId")
            .flatMap { boards?[$0]?.name } ?? "another board"
        let toName = eventField(event.payload, "toBoardId")
            .flatMap { boards?[$0]?.name } ?? "this board"
        let fromIdentifier = eventField(event.payload, "fromIdentifier")
            .map { " (\($0))" } ?? ""
        return "moved this from \(fromName)\(fromIdentifier) to \(toName)"
    default:
        return eventVerb(event.type)
    }
}

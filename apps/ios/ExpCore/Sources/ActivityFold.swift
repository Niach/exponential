import Foundation

// EXP-900: activity folding — READ-TIME ONLY. `issue_events` rows are never
// deleted or rewritten; every client folds the synced rows itself with this
// pure function (mirrored: web `lib/activity/fold.ts`, desktop
// `domain::activity_fold`, Android `domain/ActivityFold.kt`), all locked
// against `packages/domain-contract/fixtures/activity-fold.json`. A "Show all"
// toggle on the timeline header returns the unfolded list (EXP-468).
//
// THE FOLD RULE: a run of events on the SAME issue and the SAME field by the
// SAME actor, with no event (or comment, see `ActivityBarrier`) from any OTHER
// actor on that issue in between, first-to-last within `foldWindowMs`,
// collapses to its NET effect. A net of "nothing changed" (A → B → A)
// disappears entirely; a net A → C shows as ONE event carrying the first
// event's `from*` payload keys and the last event's everything else,
// timestamped at the LAST event and keyed by ITS id. `created`, `pr_opened`
// and `pr_merged` never fold (`foldFieldKey` → nil) and never break a run of
// the same actor; a run spanning MORE than the window from first to last is
// left alone entirely (no partial fold). An event without an actor never
// folds and breaks every open run on its issue.
//
// "Field" derives from the event type: `status_changed` → status,
// `assignee_changed` → assignee, `priority_changed` → priority,
// `estimate_changed` → estimate, `board_moved` → board,
// `label_added`/`label_removed` → that ONE label (`payload.labelId`; an add
// and a remove of the same label cancel),
// `relation_added`/`relation_removed` → that one relation
// (`payload.type` + `payload.relatedIssueId`).

/// Something that is not an event but still breaks another actor's run: a
/// comment. "Reviewer says no" is usually a comment, not a status change, so
/// the timeline passes its comments here and the dev's progress → review →
/// progress round trip stays visible.
public struct ActivityBarrier: Sendable {
    public let issueId: String
    public let actorUserId: String?
    public let createdAt: String

    public init(issueId: String, actorUserId: String?, createdAt: String) {
        self.issueId = issueId
        self.actorUserId = actorUserId
        self.createdAt = createdAt
    }
}

/// Namespace for the fold's one tunable. The rule itself is the two free
/// functions below, named like their web counterparts.
public enum ActivityFold {
    /// First-to-last span a run may cover and still fold (`FOLD_WINDOW_MS`).
    public static let foldWindowMs: Int = 10 * 60 * 1000
}

/// The field an event edits, or nil for an event that never folds.
public func foldFieldKey(_ event: IssueEventEntity) -> String? {
    let payload = activityPayload(of: event)
    switch event.type {
    case DomainContract.issueEventTypeStatusChanged:
        return "status"
    case DomainContract.issueEventTypeAssigneeChanged:
        return "assignee"
    case DomainContract.issueEventTypePriorityChanged:
        return "priority"
    case DomainContract.issueEventTypeEstimateChanged:
        return "estimate"
    case DomainContract.issueEventTypeBoardMoved:
        return "board"
    case DomainContract.issueEventTypeLabelAdded,
         DomainContract.issueEventTypeLabelRemoved:
        guard let labelId = activityOptionalString(payload["labelId"]) else { return nil }
        return "label:\(labelId)"
    case DomainContract.issueEventTypeRelationAdded,
         DomainContract.issueEventTypeRelationRemoved:
        guard let type = activityOptionalString(payload["type"]),
              let relatedIssueId = activityOptionalString(payload["relatedIssueId"])
        else { return nil }
        return "relation:\(type):\(relatedIssueId)"
    default:
        return nil
    }
}

/// Input in chronological order (oldest first); output likewise. `barriers`
/// (comments) only ever break runs, they are never returned. Inputs are never
/// mutated — a folded row is a fresh `IssueEventEntity` copied off the run's
/// LAST event.
public func foldActivity(
    _ events: [IssueEventEntity],
    barriers: [ActivityBarrier] = []
) -> [IssueEventEntity] {
    var steps: [ActivityStep] = []
    steps.reserveCapacity(events.count + barriers.count)
    // An unparseable timestamp can't be ordered; it inherits the previous
    // event's instant so it stays where the (chronological) input put it, and
    // `parsed == false` keeps it out of every run (it never folds).
    var carry: Double = 0
    for (index, event) in events.enumerated() {
        let at = activityEpochMs(event.createdAt)
        carry = at ?? carry
        steps.append(.event(at: carry, index: index, event: event, parsed: at != nil))
    }
    for (index, barrier) in barriers.enumerated() {
        // An unparseable barrier has no place in the order at all; it is
        // dropped rather than guessed at.
        guard let at = activityEpochMs(barrier.createdAt) else { continue }
        // A barrier at the same instant as an event sorts BEFORE it, so a
        // comment written together with a change still separates it from
        // what follows.
        steps.append(.barrier(at: at, index: index - barriers.count, barrier: barrier))
    }
    steps.sort { lhs, rhs in
        lhs.at == rhs.at ? lhs.index < rhs.index : lhs.at < rhs.at
    }

    // Open runs keyed by `${issueId} ${actor} ${field}`.
    var open: [String: ActivityRun] = [:]
    var out: [ActivityOut] = []

    func breakOthers(issueId: String, actorUserId: String?) {
        // Keys first: the dictionary is mutated in the same pass.
        let broken = open.keys.filter { key in
            guard let run = open[key], run.issueId == issueId else { return false }
            if let actorUserId, run.actorUserId == actorUserId { return false }
            return true
        }
        for key in broken {
            guard let run = open.removeValue(forKey: key) else { continue }
            flushActivityRun(run, into: &out)
        }
    }

    for step in steps {
        switch step {
        case let .barrier(_, _, barrier):
            breakOthers(issueId: barrier.issueId, actorUserId: barrier.actorUserId)
        case let .event(at, index, event, parsed):
            let actor = event.actorUserId
            breakOthers(issueId: event.issueId, actorUserId: actor)
            let field = actor == nil ? nil : foldFieldKey(event)
            guard let actor, let field, parsed else {
                out.append(ActivityOut(event: event, at: at, index: index))
                continue
            }
            let key = "\(event.issueId) \(actor) \(field)"
            let entry = ActivityRunEvent(event: event, at: at, index: index)
            if open[key] != nil {
                open[key]?.events.append(entry)
            } else {
                open[key] = ActivityRun(issueId: event.issueId, actorUserId: actor, events: [entry])
            }
        }
    }
    // Dictionary order is arbitrary, but `out` is sorted below, so the flush
    // order of the runs left open never shows.
    for run in open.values { flushActivityRun(run, into: &out) }

    out.sort { lhs, rhs in
        lhs.at == rhs.at ? lhs.index < rhs.index : lhs.at < rhs.at
    }
    return out.map(\.event)
}

// MARK: - Internals

private enum ActivityStep {
    case event(at: Double, index: Int, event: IssueEventEntity, parsed: Bool)
    case barrier(at: Double, index: Int, barrier: ActivityBarrier)

    var at: Double {
        switch self {
        case let .event(at, _, _, _): return at
        case let .barrier(at, _, _): return at
        }
    }

    var index: Int {
        switch self {
        case let .event(_, index, _, _): return index
        case let .barrier(_, index, _): return index
        }
    }
}

private struct ActivityRunEvent {
    let event: IssueEventEntity
    let at: Double
    let index: Int
}

private struct ActivityRun {
    let issueId: String
    let actorUserId: String
    var events: [ActivityRunEvent]
}

private struct ActivityOut {
    let event: IssueEventEntity
    let at: Double
    let index: Int
}

private func flushActivityRun(_ run: ActivityRun, into out: inout [ActivityOut]) {
    let events = run.events
    guard let first = events.first, let last = events.last else { return }
    if events.count == 1 {
        out.append(ActivityOut(event: first.event, at: first.at, index: first.index))
        return
    }
    // A run too long from end to end is left ALONE — never partially folded.
    if last.at - first.at > Double(ActivityFold.foldWindowMs) {
        for entry in events {
            out.append(ActivityOut(event: entry.event, at: entry.at, index: entry.index))
        }
        return
    }
    // Net effect of nothing: the whole run disappears.
    if activityBefore(first.event) == activityAfter(last.event) { return }
    out.append(
        ActivityOut(
            event: mergedActivityEvent(first: first.event, last: last.event),
            at: last.at,
            index: last.index
        )
    )
}

/// The run's one surviving row: the LAST event wearing the FIRST event's
/// `from*` payload keys.
private func mergedActivityEvent(
    first: IssueEventEntity,
    last: IssueEventEntity
) -> IssueEventEntity {
    var merged: [String: Any] = [:]
    for (key, value) in activityPayload(of: last) where !key.hasPrefix("from") {
        merged[key] = value
    }
    for (key, value) in activityPayload(of: first) where key.hasPrefix("from") {
        merged[key] = value
    }
    // `.sortedKeys` so the stored JSON text is deterministic across folds.
    let payload = (try? JSONSerialization.data(withJSONObject: merged, options: [.sortedKeys]))
        .flatMap { String(data: $0, encoding: .utf8) }
    return IssueEventEntity(
        id: last.id,
        issueId: last.issueId,
        teamId: last.teamId,
        actorUserId: last.actorUserId,
        type: last.type,
        payload: payload ?? last.payload,
        createdAt: last.createdAt,
        updatedAt: last.updatedAt
    )
}

/// The event's JSON payload as a dictionary; `{}` when absent or unreadable.
private func activityPayload(of event: IssueEventEntity) -> [String: Any] {
    guard let raw = event.payload, let data = raw.data(using: .utf8) else { return [:] }
    let object = try? JSONSerialization.jsonObject(with: data)
    return object as? [String: Any] ?? [:]
}

/// A non-empty string value, or nil (JSON null and non-strings included).
private func activityOptionalString(_ value: Any?) -> String? {
    guard let text = value as? String, !text.isEmpty else { return nil }
    return text
}

/// The web's `String(value ?? "")`: JSON null / a missing key reads as "".
private func activityStringified(_ value: Any?) -> String {
    guard let value, !(value is NSNull) else { return "" }
    if let text = value as? String { return text }
    if let number = value as? NSNumber {
        if CFGetTypeID(number) == CFBooleanGetTypeID() { return number.boolValue ? "true" : "false" }
        let double = number.doubleValue
        if double == double.rounded(), abs(double) < 1e15 { return String(number.int64Value) }
        return String(double)
    }
    return String(describing: value)
}

/// `a ?? b` over a payload the way JS `??` reads it: a JSON null falls through.
private func activityCoalesce(_ payload: [String: Any], _ keys: String...) -> Any? {
    for key in keys {
        if let value = payload[key], !(value is NSNull) { return value }
    }
    return nil
}

// The "before" side of the first event and the "after" side of the last one,
// as comparable strings. Equal = the run changed nothing. A status compares
// on the precise `statusId` pair when the payload carries one (EXP-314) and
// on the legacy enum otherwise; presence toggles (labels, relations) compare
// as present/absent.
private func activityBefore(_ event: IssueEventEntity) -> String {
    let payload = activityPayload(of: event)
    switch event.type {
    case DomainContract.issueEventTypeStatusChanged:
        return activityStringified(activityCoalesce(payload, "fromStatusId", "from"))
    case DomainContract.issueEventTypeAssigneeChanged,
         DomainContract.issueEventTypePriorityChanged,
         DomainContract.issueEventTypeEstimateChanged:
        return activityStringified(activityCoalesce(payload, "from"))
    case DomainContract.issueEventTypeBoardMoved:
        return activityStringified(activityCoalesce(payload, "fromBoardId"))
    case DomainContract.issueEventTypeLabelAdded,
         DomainContract.issueEventTypeRelationAdded:
        return "absent"
    case DomainContract.issueEventTypeLabelRemoved,
         DomainContract.issueEventTypeRelationRemoved:
        return "present"
    default:
        return ""
    }
}

private func activityAfter(_ event: IssueEventEntity) -> String {
    let payload = activityPayload(of: event)
    switch event.type {
    case DomainContract.issueEventTypeStatusChanged:
        return activityStringified(activityCoalesce(payload, "toStatusId", "to"))
    case DomainContract.issueEventTypeAssigneeChanged,
         DomainContract.issueEventTypePriorityChanged,
         DomainContract.issueEventTypeEstimateChanged:
        return activityStringified(activityCoalesce(payload, "to"))
    case DomainContract.issueEventTypeBoardMoved:
        return activityStringified(activityCoalesce(payload, "toBoardId"))
    case DomainContract.issueEventTypeLabelAdded,
         DomainContract.issueEventTypeRelationAdded:
        return "present"
    case DomainContract.issueEventTypeLabelRemoved,
         DomainContract.issueEventTypeRelationRemoved:
        return "absent"
    default:
        return ""
    }
}

/// Both wire forms (`2026-09-19T10:00:00Z` and Electric's
/// `2026-09-19 10:00:00+00`) go through the ONE tolerant parser.
private func activityEpochMs(_ createdAt: String) -> Double? {
    guard let date = WireTimestamps.parse(createdAt) else { return nil }
    // Whole milliseconds, like JS `Date.getTime()`: the window compare is a
    // strict `>` and must not trip on a float remainder.
    return (date.timeIntervalSince1970 * 1000).rounded()
}

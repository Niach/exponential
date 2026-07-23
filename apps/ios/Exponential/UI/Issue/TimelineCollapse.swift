import ExpCore
import Foundation

// Pure timeline shaping for the redesigned Activity section (EXP-240) —
// view-free so the collapse rule stays trivially testable and identical to
// Android's TimelineCollapse.

/// One chronological activity item, pre-merge: the synthesized created row,
/// regular comments, and issue events.
enum TimelineItem: Identifiable {
    /// "«creator» created the issue" — synthesized from the issue row itself
    /// (widget issues have no user creator and read "Feedback widget").
    case created(actorId: String?, createdAt: String, isWidget: Bool)
    case comment(CommentEntity)
    case event(IssueEventEntity)

    var id: String {
        switch self {
        case .created: return "created"
        case .comment(let c): return "c-\(c.id)"
        case .event(let e): return "e-\(e.id)"
        }
    }

    var createdAt: String {
        switch self {
        case .created(_, let at, _): return at
        case .comment(let c): return c.createdAt
        case .event(let e): return e.createdAt
        }
    }
}

/// A display row after collapse: either a plain item or a run of consecutive
/// events folded behind a "Show N activity items" expander.
enum TimelineDisplayRow: Identifiable {
    case item(TimelineItem)
    case collapsedRun(key: String, events: [IssueEventEntity])

    var id: String {
        switch self {
        case .item(let item): return item.id
        case .collapsedRun(let key, _): return "run-\(key)"
        }
    }
}

/// Fold runs of MORE than two consecutive events behind an expander row. The
/// run key is the FIRST event's id, so sync re-emits (rows arriving one by one
/// or re-sorting) never reset an expansion the user already opened. The
/// created item and comments never collapse — they always break a run.
func collapseTimeline(_ items: [TimelineItem], expandedRuns: Set<String>) -> [TimelineDisplayRow] {
    var rows: [TimelineDisplayRow] = []
    var run: [IssueEventEntity] = []

    func flushRun() {
        guard !run.isEmpty else { return }
        let key = run[0].id
        if run.count > 2, !expandedRuns.contains(key) {
            rows.append(.collapsedRun(key: key, events: run))
        } else {
            rows.append(contentsOf: run.map { .item(.event($0)) })
        }
        run = []
    }

    for item in items {
        if case .event(let event) = item {
            run.append(event)
        } else {
            flushRun()
            rows.append(.item(item))
        }
    }
    flushRun()
    return rows
}

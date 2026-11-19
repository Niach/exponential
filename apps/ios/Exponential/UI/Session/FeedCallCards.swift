import ExpCore
import ExpUI
import SwiftUI

/// #EXP-1943 (session-card.json THE LIVE CHAT): a call in the live feed. A
/// call's report reaches the chat's agent as a relay input (`Report from
/// Call (session <id>): <summary>`); the feed draws THE call card of that
/// session at its FIRST report (caption = the newest such report body in the
/// feed), every later report of the same call draws nothing, and a RUNNING
/// call with no report in the loaded feed draws its card after the feed's
/// last row. Its relay bubble never shows.
struct FeedCalls: Equatable {
    /// The entry that draws a call's card → that call session's id.
    var anchors: [Int: String] = [:]
    /// The later reports of a call: drawn as nothing.
    var hidden: Set<Int> = []
    /// Per call: the newest report body in the feed.
    var captions: [String: String] = [:]
    /// Running calls without a report in the loaded feed (tree order).
    var trailing: [String] = []

    static func place(_ entries: [TranscriptEntry], calls: [SessionEntity]) -> FeedCalls {
        var out = FeedCalls()
        guard !calls.isEmpty else { return out }
        let ids = Set(calls.map(\.id))
        for entry in entries {
            guard case let .single(.userMessage(_, text, _, _)) = entry.row,
                  let callId = SessionTree.feedCallSession(text, callIds: ids) else { continue }
            if out.captions[callId] == nil {
                out.anchors[entry.id] = callId
            } else {
                out.hidden.insert(entry.id)
            }
            out.captions[callId] = TranscriptGrouping.crossSessionInput(text)?.body ?? ""
        }
        out.trailing = calls
            .filter { out.captions[$0.id] == nil && SessionTree.callRunning(TreeSession(sessionRow: $0)) }
            .map(\.id)
        return out
    }
}

/// THE call card in the feed: the session card (`SessionCardView`) built by
/// `SessionTree.callCard`, ticking every second while the call runs.
struct FeedCallCard: View {
    let session: SessionEntity
    let caption: String?
    let onOpen: () -> Void

    var body: some View {
        let tree = TreeSession(sessionRow: session)
        if SessionTree.callRunning(tree) {
            TimelineView(.periodic(from: .now, by: 1)) { context in
                SessionCardView(card: SessionTree.callCard(tree, summary: caption, now: context.date), onOpen: onOpen)
            }
        } else {
            SessionCardView(card: SessionTree.callCard(tree, summary: caption, now: Date()), onOpen: onOpen)
        }
    }
}

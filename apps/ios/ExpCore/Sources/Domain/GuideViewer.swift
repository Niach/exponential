import Foundation

// EXP-1283: THE Guide picture viewer ×4. A 1:1 port of the TS reference
// `packages/domain-contract/src/guide-viewer.ts` (desktop
// `domain::session_results`, Android `domain/SessionResults.kt`), byte-locked
// by fixtures/session-results.json `viewer.cases` (`GuideViewerTests`).

public struct GuideViewerState: Equatable, Sendable {
    /// 1-based place of the picture among every Guide picture.
    public let index: Int
    public let count: Int
    public let topic: String
    public let label: String
    /// The picture's topic text (its first non-blank text entry), trimmed.
    public let text: String?
    public let prevId: String?
    public let nextId: String?

    public init(index: Int, count: Int, topic: String, label: String, text: String?, prevId: String?, nextId: String?) {
        self.index = index
        self.count = count
        self.topic = topic
        self.label = label
        self.text = text
        self.prevId = prevId
        self.nextId = nextId
    }
}

public enum GuideViewer {
    /// The viewer over `attachmentId`: every picture of the Guide in the
    /// Guide's order (the first Summary topic leads, then topics first-seen,
    /// pictures in publish order), no wrap at the ends; nil when the
    /// attachment is not a Guide picture.
    public static func state(records: [[String: Any]], attachmentId: String) -> GuideViewerState? {
        func str(_ value: Any?) -> String? {
            guard let value = value as? String,
                  !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            else { return nil }
            return value
        }
        var topics: [String] = []
        var texts: [String: String] = [:]
        var pictures: [String: [(label: String, attachmentId: String)]] = [:]
        for entry in records {
            guard let topic = str(entry["topic"]) else { continue }
            let id = str(entry["attachmentId"])
            let label = str(entry["label"])
            let text = str(entry["text"])
            if let id, let label {
                if !topics.contains(topic) { topics.append(topic) }
                pictures[topic, default: []].append((label, id))
            } else if let text {
                if !topics.contains(topic) { topics.append(topic) }
                if texts[topic] == nil { texts[topic] = text.trimmingCharacters(in: .whitespacesAndNewlines) }
            }
        }
        let isSummary = { (topic: String) in
            topic.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() == "summary"
        }
        let ordered: [String]
        if let lead = topics.first(where: isSummary) {
            ordered = [lead] + topics.filter { $0 != lead }
        } else {
            ordered = topics
        }
        let flat = ordered.flatMap { topic in
            (pictures[topic] ?? []).map { (topic: topic, label: $0.label, attachmentId: $0.attachmentId) }
        }
        guard let at = flat.firstIndex(where: { $0.attachmentId == attachmentId }) else { return nil }
        let picture = flat[at]
        return GuideViewerState(
            index: at + 1,
            count: flat.count,
            topic: picture.topic,
            label: picture.label,
            text: texts[picture.topic],
            prevId: at > 0 ? flat[at - 1].attachmentId : nil,
            nextId: at < flat.count - 1 ? flat[at + 1].attachmentId : nil
        )
    }

    /// The synced `sessions.guide` jsonb TEXT form.
    public static func state(guide raw: String?, attachmentId: String) -> GuideViewerState? {
        guard let raw, let data = raw.data(using: .utf8),
              let list = (try? JSONSerialization.jsonObject(with: data)) as? [Any]
        else { return nil }
        return state(records: list.compactMap { $0 as? [String: Any] }, attachmentId: attachmentId)
    }
}

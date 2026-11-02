import Foundation

/// #EXP-1623 (Android #EXP-1522): one viewer frame off the steer feed, read
/// ONCE into plain values. A join replay is thousands of frames in one burst,
/// and the JSON parse plus the activity decode of each one ran on the main
/// thread; `AgentSessionModel` now parses a burst off main with this and
/// folds only the decoded events there. Pure and `Sendable`, so it can.
public struct SteerWireFrame: Equatable, Sendable {
    /// One decoded activity event and the raw `at` stamp its edge carried
    /// (the subagent spans read it).
    public struct Activity: Equatable, Sendable {
        public let event: AgentActivityEvent?
        public let at: Int64?

        public init(event: AgentActivityEvent?, at: Int64? = nil) {
            self.event = event
            self.at = at
        }

        init(raw: [String: Any]?) {
            event = AgentActivityDecoder.decode(raw)
            at = (raw?["at"] as? NSNumber)?.int64Value
        }
    }

    public let t: String
    public let seq: Int?
    /// `activity`'s payload.
    public let activity: Activity
    /// `activity_synced`: the span it replayed.
    public let firstSeq: Int?
    public let truncated: Bool
    /// `history_chunk`: its request, whether it is the last one, its page.
    public let requestId: String?
    public let done: Bool
    public let page: [Activity]
    public let pageSeqs: [Int]
    /// `bye`'s outcome; `error`'s code and message.
    public let outcome: String?
    public let code: String?
    public let message: String?

    public init(
        t: String, seq: Int? = nil, activity: Activity = Activity(event: nil),
        firstSeq: Int? = nil, truncated: Bool = false, requestId: String? = nil,
        done: Bool = false, page: [Activity] = [], pageSeqs: [Int] = [],
        outcome: String? = nil, code: String? = nil, message: String? = nil
    ) {
        self.t = t
        self.seq = seq
        self.activity = activity
        self.firstSeq = firstSeq
        self.truncated = truncated
        self.requestId = requestId
        self.done = done
        self.page = page
        self.pageSeqs = pageSeqs
        self.outcome = outcome
        self.code = code
        self.message = message
    }

    /// nil = not JSON, or no `t`.
    public static func parse(_ text: String) -> SteerWireFrame? {
        guard let data = text.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let t = obj["t"] as? String else { return nil }
        let isChunk = t == "history_chunk"
        return SteerWireFrame(
            t: t,
            seq: obj["seq"] as? Int,
            activity: t == "activity" ? Activity(raw: obj["event"] as? [String: Any]) : Activity(event: nil),
            firstSeq: obj["firstSeq"] as? Int,
            truncated: obj["truncated"] as? Bool == true,
            requestId: obj["requestId"] as? String,
            done: obj["done"] as? Bool == true,
            page: isChunk ? ((obj["events"] as? [[String: Any]]) ?? []).map { Activity(raw: $0) } : [],
            pageSeqs: isChunk ? ((obj["seqs"] as? [Int]) ?? []) : [],
            outcome: obj["outcome"] as? String,
            code: obj["code"] as? String,
            message: obj["message"] as? String
        )
    }

    /// The burst parse: frames that are not JSON drop out, order is kept.
    public static func parse(_ texts: [String]) -> [SteerWireFrame] {
        texts.compactMap(parse)
    }
}

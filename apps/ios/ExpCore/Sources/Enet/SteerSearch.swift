import Foundation

// #EDE-96 (packages/domain-contract/fixtures/steer-enet.json SEARCH): a
// viewer asks the device that holds the session's journal to search the
// transcript. `search {requestId, query, beforeSeq?, limit}` goes out on the
// feed like `history_page`; the device answers the asker only with ONE
// `search_result`. Mirrors the TS reference `steerSearch` frame types.

/// One match: the ROW's `seq` (a tool's output = its `tool` row), the row
/// `kind` and a `snippet` with the match's offset and length in CODE POINTS
/// (unicode scalars) of the snippet.
public struct SteerSearchHit: Equatable, Sendable {
    public let seq: Int
    public let kind: String
    public let snippet: String
    public let matchStart: Int
    public let matchLen: Int

    public init(seq: Int, kind: String, snippet: String, matchStart: Int, matchLen: Int) {
        self.seq = seq
        self.kind = kind
        self.snippet = snippet
        self.matchStart = matchStart
        self.matchLen = matchLen
    }
}

/// The device's answer. `done == false` = ask again with
/// `beforeSeq = scannedToSeq`; `total` only when the whole journal was
/// searched in this answer.
public struct SteerSearchResult: Equatable, Sendable {
    public let hits: [SteerSearchHit]
    public let total: Int?
    public let done: Bool
    public let scannedToSeq: Int

    public init(hits: [SteerSearchHit], total: Int?, done: Bool, scannedToSeq: Int) {
        self.hits = hits
        self.total = total
        self.done = done
        self.scannedToSeq = scannedToSeq
    }
}

public enum SteerSearch {
    /// The most hits one answer carries; a viewer asks for that many.
    public static let hitsMax = 50
    /// How long a search waits for its answer (the fixture's `sendTimeoutMs`).
    public static let timeoutMs = SteerEnet.sendTimeoutMs
    /// The hit kinds this build draws; any other is skipped.
    public static let hitKinds: Set<String> = ["user_message", "narration", "tool", "tool_output"]

    /// The viewer's `search` frame (`beforeSeq` nil = from the newest event).
    public static func frame(requestId: String, query: String, beforeSeq: Int?) -> [String: Any] {
        var frame: [String: Any] = ["t": "search", "requestId": requestId, "query": query, "limit": hitsMax]
        if let beforeSeq { frame["beforeSeq"] = beforeSeq }
        return frame
    }

    /// A `search_result` frame's body; nil = not one, or malformed. Hits of a
    /// kind this build does not know are skipped.
    public static func parseResult(_ obj: [String: Any]) -> SteerSearchResult? {
        guard obj["t"] as? String == "search_result",
              let done = obj["done"] as? Bool,
              let scannedToSeq = obj["scannedToSeq"] as? Int,
              let hits = obj["hits"] as? [[String: Any]] else { return nil }
        return SteerSearchResult(
            hits: hits.compactMap { hit in
                guard let seq = hit["seq"] as? Int,
                      let kind = hit["kind"] as? String, hitKinds.contains(kind),
                      let snippet = hit["snippet"] as? String,
                      let matchStart = hit["matchStart"] as? Int,
                      let matchLen = hit["matchLen"] as? Int else { return nil }
                return SteerSearchHit(seq: seq, kind: kind, snippet: snippet, matchStart: matchStart, matchLen: matchLen)
            },
            total: obj["total"] as? Int,
            done: done,
            scannedToSeq: scannedToSeq
        )
    }
}

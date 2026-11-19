import Foundation

/// #EDE-130 (`packages/domain-contract/fixtures/find-in-chat.json`, its
/// `$comment` = the spec ×4): find in chat on a phone. Pure: the menu entry,
/// the query, the stops in reading order, the counter, the steps, keeping
/// the current stop while answers arrive, what a landing does, the status
/// line and the keys. Mirrors packages/domain-contract/src/find-in-chat.ts
/// and desktop `domain::find_in_chat`.
public enum FindInChat {
    public static let queryMaxChars = 200
    public static let hitsMax = 50
    public static let debounceMs = 150

    public enum Copy {
        public static let menu = "Search in chat"
        public static let placeholder = "Search in chat"
        public static let counter = "{n} of {m}"
        public static let counterCompact = "{n}/{m}"
        public static let noResults = "No results"
        public static let searchingOlder = "Searching older messages…"
        public static let searchingDevice = "Searching older messages on {device}…"
        public static let deviceOffline = "{device} is offline: older tool output not searched"
        public static let outputHintOne = "1 match in output"
        public static let outputHintMany = "{n} matches in output"
        public static let previous = "Previous match"
        public static let next = "Next match"
        public static let close = "Close search"
    }

    public enum TestIds {
        public static let bar = "find-in-chat"
        public static let input = "find-in-chat-input"
        public static let counter = "find-in-chat-counter"
        public static let previous = "find-in-chat-previous"
        public static let next = "find-in-chat-next"
        public static let close = "find-in-chat-close"
        public static let status = "find-in-chat-status"
        public static let outputHint = "find-in-chat-output-hint"
    }

    public enum Shortcut {
        public static let mac = "⌘F"
        public static let other = "Ctrl F"
    }

    private static let kindsWithTranscript: Set<String> = ["session", "agent", "assistant"]

    /// The `…` menu with `search` first in its own group (an app: unchanged).
    public static func menu<E>(kind: String, entries: [E], search: E, separator: E) -> [E] {
        guard kindsWithTranscript.contains(kind) else { return entries }
        return entries.isEmpty ? [search] : [search, separator] + entries
    }

    /// The query as the chat store takes it: ASCII whitespace runs = one
    /// space, trimmed, the first `queryMaxChars` code points.
    public static func query(_ raw: String) -> String {
        let ascii: Set<Unicode.Scalar> = [" ", "\t", "\n", "\r", "\u{0C}", "\u{0B}"]
        var out = String.UnicodeScalarView()
        var inRun = false
        for s in raw.unicodeScalars {
            if ascii.contains(s) {
                if !inRun { out.append(" ") }
                inRun = true
            } else {
                out.append(s)
                inRun = false
            }
        }
        var scalars = Array(out)
        // JS `trim()`: Unicode white space and line terminators, the BOM.
        let trims = CharacterSet.whitespacesAndNewlines.union(CharacterSet(charactersIn: "\u{FEFF}"))
        while let first = scalars.first, trims.contains(first) { scalars.removeFirst() }
        while let last = scalars.last, trims.contains(last) { scalars.removeLast() }
        var cut = String.UnicodeScalarView()
        cut.append(contentsOf: scalars.prefix(queryMaxChars))
        return String(cut)
    }

    /// A loaded row, top to bottom. `count` = its painted highlight count.
    public struct Row: Equatable, Sendable {
        public var key: String
        public var count: Int
        public var group: String?
        /// A tool row whose output matches: one more stop, ringing the row.
        public var outputHit: Bool

        public init(key: String, count: Int, group: String? = nil, outputHit: Bool = false) {
            self.key = key
            self.count = count
            self.group = group
            self.outputHit = outputHit
        }
    }

    /// An older hit (store, then device), newest first.
    public struct OlderHit: Equatable, Sendable {
        public var key: String
        public var output: Bool

        public init(key: String, output: Bool = false) {
            self.key = key
            self.output = output
        }

        /// The seq an event key ends with (nil = a message key).
        public var seq: Int? {
            guard key.hasPrefix("e:"), let colon = key.lastIndex(of: ":") else { return nil }
            return Int(key[key.index(after: colon)...])
        }
    }

    /// One stop of the counter.
    public struct Stop: Equatable, Hashable, Sendable {
        public var key: String
        public var local: Int
        public var loaded: Bool
        public var group: String?
        public var output: Bool

        public init(key: String, local: Int, loaded: Bool, group: String?, output: Bool) {
            self.key = key
            self.local = local
            self.loaded = loaded
            self.group = group
            self.output = output
        }

        /// The same stop (key, local index, output) whatever its paging.
        public func same(_ other: Stop) -> Bool {
            key == other.key && local == other.local && output == other.output
        }
    }

    /// Every stop in reading order (newest last).
    public static func stops(rows: [Row], older: [OlderHit]) -> [Stop] {
        let loaded = Set(rows.map(\.key))
        var seen = Set<String>()
        var out: [Stop] = []
        for hit in older where !loaded.contains(hit.key) && !seen.contains(hit.key) {
            seen.insert(hit.key)
            out.append(Stop(key: hit.key, local: 0, loaded: false, group: nil, output: hit.output))
        }
        out.reverse()
        for row in rows {
            for local in 0..<max(row.count, 0) {
                out.append(Stop(key: row.key, local: local, loaded: true, group: row.group, output: false))
            }
            if row.outputHit {
                out.append(Stop(key: row.key, local: 0, loaded: true, group: row.group, output: true))
            }
        }
        return out
    }

    /// The counter text (`""` = none shown). `current` is 0-based.
    public static func counter(current: Int?, total: Int, done: Bool, compact: Bool) -> String {
        guard total > 0, let current else { return done && total == 0 ? Copy.noResults : "" }
        return (compact ? Copy.counterCompact : Copy.counter)
            .replacingOccurrences(of: "{n}", with: String(current + 1))
            .replacingOccurrences(of: "{m}", with: String(total))
    }

    public enum Step: String, Sendable {
        case start, previous, next
    }

    /// The stop after a step (wraps); `start` = the newest.
    public static func step(current: Int?, total: Int, _ step: Step) -> Int? {
        guard total > 0 else { return nil }
        guard step != .start, let current else { return total - 1 }
        if step == .previous { return (current - 1 + total) % total }
        return (current + 1) % total
    }

    /// The same stop in the new list (same key, local, output), else the newest.
    public static func keep(before: [Stop], current: Int?, after: [Stop]) -> Int? {
        guard !after.isEmpty else { return nil }
        if let current, before.indices.contains(current) {
            let was = before[current]
            if let at = after.firstIndex(where: { $0.same(was) }) { return at }
        }
        return after.count - 1
    }

    /// What landing on a stop does. `current` = the row's `highlight.current`.
    public struct Landing: Equatable, Sendable {
        public var openGroup: String?
        public var pageIn: Bool
        /// Always the centre (`scroll: "centre"`).
        public var ring: Bool
        public var current: Int?
    }

    public static func land(_ stop: Stop) -> Landing {
        guard stop.loaded else { return Landing(openGroup: nil, pageIn: true, ring: true, current: nil) }
        return Landing(openGroup: stop.group, pageIn: false, ring: stop.output, current: stop.output ? nil : stop.local)
    }

    public enum Phase: String, Sendable {
        case none, searching, done, offline
    }

    /// The status line under the bar (nil = none).
    public static func status(store: Phase, device: Phase, deviceName: String) -> String? {
        if device == .offline { return Copy.deviceOffline.replacingOccurrences(of: "{device}", with: deviceName) }
        if device == .searching { return Copy.searchingDevice.replacingOccurrences(of: "{device}", with: deviceName) }
        if store == .searching { return Copy.searchingOlder }
        return nil
    }

    public enum KeyAction: String, Sendable {
        case find, global, previous, next, close
    }

    /// A key press: `mod` = Cmd on Apple, else Ctrl.
    public static func key(_ key: String, mod: Bool, shift: Bool, inChat: Bool, inField: Bool) -> KeyAction? {
        let key = key.utf16.count == 1 ? key.lowercased() : key
        if mod, key == "k" { return .global }
        if mod, key == "f" { return inChat ? .find : .global }
        guard inField, !mod else { return nil }
        if key == "Enter" { return shift ? .next : .previous }
        if key == "Escape" { return .close }
        return nil
    }

    /// The tool row's hint beside its title (`n` = its output's matches).
    public static func outputHint(_ n: Int) -> String {
        n == 1 ? Copy.outputHintOne : Copy.outputHintMany.replacingOccurrences(of: "{n}", with: String(n))
    }

    /// The menu's shortcut hint (phones show none).
    public static func shortcut(apple: Bool) -> String {
        apple ? Shortcut.mac : Shortcut.other
    }

    /// A hit's key: an event (seq = the tool row's for output) or a message.
    public static func eventKey(sessionId: String, seq: Int) -> String { "e:\(sessionId):\(seq)" }
    public static func messageKey(_ messageId: String) -> String { "m:\(messageId)" }

    /// #EXP-1998: a store hit as `older` reads it (an event's carries its
    /// `seq`, a synced message's its `messageId`).
    public struct StoreHit: Equatable, Sendable {
        public var sessionId: String
        public var seq: Int?
        public var messageId: String?
        public var kind: String

        public init(sessionId: String, seq: Int? = nil, messageId: String? = nil, kind: String) {
            self.sessionId = sessionId
            self.seq = seq
            self.messageId = messageId
            self.kind = kind
        }
    }

    /// #EXP-1998: a device's hit (always the shown session's event).
    public struct DeviceHit: Equatable, Sendable {
        public var seq: Int
        public var kind: String

        public init(seq: Int, kind: String) {
            self.seq = seq
            self.kind = kind
        }
    }

    /// #EXP-1998: the synced messages the host draws as bubbles (nil = a
    /// live transcript, which holds them as its own rows): `loaded` = every
    /// loaded synced message of the session, `drawable` = those it would draw
    /// with no cap, `more` = older ones exist unloaded.
    public struct Synced: Equatable, Sendable {
        public var loaded: [String]
        public var drawable: [String]
        public var more: Bool

        public init(loaded: [String], drawable: [String], more: Bool) {
            self.loaded = loaded
            self.drawable = drawable
            self.more = more
        }
    }

    /// #EXP-1998 (`olderCases`): the store's and the device's hits OLDER
    /// than the loaded rows, store first, first key wins.
    public static func older(sessionId: String, lowestSeq: Int?, store: [StoreHit], device: [DeviceHit], synced: Synced?) -> [OlderHit] {
        let loaded = Set(synced?.loaded ?? [])
        let drawable = Set(synced?.drawable ?? [])
        var seen = Set<String>()
        var out: [OlderHit] = []
        func push(_ key: String, _ output: Bool) {
            guard !seen.contains(key) else { return }
            seen.insert(key)
            out.append(OlderHit(key: key, output: output))
        }
        func event(_ seq: Int, _ kind: String) {
            if lowestSeq.map({ seq < $0 }) ?? true { push(eventKey(sessionId: sessionId, seq: seq), kind == "tool_output") }
        }
        for hit in store where hit.sessionId == sessionId {
            if let seq = hit.seq {
                event(seq, hit.kind)
            } else if let id = hit.messageId, let synced {
                if loaded.contains(id) ? drawable.contains(id) : synced.more { push(messageKey(id), false) }
            }
        }
        for hit in device { event(hit.seq, hit.kind) }
        return out
    }

    /// The chat store's `searchChat` answer (`{hits, total}`) as the shown
    /// session's hits, in its order: a hit of another session of the tree,
    /// with neither a seq nor a message id, or of a kind the host does not
    /// know is dropped.
    public static func storeHits(_ json: String?, sessionId: String) -> [StoreHit] {
        guard let data = json?.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let hits = object["hits"] as? [[String: Any]] else { return [] }
        return hits.compactMap { hit -> StoreHit? in
            guard hit["sessionId"] as? String == sessionId, let kind = hit["kind"] as? String, knownKinds.contains(kind) else { return nil }
            let seq = (hit["seq"] as? NSNumber)?.intValue
            let messageId = hit["messageId"] as? String
            guard seq != nil || messageId != nil else { return nil }
            return StoreHit(sessionId: sessionId, seq: seq, messageId: seq == nil ? messageId : nil, kind: kind)
        }
    }

    /// The device's `search_result` hits (newest first), kinds it knows.
    public static func deviceHits(_ hits: [(seq: Int, kind: String)]) -> [DeviceHit] {
        hits.compactMap { hit in knownKinds.contains(hit.kind) ? DeviceHit(seq: hit.seq, kind: hit.kind) : nil }
    }

    /// The hit kinds a transcript draws (chat-store.json `ChatDocKind`).
    static let knownKinds: Set<String> = ["user_message", "narration", "tool", "tool_output", "message"]
}

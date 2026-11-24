import Foundation

// #EXP-2067 (packages/domain-contract/fixtures/chat-express.json): Express
// blocks inline in an agent's text ×4. The Swift mirror of the TS reference
// `src/chat-express.ts`; `ChatExpressTests` replays every case group.
//
// The SPLIT and the COMPILE are the eui SDK's (`expressUnwrapJson` + an
// unwrapped `ExpressDecoder` per block, the FFI), injected as an
// `ExpressSdk` so ExpCore stays free of the binding: the app passes the
// FFI's (`ExpressSdks.ffi`), the tests the same when the xcframework is
// built. Everything that reads an agent text WITHOUT drawing it (previews,
// notifications, search, read-aloud, copy) uses the pure `plainProse`.

/// The eui SDK, injected (the TS `ExpressSdk`).
public protocol ExpressSdk {
    /// `unwrap(text)`: the text's prose and blocks in order.
    func unwrap(_ text: String) -> [ChatExpress.RawPart]
    /// An UNWRAPPED Express stream whose default surface id is `surfaceId`.
    func decoder(surfaceId: String) -> ExpressBlockDecoder
}

/// One block's stream (the SDK's `ExpressDecoder`, unwrapped).
public protocol ExpressBlockDecoder: AnyObject {
    func push(_ chunk: String) -> ChatExpress.Decoded
    func end() -> ChatExpress.Decoded
}

/// The SDK over its JSON facade (the FFI's `expressUnwrapJson` and
/// `ExpressDecoder.push/end`, which answer JSON text).
public struct ExpressJSONSdk: ExpressSdk, Sendable {
    public typealias Stream = (push: @Sendable (String) -> String, end: @Sendable () -> String)

    let unwrapJson: @Sendable (String) -> String
    let stream: @Sendable (String) -> Stream

    public init(unwrapJson: @escaping @Sendable (String) -> String, stream: @escaping @Sendable (String) -> Stream) {
        self.unwrapJson = unwrapJson
        self.stream = stream
    }

    public func unwrap(_ text: String) -> [ChatExpress.RawPart] {
        ChatExpress.RawPart.list(json: unwrapJson(text))
    }

    public func decoder(surfaceId: String) -> ExpressBlockDecoder {
        JSONDecoderStream(stream(surfaceId))
    }

    private final class JSONDecoderStream: ExpressBlockDecoder {
        let stream: Stream

        init(_ stream: Stream) {
            self.stream = stream
        }

        func push(_ chunk: String) -> ChatExpress.Decoded { ChatExpress.Decoded(json: stream.push(chunk)) }
        func end() -> ChatExpress.Decoded { ChatExpress.Decoded(json: stream.end()) }
    }
}

public enum ChatExpress {
    // MARK: - The fixture's constants (asserted against it in the tests)

    public enum Limits {
        public static let blocksPerRowMax = 8
        public static let blockMaxBytes = 32768
        public static let blockMaxSurfaces = 4
        public static let blockMaxComponents = 200
        public static let actionContextMaxBytes = 4096
        public static let actionNameMax = 64
        public static let noteLineMax = 120
    }

    public enum Layout {
        /// The gap between a block's surface frames.
        public static let blockGap: Double = 8
    }

    public enum Copy {
        public static let blockFailed = "This view could not be drawn"
        public static let blockOnly = "Sent a view"
        public static let actionMarker = "{author} pressed {name}"
        public static let you = "You"
        public static let actionTooLarge = "That action carries too much data to send"
    }

    /// An event whose text reads as the person's message.
    public static let replyEvent = "reply"
    public static let gate = #"^[ \t]*<a2ui(?:[ \t][^>\n]*)?>"#

    // MARK: - The SDK's shapes

    /// One part of `unwrap` (the FFI's JSON: `{text}` | `{a2ui_raw, is_final}`).
    public enum RawPart: Equatable, Sendable {
        case text(String)
        case block(raw: String, isFinal: Bool)

        static func list(json: String) -> [RawPart] {
            guard let list = (try? JSONSerialization.jsonObject(with: Data(json.utf8))) as? [[String: Any]] else { return [] }
            return list.compactMap { part in
                if let raw = part["a2ui_raw"] as? String {
                    return .block(raw: raw, isFinal: !CardJSON.isFalse(part["is_final"]))
                }
                return (part["text"] as? String).map(RawPart.text)
            }
        }
    }

    /// One decoder issue: its byte offset and the SDK's words.
    public struct Issue: Equatable, Sendable {
        public let at: Int
        public let message: String

        public init(at: Int, message: String) {
            self.at = at
            self.message = message
        }
    }

    /// What a block's decoder returns per push (the SDK's `Decoded`).
    public struct Decoded: Equatable, Sendable {
        public let messages: [CardJSON]
        public let issues: [Issue]

        public init(messages: [CardJSON], issues: [Issue]) {
            self.messages = messages
            self.issues = issues
        }

        init(json: String) {
            let object = (try? JSONSerialization.jsonObject(with: Data(json.utf8))) as? [String: Any] ?? [:]
            messages = ((object["messages"] as? [Any]) ?? []).compactMap(CardJSON.init(any:))
            issues = ((object["issues"] as? [[String: Any]]) ?? []).map {
                Issue(at: ($0["at"] as? NSNumber)?.intValue ?? 0, message: $0["message"] as? String ?? "")
            }
        }
    }

    // MARK: - Rows

    public enum BlockStatus: String, Equatable, Sendable {
        case open, ok, failed, tooMany, tooLarge
    }

    public enum Segment: Equatable, Sendable {
        case prose(String)
        case block(key: String, raw: String, closed: Bool)
    }

    public struct BlockState: Equatable, Sendable {
        public let key: String
        public let raw: String
        public let closed: Bool
        public let status: BlockStatus
        /// The SDK's category (`ParseError` | `ValidationError` |
        /// `CatalogError`) or `Unclosed`; only on `failed`.
        public let error: String?
        /// The SDK's own words (never fixture-locked).
        public let errorMessage: String?
        /// Surface ids the block's stream created and did not delete.
        public let surfaces: [String]
        /// EVERY A2UI message of the block's stream since it (re)started:
        /// what its renderer applies (a restart bumps `generation`).
        public let messages: [CardJSON]
        public let generation: Int

        /// The block draws its marker (`failed`, `tooMany`, `tooLarge`).
        public var marked: Bool { status == .failed || status == .tooMany || status == .tooLarge }
    }

    public enum RowSegment: Equatable, Sendable {
        case prose(String)
        case block(BlockState)
    }

    /// A row's text holds blocks: one of its lines starts with the open tag.
    public static func hasExpressBlock(_ text: String) -> Bool {
        gateRegex.firstMatch(in: text, range: NSRange(text.startIndex..., in: text)) != nil
    }

    private static let tag = "<a2ui>"

    /// While a row streams, a last line that is a proper prefix of `<a2ui>`
    /// is held back (the tag may still be arriving).
    public static func withoutHeldTail(_ text: String, final: Bool) -> String {
        if final { return text }
        let ns = text as NSString
        let nl = ns.range(of: "\n", options: .backwards).location
        let start = nl == NSNotFound ? 0 : nl + 1
        let last = trimStart(ns.substring(from: start))
        if !last.isEmpty, last.utf16.count < tag.utf16.count, tag.hasPrefix(last) {
            guard nl != NSNotFound else { return "" }
            return ns.substring(to: nl)
        }
        return text
    }

    /// The key of a synced row (`m:<messages.id>`).
    public static func syncedRowKey(_ messageId: String) -> String { "m:\(messageId)" }

    /// The key of a feed narration row: its messageId, else its first seq.
    public static func feedRowKey(sessionId: String, messageId: String?, firstSeq: Int) -> String {
        if let messageId, !messageId.isEmpty { return "n:\(sessionId):\(messageId)" }
        return "e:\(sessionId):\(firstSeq)"
    }

    /// `segmentCases` without the compile: nil when the gate fails.
    public static func expressSegments(_ text: String, rowKey: String, sdk: ExpressSdk) -> [Segment]? {
        guard hasExpressBlock(text) else { return nil }
        var n = 0
        var out: [Segment] = []
        for part in sdk.unwrap(text) {
            switch part {
            case let .text(prose):
                if !trim(prose).isEmpty { out.append(.prose(prose)) }
            case let .block(raw, isFinal):
                n += 1
                out.append(.block(key: "\(rowKey)#\(n)", raw: raw, closed: isFinal))
            }
        }
        return out
    }

    private static let issuePrefixes: [(String, String)] = [
        ("parse error: ", "ParseError"),
        ("validation error: ", "ValidationError"),
        ("catalog error: ", "CatalogError"),
    ]

    /// A decoder issue's message (`<category text>: <message>`, the SDK's
    /// form on every platform) → the category and the SDK's words.
    public static func issueCategory(_ message: String) -> (category: String, message: String) {
        for (prefix, category) in issuePrefixes where message.hasPrefix(prefix) {
            return (category, String(message.dropFirst(prefix.count)))
        }
        return ("ParseError", message)
    }

    static func utf8(_ s: String) -> Int { s.utf8.count }

    /// The surfaces a block's messages leave and its component ids per surface.
    static func tally(_ messages: [CardJSON]) -> (surfaces: [String], components: Int) {
        var surfaces: [String] = []
        var comps: [String: Set<String>] = [:]
        for message in messages {
            guard let m = message.objectValue else { continue }
            let create = m["createSurface"]?.objectValue
            let update = m["updateComponents"]?.objectValue
            if let create, case let .string(id)? = create["surfaceId"] {
                surfaces.append(id)
                comps[id] = []
            }
            if let c = create ?? update, case let .string(id)? = c["surfaceId"], case let .array(list)? = c["components"] {
                for k in list {
                    if case let .string(componentId)? = k.objectValue?["id"] { comps[id]?.insert(componentId) }
                }
            }
            if let del = m["deleteSurface"]?.objectValue, case let .string(id)? = del["surfaceId"] {
                if let i = surfaces.firstIndex(of: id) { surfaces.remove(at: i) }
                comps[id] = nil
            }
        }
        return (surfaces, comps.values.reduce(0) { $0 + $1.count })
    }

    /// One row's blocks across its updates (`streamCases`): `update(text,
    /// final)` re-splits the text, pushes only a block's new suffix,
    /// restarts a block whose raw changed otherwise, and ends a block once
    /// it is closed or the row is final. Any chunking reaches the states
    /// `segmentCases` lists for the text so far.
    public final class Row {
        private final class Live {
            var raw: String
            let decoder: ExpressBlockDecoder
            var messages: [CardJSON] = []
            var issue: Issue?
            var ended = false
            let generation: Int

            init(decoder: ExpressBlockDecoder, generation: Int) {
                self.raw = ""
                self.decoder = decoder
                self.generation = generation
            }
        }

        private let rowKey: String
        private let sdk: ExpressSdk
        private var blocks: [String: Live] = [:]

        public init(rowKey: String, sdk: ExpressSdk) {
            self.rowKey = rowKey
            self.sdk = sdk
        }

        public func update(_ text: String, final: Bool) -> [RowSegment]? {
            let shown = withoutHeldTail(text, final: final)
            guard let segments = expressSegments(shown, rowKey: rowKey, sdk: sdk) else {
                blocks.removeAll()
                if shown.utf16.elementsEqual(text.utf16) { return nil }
                // a held tail: the prose before it, the tag still arriving
                let prose = trim(shown)
                return prose.isEmpty ? [] : [.prose(prose)]
            }
            var index = 0
            var seen = Set<String>()
            let out: [RowSegment] = segments.map { segment in
                switch segment {
                case let .prose(text):
                    return .prose(text)
                case let .block(key, raw, closed):
                    seen.insert(key)
                    defer { index += 1 }
                    return .block(block(key: key, raw: raw, closed: closed, index: index, final: final))
                }
            }
            for key in blocks.keys where !seen.contains(key) { blocks[key] = nil }
            return out
        }

        private func block(key: String, raw: String, closed: Bool, index: Int, final: Bool) -> BlockState {
            func state(
                _ status: BlockStatus, error: String? = nil, errorMessage: String? = nil,
                surfaces: [String] = [], messages: [CardJSON] = [], generation: Int = 0
            ) -> BlockState {
                BlockState(
                    key: key, raw: raw, closed: closed, status: status, error: error, errorMessage: errorMessage,
                    surfaces: surfaces, messages: messages, generation: generation
                )
            }
            if index >= Limits.blocksPerRowMax { return state(.tooMany) }
            if utf8(raw) > Limits.blockMaxBytes { return state(.tooLarge) }
            var live = blocks[key]
            if let held = live {
                let grows = raw.utf8.starts(with: held.raw.utf8)
                let same = raw.utf8.elementsEqual(held.raw.utf8)
                if !(grows && !(held.ended && !same)) {
                    live = Live(decoder: sdk.decoder(surfaceId: key), generation: held.generation + 1)
                }
            }
            let current = live ?? Live(decoder: sdk.decoder(surfaceId: key), generation: 0)
            blocks[key] = current
            if !current.ended, raw.utf8.count > current.raw.utf8.count {
                let suffix = String(decoding: Array(raw.utf8.dropFirst(current.raw.utf8.count)), as: UTF8.self)
                take(current, current.decoder.push(suffix))
                current.raw = raw
            }
            if !current.ended, closed || final {
                take(current, current.decoder.end())
                current.ended = true
            }
            let (surfaces, components) = tally(current.messages)
            if surfaces.count > Limits.blockMaxSurfaces || components > Limits.blockMaxComponents {
                return state(.tooLarge, generation: current.generation)
            }
            if let issue = current.issue {
                let (category, message) = issueCategory(issue.message)
                return state(
                    .failed, error: category, errorMessage: message,
                    surfaces: surfaces, messages: current.messages, generation: current.generation
                )
            }
            if !closed {
                return final
                    ? state(
                        .failed, error: "Unclosed", errorMessage: unclosedMessage,
                        surfaces: surfaces, messages: current.messages, generation: current.generation
                    )
                    : state(.open, surfaces: surfaces, messages: current.messages, generation: current.generation)
            }
            return state(.ok, surfaces: surfaces, messages: current.messages, generation: current.generation)
        }

        private func take(_ live: Live, _ decoded: Decoded) {
            if live.issue != nil { return }
            live.messages.append(contentsOf: decoded.messages)
            if let first = decoded.issues.first { live.issue = first }
        }
    }

    static let unclosedMessage = "The block has no </a2ui> line."

    /// `segmentCases` in one go (a fresh row).
    public static func expressRow(_ text: String, rowKey: String, final: Bool, sdk: ExpressSdk) -> [RowSegment]? {
        Row(rowKey: rowKey, sdk: sdk).update(text, final: final)
    }

    /// The drawn prose of a row (`segmentCases.prose`): its prose segments
    /// joined by a blank line; the whole text when it holds no block.
    public static func expressProse(_ segments: [RowSegment]?, text: String) -> String {
        guard let segments else { return text }
        return segments.compactMap { segment -> String? in
            if case let .prose(text) = segment { return text }
            return nil
        }.joined(separator: "\n\n")
    }

    // MARK: - The pure prose

    // swiftlint:disable force_try
    private static let gateRegex = try! NSRegularExpression(pattern: gate, options: [.anchorsMatchLines])
    private static let openLine = try! NSRegularExpression(pattern: "^" + gate.dropFirst())
    private static let fence = try! NSRegularExpression(pattern: #"^[ \t]*(```|~~~)[A-Za-z0-9_-]*[ \t]*$"#)
    private static let closeLine = try! NSRegularExpression(pattern: #"^[ \t]*</a2ui>[ \t]*$"#)
    // swiftlint:enable force_try

    private static func matches(_ regex: NSRegularExpression, _ text: String) -> Bool {
        regex.firstMatch(in: text, range: NSRange(text.startIndex..., in: text)) != nil
    }

    /// The PURE prose (`plainProseCases`) for every read that does not
    /// draw: previews, notifications, search, read-aloud, copy. Lines from
    /// an open tag line through the next `</a2ui>` line (or the end) go,
    /// with a fence line right before or after; the remaining runs,
    /// trimmed, joined by a blank line. No SDK: a list never loads the
    /// compiler.
    public static func plainProse(_ text: String) -> String {
        guard hasExpressBlock(text) else { return text }
        let lines = text.components(separatedBy: "\n")
        var keep: [String?] = []
        var i = 0
        while i < lines.count {
            let line = lines[i]
            guard matches(openLine, line) else {
                keep.append(line)
                i += 1
                continue
            }
            // a fence line right before the block goes with it
            if let last = keep.last, let previous = last, matches(fence, previous) { keep.removeLast() }
            let ns = line as NSString
            let open = openLine.firstMatch(in: line, range: NSRange(location: 0, length: ns.length))
            let rest = open.map { ns.substring(from: $0.range.upperBound) } ?? line
            var j = i
            if !rest.contains("</a2ui>") {
                j = i + 1
                while j < lines.count, !matches(closeLine, lines[j]) { j += 1 }
            }
            if j + 1 < lines.count, matches(fence, lines[j + 1]) { j += 1 }
            keep.append(nil)
            i = j + 1
        }
        var runs: [String] = []
        var run: [String] = []
        for line in keep {
            if let line {
                run.append(line)
            } else {
                runs.append(run.joined(separator: "\n"))
                run = []
            }
        }
        runs.append(run.joined(separator: "\n"))
        return runs.map(trim).filter { !$0.isEmpty }.joined(separator: "\n\n")
    }

    /// The text every non-drawing read takes of a message: an agent's pure
    /// prose; anyone else's text as it is (a person's pasted tag stays text).
    public static func readText(_ text: String, authorKind: String) -> String {
        authorKind == "agent" ? plainProse(text) : text
    }

    // MARK: - Actions

    /// THE `action` message card.
    public struct ActionCard: Equatable, Sendable {
        public let name: String
        public let context: [String: CardJSON]?
        public let surfaceId: String
        public let block: String?
        public let messageId: String?

        public init(name: String, context: [String: CardJSON]?, surfaceId: String, block: String?, messageId: String?) {
            self.name = name
            self.context = context
            self.surfaceId = surfaceId
            self.block = block
            self.messageId = messageId
        }

        /// The card as JSON (`{kind: "action", …}`, nulls kept).
        public var json: CardJSON {
            .object([
                "kind": .string("action"),
                "name": .string(name),
                "context": context.map(CardJSON.object) ?? .null,
                "surfaceId": .string(surfaceId),
                "block": block.map(CardJSON.string) ?? .null,
                "messageId": messageId.map(CardJSON.string) ?? .null,
            ])
        }
    }

    public struct ActionOrigin: Equatable, Sendable {
        public let surfaceId: String
        public let block: String?
        public let messageId: String?

        public init(surfaceId: String, block: String?, messageId: String?) {
            self.surfaceId = surfaceId
            self.block = block
            self.messageId = messageId
        }
    }

    /// An action event as the renderer hands it over.
    public struct ActionEvent: Equatable, Sendable {
        public let name: String?
        public let context: CardJSON?
        public let userMessage: String?

        public init(name: String?, context: CardJSON?, userMessage: String?) {
            self.name = name
            self.context = context
            self.userMessage = userMessage
        }
    }

    public enum ActionOutcome: Equatable, Sendable {
        /// The person message the action posts.
        case post(body: String, card: ActionCard)
        /// Nothing posted: THE failed-action toast (`copy.actionTooLarge`).
        case tooLarge
    }

    /// `actionCases`: the person message an action posts (body + card), or
    /// `tooLarge`; nil = no event name.
    public static func surfaceAction(_ event: ActionEvent, origin: ActionOrigin) -> ActionOutcome? {
        guard let name = event.name, !name.isEmpty else { return nil }
        // An empty object is none: what a renderer hands for an event without one.
        let object = event.context?.objectValue
        let context = (object?.isEmpty ?? true) ? nil : object
        if name.unicodeScalars.count > Limits.actionNameMax { return .tooLarge }
        if let context, utf8(json(.object(context))) > Limits.actionContextMaxBytes { return .tooLarge }
        let user = trim(event.userMessage ?? "")
        let body: String
        if name == replyEvent {
            if !user.isEmpty {
                body = user
            } else if case let .string(text)? = context?["text"] {
                body = trim(text)
            } else {
                body = ""
            }
        } else {
            body = user
        }
        return .post(
            body: body,
            card: ActionCard(name: name, context: context, surfaceId: origin.surfaceId, block: origin.block, messageId: origin.messageId)
        )
    }

    /// An exponential_show surface's origin (`show:<messages.id>`).
    public static func showActionOrigin(_ messageId: String) -> ActionOrigin {
        ActionOrigin(surfaceId: "show:\(messageId)", block: nil, messageId: messageId)
    }

    private static let actionLead = "[action] "

    /// `inputCases`: the ONE input the device hands the agent.
    public static func actionInput(_ card: ActionCard, body: String) -> String {
        let head = "{" + [
            "\"name\":" + json(.string(card.name)),
            "\"surfaceId\":" + json(.string(card.surfaceId)),
            "\"block\":" + json(card.block.map(CardJSON.string) ?? .null),
            "\"messageId\":" + json(card.messageId.map(CardJSON.string) ?? .null),
            "\"context\":" + json(card.context.map(CardJSON.object) ?? .null),
        ].joined(separator: ",") + "}"
        return actionLead + head + (body.isEmpty ? "" : "\n\(body)")
    }

    /// A feed input back to its card and body (nil = not an action input).
    public static func parseActionInput(_ text: String) -> (card: ActionCard, body: String)? {
        guard text.hasPrefix(actionLead) else { return nil }
        let ns = text as NSString
        let nl = ns.range(of: "\n").location
        let head = nl == NSNotFound
            ? ns.substring(from: actionLead.utf16.count)
            : ns.substring(with: NSRange(location: actionLead.utf16.count, length: nl - actionLead.utf16.count))
        guard let object = (try? JSONSerialization.jsonObject(with: Data(head.utf8))) as? [String: Any],
              let name = object["name"] as? String,
              let surfaceId = object["surfaceId"] as? String
        else { return nil }
        let context = (object["context"] as? [String: Any]).flatMap(CardJSON.init(any:))?.objectValue
        return (
            ActionCard(
                name: name, context: context, surfaceId: surfaceId,
                block: object["block"] as? String, messageId: object["messageId"] as? String
            ),
            nl == NSNotFound ? "" : ns.substring(from: nl + 1)
        )
    }

    /// A synced message's `action` card (nil = another kind or none).
    public static func actionCard(_ card: TreeMessageCard?) -> ActionCard? {
        guard let card, card.kind == "action", let name = card.eventName, let surfaceId = card.actionSurfaceId else {
            return nil
        }
        return ActionCard(
            name: name, context: card.actionContext, surfaceId: surfaceId,
            block: card.actionBlock, messageId: card.messageId
        )
    }

    /// The marker's words of an action row without a body.
    public static func actionMarker(name: String, author: String?) -> String {
        replacingFirst(replacingFirst(Copy.actionMarker, "{author}", author ?? Copy.you), "{name}", name)
    }

    private static func replacingFirst(_ text: String, _ target: String, _ value: String) -> String {
        guard let range = text.range(of: target) else { return text }
        return text.replacingCharacters(in: range, with: value)
    }

    // MARK: - JSON as `JSON.stringify` writes it

    /// Compact JSON (object keys sorted; strings escaped like `JSON.stringify`).
    static func json(_ value: CardJSON) -> String {
        switch value {
        case let .string(text):
            var out = "\""
            for scalar in text.unicodeScalars {
                switch scalar {
                case "\"": out += "\\\""
                case "\\": out += "\\\\"
                case "\n": out += "\\n"
                case "\r": out += "\\r"
                case "\t": out += "\\t"
                case "\u{08}": out += "\\b"
                case "\u{0C}": out += "\\f"
                default:
                    if scalar.value < 0x20 {
                        out += String(format: "\\u%04x", scalar.value)
                    } else {
                        out.unicodeScalars.append(scalar)
                    }
                }
            }
            return out + "\""
        case let .array(values):
            return "[" + values.map(json).joined(separator: ",") + "]"
        case let .object(values):
            return "{" + values.keys.sorted().map { json(.string($0)) + ":" + json(values[$0] ?? .null) }.joined(separator: ",") + "}"
        default:
            return value.compactString
        }
    }

    static func trim(_ text: String) -> String {
        text.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private static func trimStart(_ text: String) -> String {
        String(text.drop { $0.isWhitespace || $0.isNewline })
    }
}

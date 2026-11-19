import Foundation

// #EXP-1958 (sealed-content.json `cardStamps`): a tap on a SEALED replies /
// suggestions card is the client's. It opens the card with the tree key and
// sends the picked item's text sealed through the composer's own writer
// (`sessions.send` + `cardItem`); the server only stamps the item. The TS
// reference is `cardItemWire` in packages/domain-contract/src/sealed-content.ts.

public enum CardItemWire {
    public enum Action: String, Sendable, CaseIterable {
        case reply, here, start, startAll, dismiss, dismissReplies, file
    }

    /// `server` = the server's own action (an unsealed card); `sealed` =
    /// `sessions.send` through the sealing writer; `stamp` = the server
    /// action that only stamps the item; `refused` = nothing leaves.
    public enum Wire: String, Sendable {
        case server, sealed, stamp, refused
    }

    static let serverCalls: [Action: String] = [
        .reply: "sessions.sendReply",
        .here: "sessions.startSuggestion",
        .start: "sessions.startSuggestion",
        .startAll: "sessions.startAllSuggestions",
        .dismiss: "sessions.dismissSuggestion",
        .dismissReplies: "sessions.dismissReplies",
        .file: "sessions.fileSuggestion",
    ]

    /// Which wire a tap takes (`tapCases`): decided by the STORED card
    /// (sealed or not) and whether this install opened it.
    public static func wire(sealed: Bool, opened: Bool, action: Action) -> (wire: Wire, call: String?) {
        if !sealed { return (.server, serverCalls[action]) }
        if !opened { return (.refused, nil) }
        switch action {
        case .reply, .here, .start, .startAll: return (.sealed, "sessions.send")
        case .dismiss, .dismissReplies, .file: return (.stamp, serverCalls[action])
        }
    }

    /// The wire for a card by its own seal status (`TreeMessage.cardSealStatus`).
    public static func wire(_ status: SealStatus, action: Action) -> Wire {
        wire(sealed: status != .plain, opened: status == .opened, action: action).wire
    }

    /// An item's request, as the server composes it for an unsealed card
    /// (`suggestionRequest`): the title, a blank line, the text.
    public static func request(title: String, text: String) -> String {
        "\(title)\n\n\(text)"
    }

    /// The issue ids a sealed Start names, in the item's order: its uuids
    /// as they are, an `#IDENT` through `resolved` (UPPERCASED identifier ->
    /// id, the issue store's answer), anything else dropped.
    public static func issueIds(_ ids: [String], resolved: [String: String] = [:]) -> [String]? {
        var out: [String] = []
        for raw in ids {
            guard let id = UUID(uuidString: raw) != nil ? raw : resolved[identifier(raw)] else { continue }
            if !out.contains(id) { out.append(id) }
        }
        return out.isEmpty ? nil : out
    }

    /// An item's issue ref as the store keys it (`#exp-7` -> `EXP-7`).
    public static func identifier(_ raw: String) -> String {
        (raw.hasPrefix("#") ? String(raw.dropFirst()) : raw).uppercased()
    }

    /// The item a tap names on the card this install opened. An item the
    /// card no longer carries FAILS (`CardItemGone`), never a silent success.
    public static func item<Item: Identifiable>(_ id: Item.ID, in items: [Item]?) throws -> Item {
        guard let item = items?.first(where: { $0.id == id }) else { throw CardItemGone() }
        return item
    }

    /// Reply / Do it here: a message into the card's session.
    public static func message(sessionId: String, text: String) -> SessionsSendInput {
        .message(sessionId: sessionId, text: text)
    }

    /// Start: a child of the card's session, named by the item's title.
    public static func start(
        parentId: String,
        title: String,
        text: String,
        issueIds ids: [String],
        resolved: [String: String] = [:],
        deviceId: String? = nil,
        account: String? = nil,
        model: String? = nil
    ) -> SessionsSendInput {
        .child(
            parentId: parentId,
            deviceId: deviceId,
            options: SessionLaunchOptions(account: account, model: model),
            text: request(title: title, text: text),
            name: title.isEmpty ? nil : title,
            issueIds: issueIds(ids, resolved: resolved)
        )
    }
}

/// A tap on an item the opened card no longer carries: nothing was sent.
public struct CardItemGone: LocalizedError, Equatable, Sendable {
    public init() {}
    public var errorDescription: String? { "This item is no longer on the card" }
}

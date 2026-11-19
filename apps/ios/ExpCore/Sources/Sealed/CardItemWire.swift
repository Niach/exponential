import Foundation

// #EXP-1958 (sealed-content.json `cardStamps`): a tap on a SEALED replies /
// suggestions card is the client's. It opens the card with the tree key and
// sends the picked item's text sealed through the composer's own writer
// (`sessions.send` + `cardItem`); the server only stamps the item. The TS
// reference is `cardItemWire` in packages/domain-contract/src/sealed-content.ts.

/// `sessions.send`'s tRPC-only `cardItem`: the sealed card item a send acts on.
public struct CardItemRef: Encodable, Equatable, Sendable {
    public let messageId: String
    public let itemId: String

    public init(messageId: String, itemId: String) {
        self.messageId = messageId
        self.itemId = itemId
    }
}

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

    /// The issue ids a sealed Start names: the item's uuids as they are
    /// (an #IDENT this store does not resolve is dropped).
    public static func issueIds(_ ids: [String]) -> [String]? {
        let uuids = ids.filter { UUID(uuidString: $0) != nil }
        return uuids.isEmpty ? nil : uuids
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
            issueIds: issueIds(ids)
        )
    }
}

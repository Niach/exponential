import Foundation
import UserNotifications

/// #EXP-1641: answering from the notification, a 1:1 port of the TS
/// reference `packages/domain-contract/src/notification-actions.ts` (Android
/// `data/push/NotificationActions.kt`), locked by
/// `fixtures/notification-actions.json` (`NotificationActionsTests`). The
/// fixture's `$comment` holds the rules. In ExpPush so the Notification
/// Service Extension (which links no ExpCore) draws them; the app runs them
/// (`NotificationActionRunner`).
public enum NotificationActions {
    /// Fixture `copy`.
    public enum Copy {
        public static let approve = "Approve"
        public static let requestChanges = "Request changes…"
        public static let requestChangesPlaceholder = "What should change?"
        public static let answer = "Answer…"
        public static let answerPlaceholder = "Answer"
        public static let reply = "Reply"
        public static let replyPlaceholder = "Reply"
        public static let send = "Send"
        public static let open = "Open"
        public static let failed = "Couldn't send. Open Exponential to retry."
    }

    /// Fixture `dataKeys`.
    public enum DataKeys {
        public static let messageId = "messageId"
        public static let cardKind = "cardKind"
        public static let options = "options"
        public static let custom = "custom"
        public static let planQuestions = "planQuestions"
        public static let sealed = "sealed"
    }

    public static let maxOptionButtons = 3

    public static let approve = "approve"
    public static let requestChanges = "request-changes"
    public static let answer = "answer"
    public static let reply = "reply"
    public static let open = "open"
    public static let optionPrefix = "option:"

    /// The option labels the extension drew, by index (`userInfo` key): the
    /// response carries the action id only.
    public static let optionLabelsKey = "expActionOptionLabels"
    /// Category ids this module registers start with this.
    public static let categoryPrefix = "exp-actions-"

    public enum Kind: String, Equatable, Sendable {
        case button, text
    }

    public struct Action: Equatable, Sendable {
        public let id: String
        public let kind: Kind
        public let title: String

        public init(id: String, kind: Kind, title: String) {
            self.id = id
            self.kind = kind
            self.title = title
        }
    }

    public struct AskOptions: Equatable, Sendable {
        public let options: [String]
        public let custom: Bool

        public init(options: [String], custom: Bool) {
            self.options = options
            self.custom = custom
        }
    }

    private static let optionsLead = "Options: "
    private static let planQuestionsMark = "\n\nQuestions:\n- "
    private static let flagsPattern = #"\s\(((?:several allowed|free text allowed)(?:, (?:several allowed|free text allowed))*)\)$"#

    /// A question's options and free-text flag, read from its bus ask text.
    public static func askTextOptions(_ askText: String) -> AskOptions {
        guard let line = askText.components(separatedBy: "\n").dropFirst().first(where: { $0.hasPrefix(optionsLead) }) else {
            return AskOptions(options: [], custom: true)
        }
        var rest = String(line.dropFirst(optionsLead.count))
        var custom = false
        if let regex = try? NSRegularExpression(pattern: flagsPattern),
           let match = regex.firstMatch(in: rest, range: NSRange(rest.startIndex..., in: rest)),
           let whole = Range(match.range, in: rest), let flags = Range(match.range(at: 1), in: rest) {
            custom = rest[flags].contains("free text allowed")
            rest = String(rest[..<whole.lowerBound])
        }
        let options = rest.components(separatedBy: " | ").filter { !$0.isEmpty }
        return AskOptions(options: options, custom: custom)
    }

    private static func parseOptions(_ raw: String?) -> [String] {
        guard let raw, let parsed = try? JSONSerialization.jsonObject(with: Data(raw.utf8)) as? [Any] else { return [] }
        return parsed.compactMap { $0 as? String }
    }

    /// The actions a delivered push offers, in order (empty = none).
    /// `keyHeld` = this install holds the session tree's key;
    /// `openedAskText` = a sealed card's ask text opened here (nil = not).
    public static func notificationActions(data: [String: String], keyHeld: Bool, openedAskText: String?) -> [Action] {
        let sealed = data[DataKeys.sealed] == "1"
        let kind = data[DataKeys.cardKind]
        let openAction = Action(id: open, kind: .button, title: Copy.open)
        if kind == "plan" || kind == "question" {
            guard let messageId = data[DataKeys.messageId], !messageId.isEmpty else { return [openAction] }
            let opened = sealed && keyHeld ? openedAskText : nil
            if sealed && opened == nil { return [openAction] }
            if kind == "plan" {
                let questions = opened.map { $0.contains(planQuestionsMark) }
                    ?? ((Double(data[DataKeys.planQuestions] ?? "0") ?? 0) > 0)
                let changes = Action(id: requestChanges, kind: .text, title: Copy.requestChanges)
                return questions ? [changes, openAction] : [Action(id: approve, kind: .button, title: Copy.approve), changes, openAction]
            }
            let ask = opened.map(askTextOptions)
                ?? AskOptions(options: parseOptions(data[DataKeys.options]), custom: data[DataKeys.custom] != "0")
            let room = ask.custom ? maxOptionButtons - 1 : maxOptionButtons
            let buttons = ask.options.count <= room
                ? ask.options.enumerated().map { Action(id: "\(optionPrefix)\($0.offset)", kind: .button, title: $0.element) }
                : []
            let actions = ask.custom ? buttons + [Action(id: answer, kind: .text, title: Copy.answer)] : buttons
            return actions.isEmpty ? [openAction] : actions
        }
        if let sessionId = data["sessionId"], !sessionId.isEmpty {
            return sealed && !keyHeld ? [] : [Action(id: reply, kind: .text, title: Copy.reply)]
        }
        return []
    }

    /// The string values of a push's `userInfo` (the fixture's `data`).
    public static func data(_ userInfo: [AnyHashable: Any]) -> [String: String] {
        var out: [String: String] = [:]
        for (key, value) in userInfo {
            if let key = key as? String, let value = value as? String { out[key] = value }
        }
        return out
    }

    // MARK: running

    /// How a tapped action runs (never in clear for a `sealed=1` push).
    public enum Route: Equatable, Sendable {
        /// The composer's send.
        case send
        /// Sealed for the named tree, the session row not needed.
        case sendSealed(rootId: String)
        /// THE answer path, re-sealing a sealed card.
        case answer
        /// The card first (fetched), then `answer`.
        case fetchThenAnswer
        /// Nothing is sent: the failed notification.
        case refuse
    }

    /// `actionId` of a push (`sealed` = `dataKeys.sealed` is `1`, `rootId`
    /// its tree, `cardLocal` = the card's message row is in the local store).
    public static func route(actionId: String, sealed: Bool, rootId: String?, cardLocal: Bool) -> Route {
        if actionId == reply {
            if !sealed { return .send }
            guard let rootId, !rootId.isEmpty else { return .refuse }
            return .sendSealed(rootId: rootId)
        }
        if actionId == approve || actionId == requestChanges || actionId == answer || actionId.hasPrefix(optionPrefix) {
            return sealed && !cardLocal ? .fetchThenAnswer : .answer
        }
        return .refuse
    }

    // MARK: the OS category

    /// One category per distinct action set (option buttons wear their
    /// labels, so the id hashes ids and titles).
    public static func categoryIdentifier(_ actions: [Action]) -> String {
        var hash: UInt64 = 0xcbf2_9ce4_8422_2325
        for byte in actions.map({ "\($0.id)\u{1}\($0.kind.rawValue)\u{1}\($0.title)" }).joined(separator: "\u{2}").utf8 {
            hash ^= UInt64(byte)
            hash = hash &* 0x0000_0100_0000_01b3
        }
        return categoryPrefix + String(hash, radix: 16)
    }

    /// The OS-drawn category: every mutating action behind the device unlock
    /// and never foreground; `open` = foreground (like a tap).
    public static func category(_ actions: [Action]) -> UNNotificationCategory {
        let drawn: [UNNotificationAction] = actions.map { action in
            if action.id == open {
                return UNNotificationAction(identifier: action.id, title: action.title, options: [.foreground])
            }
            switch action.kind {
            case .button:
                return UNNotificationAction(identifier: action.id, title: action.title, options: [.authenticationRequired])
            case .text:
                return UNTextInputNotificationAction(
                    identifier: action.id,
                    title: action.title,
                    options: [.authenticationRequired],
                    textInputButtonTitle: Copy.send,
                    textInputPlaceholder: placeholder(action.id)
                )
            }
        }
        return UNNotificationCategory(identifier: categoryIdentifier(actions), actions: drawn, intentIdentifiers: [], options: [])
    }

    static func placeholder(_ id: String) -> String {
        switch id {
        case requestChanges: Copy.requestChangesPlaceholder
        case answer: Copy.answerPlaceholder
        default: Copy.replyPlaceholder
        }
    }

    /// The registered set plus `category` (ours capped at `keep`: the OS
    /// hands the set back unordered, so an older one past the cap goes;
    /// another module's categories stay).
    public static func merged(_ existing: [UNNotificationCategory], adding category: UNNotificationCategory, keep: Int = 64) -> Set<UNNotificationCategory> {
        guard !existing.contains(where: { $0.identifier == category.identifier }) else { return Set(existing) }
        let ours = existing.filter { $0.identifier.hasPrefix(categoryPrefix) }
        let others = existing.filter { !$0.identifier.hasPrefix(categoryPrefix) }
        return Set(others + ours.suffix(max(0, keep - 1)) + [category])
    }

    /// The completion form of `register` (the extension's callbacks).
    public static func register(
        _ actions: [Action],
        center: UNUserNotificationCenter = .current(),
        completion: @escaping @Sendable (String) -> Void
    ) {
        let category = category(actions)
        nonisolated(unsafe) let adding = category
        center.getNotificationCategories { existing in
            center.setNotificationCategories(merged(Array(existing), adding: adding))
            completion(adding.identifier)
        }
    }

    /// Registers the category of `actions` (async: the extension waits
    /// before delivering, so the OS knows it when it draws the push).
    public static func register(_ actions: [Action], center: UNUserNotificationCenter = .current()) async -> String {
        let category = category(actions)
        let existing = await center.notificationCategories()
        center.setNotificationCategories(merged(Array(existing), adding: category))
        return category.identifier
    }
}

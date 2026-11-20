import Foundation

/// #EDE-99 (search-list.json `chats`, its `$comment` CHATS = the spec ×4): the
/// global search's CHATS section over the chat store's `searchAll`. Pure:
/// which sections show, the chat rows, what a pick opens and where find in
/// chat starts. Mirrors packages/domain-contract/src/search-list.ts, desktop
/// `domain::search_list` and Android `domain/SearchList.kt`.
extension SearchList {
    public enum GlobalChats {
        public static let debounceMs = 150
        public static let chatsMax = 20
        /// The phone's chat row (`row.chatHeightPhone`).
        public static let chatHeightPhone: Double = 64
        public static let chatHeight: Double = 52

        public enum Icons {
            public static let chat = "search-chat"
            public static let agent = "search-agent"
        }

        public enum Copy {
            public static let issues = "Issues"
            public static let chats = "Chats"
            public static let you = "You: "
            public static let noResults = "No results"
            public static let placeholder = "Search issues and chats"
        }

        public enum TestIds {
            public static let issues = "search-list-issues"
            public static let chats = "search-list-chats"
            public static let chatRow = "search-list-chat-row"
        }
    }

    /// One hit of the chat store (chat-store.json `ChatHit`).
    public struct ChatHit: Equatable, Hashable, Sendable {
        public var rootId: String
        public var sessionId: String
        public var deviceId: String?
        public var kind: String
        /// Epoch ms.
        public var at: Double
        public var seq: Int?
        public var messageId: String?
        public var snippet: String
        public var matchStart: Int
        public var matchLen: Int

        public init(
            rootId: String, sessionId: String, deviceId: String? = nil, kind: String, at: Double,
            seq: Int? = nil, messageId: String? = nil, snippet: String, matchStart: Int = 0, matchLen: Int = 0
        ) {
            self.rootId = rootId
            self.sessionId = sessionId
            self.deviceId = deviceId
            self.kind = kind
            self.at = at
            self.seq = seq
            self.messageId = messageId
            self.snippet = snippet
            self.matchStart = matchStart
            self.matchLen = matchLen
        }

        public init?(_ object: [String: Any]) {
            guard let rootId = object["rootId"] as? String, let sessionId = object["sessionId"] as? String,
                  let kind = object["kind"] as? String, let at = (object["at"] as? NSNumber)?.doubleValue else { return nil }
            self.init(
                rootId: rootId, sessionId: sessionId, deviceId: object["deviceId"] as? String, kind: kind, at: at,
                seq: (object["seq"] as? NSNumber)?.intValue, messageId: object["messageId"] as? String,
                snippet: object["snippet"] as? String ?? "",
                matchStart: (object["matchStart"] as? NSNumber)?.intValue ?? 0,
                matchLen: (object["matchLen"] as? NSNumber)?.intValue ?? 0
            )
        }
    }

    /// One chat of `searchAll`: its newest hit and how many.
    public struct ChatHitChat: Equatable, Sendable {
        public var rootId: String
        public var count: Int
        public var hit: ChatHit

        public init(rootId: String, count: Int, hit: ChatHit) {
            self.rootId = rootId
            self.count = count
            self.hit = hit
        }
    }

    /// `searchAll`'s `{chats, total}` text (nil or unreadable = none).
    public static func chatHits(_ json: String?) -> [ChatHitChat] {
        guard let data = json?.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let chats = object["chats"] as? [[String: Any]] else { return [] }
        return chats.compactMap { chat in
            guard let rootId = chat["rootId"] as? String, let hit = (chat["hit"] as? [String: Any]).flatMap(ChatHit.init) else { return nil }
            return ChatHitChat(rootId: rootId, count: (chat["count"] as? NSNumber)?.intValue ?? 1, hit: hit)
        }
    }

    // MARK: - Sections (`sectionCases`)

    public enum GlobalSectionKey: String, Sendable {
        case issues, chats
    }

    public struct GlobalSection: Equatable, Sendable {
        public var key: GlobalSectionKey
        public var count: Int
        public var loading: Bool

        public init(key: GlobalSectionKey, count: Int, loading: Bool) {
            self.key = key
            self.count = count
            self.loading = loading
        }
    }

    public struct GlobalSections: Equatable, Sendable {
        /// The sections wear headers.
        public var titled: Bool
        public var sections: [GlobalSection]
        /// The input's trailing count (nil = none).
        public var count: Int?
        /// The empty state shows.
        public var empty: Bool
    }

    /// `chats` nil = no chat store on this client.
    public static func globalSections(query: String, issues: Int, chats: (rows: Int, loading: Bool)?) -> GlobalSections {
        let blank = FindInChat.query(query).isEmpty
        guard !blank, let chats else {
            return GlobalSections(
                titled: false, sections: [GlobalSection(key: .issues, count: issues, loading: false)],
                count: blank ? nil : issues, empty: false
            )
        }
        var sections: [GlobalSection] = []
        if issues > 0 { sections.append(GlobalSection(key: .issues, count: issues, loading: false)) }
        if chats.rows > 0 { sections.append(GlobalSection(key: .chats, count: chats.rows, loading: chats.loading)) }
        let count = issues + chats.rows
        return GlobalSections(titled: true, sections: sections, count: count, empty: count == 0 && !chats.loading)
    }

    // MARK: - Rows (`rowCases`)

    /// One of the active team's synced roots I can see.
    public struct GlobalChatRoot: Equatable, Sendable {
        public var title: String
        public var agent: Bool

        public init(title: String, agent: Bool) {
            self.title = title
            self.agent = agent
        }
    }

    public struct GlobalChatRow: Equatable, Identifiable, Sendable {
        public var key: String
        public var rootId: String
        public var icon: String
        public var title: String
        public var subtitle: String
        public var highlight: String
        public var trailing: String
        /// What a pick opens.
        public var hit: ChatHit

        public var id: String { key }
    }

    /// The Chats rows: the store's order, roots of the active team only, at
    /// most `limit`. `messageAuthors`: synced messages' author kinds by id.
    public static func globalChatRows(
        query: String,
        now: Date,
        utcOffsetMinutes: Int,
        chats: [ChatHitChat],
        roots: [String: GlobalChatRoot],
        messageAuthors: [String: String],
        limit: Int
    ) -> [GlobalChatRow] {
        let highlight = FindInChat.query(query)
        let nowMs = now.timeIntervalSince1970 * 1000
        var rows: [GlobalChatRow] = []
        for chat in chats {
            if rows.count >= limit { break }
            guard let root = roots[chat.rootId] else { continue }
            let hit = chat.hit
            let person = hit.kind == "user_message"
                || (hit.kind == "message" && hit.messageId.flatMap { messageAuthors[$0] } == "user")
            rows.append(GlobalChatRow(
                key: "chat:\(chat.rootId)",
                rootId: chat.rootId,
                icon: root.agent ? GlobalChats.Icons.agent : GlobalChats.Icons.chat,
                title: root.title,
                subtitle: (person ? GlobalChats.Copy.you : "") + hit.snippet,
                highlight: highlight,
                trailing: Chats.chatTimeLabel(atMs: hit.at, nowMs: nowMs, utcOffsetMinutes: utcOffsetMinutes),
                hit: hit
            ))
        }
        return rows
    }

    // MARK: - A pick (`pickCases`, `startCases`)

    /// Find in chat's start target: a stop's key and whether it is an output stop.
    public struct GlobalChatTarget: Equatable, Hashable, Sendable {
        public var key: String
        public var output: Bool

        public init(key: String, output: Bool) {
            self.key = key
            self.output = output
        }
    }

    public struct GlobalChatPick: Equatable, Hashable, Sendable {
        /// The session to open: the root, or the child it was said in.
        public var sessionId: String
        public var query: String
        public var target: GlobalChatTarget
    }

    public static func globalChatPick(query: String, hit: ChatHit) -> GlobalChatPick {
        let key = hit.kind == "message"
            ? FindInChat.messageKey(hit.messageId ?? "")
            : FindInChat.eventKey(sessionId: hit.sessionId, seq: hit.seq ?? 0)
        return GlobalChatPick(
            sessionId: hit.sessionId, query: FindInChat.query(query),
            target: GlobalChatTarget(key: key, output: hit.kind == "tool_output")
        )
    }

    /// Find in chat's start for a pick: the target's first stop, else the
    /// newest; nil while no stops.
    public static func globalChatStart(_ stops: [FindInChat.Stop], target: GlobalChatTarget) -> Int? {
        guard !stops.isEmpty else { return nil }
        return stops.firstIndex { $0.key == target.key && $0.output == target.output } ?? stops.count - 1
    }
}

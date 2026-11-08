import Foundation

/// #FEED-191 THE RESULTS TAB ×4 (`packages/domain-contract/fixtures/results-tab.json`,
/// TS reference `src/results-tab.ts`, desktop `domain::results_tab`, Android
/// `domain/ResultsTab.kt`): a chat tree's results, the session screen's last
/// tab. `ResultsTabTests` replays every fixture case.
public enum ResultsTab {
    // MARK: - Fixture constants

    public enum Copy {
        public static let now = "now"
        public static let untitled = "Result"
    }

    /// The session screen's Results tab (`tab`).
    public enum Tab {
        public enum Copy {
            public static let label = "Results"
            public static let empty = "No results yet"
            public static let changes = "Changes"
            public static let stack = "Stack"
        }

        public enum TestIds {
            public static let tab = "chat-results-tab"
            public static let list = "chat-results"
            public static let section = "result-section"
            public static let changes = "chat-results-changes"
            public static let stack = "chat-results-stack"
        }

        /// A section's geometry; iOS = the `phone` form factor.
        public enum Tile {
            public static let thumbHeight: Double = 240
            public static let thumbMaxWidth: Double = 320
            public static let gap: Double = 8
            public static let textGap: Double = 4
            public static let radius: Double = 10
            public static let listGap: Double = 32
            public static let leadGap: Double = 24
        }

        /// The styleguide specimen draws `cases[specimenCase]`.
        public static let specimenCase = 1
    }

    // MARK: - Input

    public struct SessionInput: Equatable, Sendable {
        public let id: String
        public let rootId: String
        /// The session's display name (session-title).
        public let name: String

        public init(id: String, rootId: String, name: String) {
            self.id = id
            self.rootId = rootId
            self.name = name
        }
    }

    public struct MessageInput: Equatable, Sendable {
        public let id: String
        public let sessionId: String
        public let authorSessionId: String?
        /// Epoch ms.
        public let createdAt: Double
        public let card: TreeMessageCard?

        public init(id: String, sessionId: String, authorSessionId: String?, createdAt: Double, card: TreeMessageCard?) {
            self.id = id
            self.sessionId = sessionId
            self.authorSessionId = authorSessionId
            self.createdAt = createdAt
            self.card = card
        }
    }

    // MARK: - Output

    public struct Picture: Equatable, Sendable {
        public let label: String
        public let attachmentId: String
        public let width: Double?
        public let height: Double?
        public let caption: String?

        public init(label: String, attachmentId: String, width: Double?, height: Double?, caption: String?) {
            self.label = label
            self.attachmentId = attachmentId
            self.width = width
            self.height = height
            self.caption = caption
        }
    }

    public struct Result: Equatable, Sendable, Identifiable {
        public let id: String
        public let sessionId: String
        public let sessionName: String
        public let createdAt: Double
        /// `<m>m` / `<h>h` / `<d>d`, `now` under a minute.
        public let age: String
        public let title: String
        public let description: String?
        public let pictures: [Picture]
        public let surface: Bool

        public init(
            id: String, sessionId: String, sessionName: String, createdAt: Double, age: String, title: String,
            description: String?, pictures: [Picture], surface: Bool
        ) {
            self.id = id
            self.sessionId = sessionId
            self.sessionName = sessionName
            self.createdAt = createdAt
            self.age = age
            self.title = title
            self.description = description
            self.pictures = pictures
            self.surface = surface
        }
    }

    // MARK: - The fold

    private static func resolvedPictures(_ card: TreeMessageCard) -> [Picture] {
        card.pictures.compactMap { picture in
            guard let attachmentId = picture.attachmentId, !attachmentId.isEmpty else { return nil }
            return Picture(
                label: picture.label, attachmentId: attachmentId, width: picture.width, height: picture.height,
                caption: picture.caption
            )
        }
    }

    /// A surface card that goes on the Results tab: `result`, else a card from
    /// before the rename's `story`, else any resolved picture. Plans and
    /// questions never do.
    public static func isResultCard(_ card: TreeMessageCard?) -> Bool {
        guard let card, card.kind == "surface", (card.questionId ?? "").isEmpty else { return false }
        return card.result ?? card.story ?? !resolvedPictures(card).isEmpty
    }

    /// `<m>m` / `<h>h` / `<d>d`, floored; under a minute = `Copy.now`.
    public static func age(_ createdAt: Double, now: Double) -> String {
        let minutes = Int((max(0, now - createdAt) / 60_000).rounded(.down))
        if minutes < 1 { return Copy.now }
        if minutes < 60 { return "\(minutes)m" }
        let hours = minutes / 60
        if hours < 24 { return "\(hours)h" }
        return "\(hours / 24)d"
    }

    private static func blank(_ value: String?) -> String? {
        guard let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines), !trimmed.isEmpty else {
            return nil
        }
        return trimmed
    }

    /// THE fold: every result the root's tree posted, newest first.
    public static func results(
        now: Double, rootId: String, sessions: [SessionInput], messages: [MessageInput]
    ) -> [Result] {
        let sessionsById = Dictionary(sessions.map { ($0.id, $0) }, uniquingKeysWith: { _, last in last })
        var out: [Result] = []
        for message in messages {
            guard let author = message.authorSessionId, author == message.sessionId,
                  isResultCard(message.card), let card = message.card,
                  let session = sessionsById[message.sessionId], session.rootId == rootId
            else { continue }
            out.append(
                Result(
                    id: message.id,
                    sessionId: session.id,
                    sessionName: session.name,
                    createdAt: message.createdAt,
                    age: age(message.createdAt, now: now),
                    title: blank(card.title) ?? Copy.untitled,
                    description: blank(card.description),
                    pictures: resolvedPictures(card),
                    surface: card.surface != nil
                )
            )
        }
        return out.sorted { a, b in a.createdAt != b.createdAt ? a.createdAt > b.createdAt : a.id > b.id }
    }
}

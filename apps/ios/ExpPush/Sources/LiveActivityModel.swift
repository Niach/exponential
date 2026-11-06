import Foundation

// #EXP-1639 (packages/domain-contract/fixtures/live-activity.json, its
// `$comment` is the spec; TS reference src/live-activity.ts, Android
// `data/push/LiveActivity.kt`): a running session on the Lock Screen and in
// the Dynamic Island. The server decides and SEALS the content; the widget
// extension opens it (`LiveActivityDisplay.open`) and only draws it.
// `follows`/`content` are mirrored to lock the rules (`LiveActivityModelTests`
// replays the `*Cases`), never run on the phone. Lives in ExpPush because the
// widget extension links ExpPush only (no GRDB, no ExpCore).
public enum LiveActivityModel {
    /// The sealed payload's `data.type`.
    public static let pushType = "session_activity"

    public static let encryptedTitle = "Encrypted session"
    public static let prBadge = "PR #{n}"
    public static let badgeNeedsYou = "needs-you"
    public static let badgeTimer = "timer"

    /// Phase → its label, in the fixture's order.
    public static let phases: [(key: String, label: String)] = [
        ("starting", "Starting"),
        ("working", "Working"),
        ("needsYou", "Needs you"),
        ("blocked", "Blocked"),
        ("done", "Done"),
    ]
    public static let segments = ["Working", "Needs you", "PR opened", "Done"]

    public static let maxActivities = 3
    public static let dismissMinutes = 15
    public static let minUpdateSeconds = 30

    public static func label(phase: String) -> String? {
        phases.first { $0.key == phase }?.label
    }

    public struct Session: Equatable, Sendable {
        public var id: String
        public var parentId: String?
        public var userId: String
        public var agentId: String?
        public var startedByAgentId: String?
        /// The display title; ignored when `sealedTitle`.
        public var title: String?
        public var sealedTitle: Bool
        public var status: String
        public var busy: Bool
        public var needsInput: Bool
        public var blocked: Bool
        public var startedAt: String?

        public init(
            id: String, parentId: String?, userId: String, agentId: String?, startedByAgentId: String?,
            title: String?, sealedTitle: Bool, status: String, busy: Bool, needsInput: Bool, blocked: Bool,
            startedAt: String?
        ) {
            self.id = id
            self.parentId = parentId
            self.userId = userId
            self.agentId = agentId
            self.startedByAgentId = startedByAgentId
            self.title = title
            self.sealedTitle = sealedTitle
            self.status = status
            self.busy = busy
            self.needsInput = needsInput
            self.blocked = blocked
            self.startedAt = startedAt
        }
    }

    public struct Content: Equatable, Sendable {
        public var phase: String
        public var title: String
        public var subtitle: String
        public var detail: String?
        /// `needs-you`, `PR #<n>` or `timer`.
        public var badge: String
        public var segment: Int

        public init(phase: String, title: String, subtitle: String, detail: String?, badge: String, segment: Int) {
            self.phase = phase
            self.title = title
            self.subtitle = subtitle
            self.detail = detail
            self.badge = badge
            self.segment = segment
        }
    }

    public static func phase(status: String, needsInput: Bool, blocked: Bool) -> String {
        if needsInput { return "needsYou" }
        if status == "idle" || status == "ended" { return "done" }
        if blocked { return "blocked" }
        return status == "queued" ? "starting" : "working"
    }

    public static func phase(_ session: Session) -> String {
        phase(status: session.status, needsInput: session.needsInput, blocked: session.blocked)
    }

    /// The sessions a member's phone follows, newest started first, capped.
    public static func follows(me: String, sessions: [Session]) -> [String] {
        let followed = sessions.filter {
            $0.parentId == nil && $0.userId == me && $0.startedByAgentId == nil && $0.agentId == nil
                && phase($0) != "done"
        }
        // A stable sort, like the reference's Array.prototype.sort.
        let sorted = followed.enumerated().sorted { a, b in
            let lhs = a.element.startedAt ?? "", rhs = b.element.startedAt ?? ""
            return lhs != rhs ? lhs > rhs : a.offset < b.offset
        }
        return sorted.prefix(maxActivities).map(\.element.id)
    }

    /// What the activity shows.
    public static func content(
        session: Session, deviceLabel: String?, prNumber: Int?, askPreview: String?, blockedLabel: String?
    ) -> Content {
        let phase = phase(session)
        let label = label(phase: phase) ?? ""
        let pr = prNumber.map { prBadge.replacingOccurrences(of: "{n}", with: String($0)) }
        let title = session.sealedTitle || (session.title ?? "").isEmpty ? encryptedTitle : session.title!
        let detail: String? = switch phase {
        case "needsYou": askPreview
        case "blocked": blockedLabel
        default: nil
        }
        let segment = phase == "done" ? 4 : phase == "needsYou" ? 2 : pr != nil ? 3 : 1
        return Content(
            phase: phase,
            title: title,
            subtitle: (deviceLabel ?? "").isEmpty ? label : "\(label) · \(deviceLabel!)",
            detail: detail,
            badge: phase == "needsYou" ? badgeNeedsYou : (pr ?? badgeTimer),
            segment: segment
        )
    }
}

/// What one Live Activity draws: the opened sealed content
/// (`activityPayload` on the server: `{title, body, data}`).
public struct LiveActivityDisplay: Equatable, Sendable {
    public enum Badge: Equatable, Sendable {
        /// Counts up from `startedAt` (the OS draws it).
        case timer
        case needsYou
        /// `PR #<n>`, or the phase label of an ended timer.
        case text(String)
    }

    public var sessionId: String?
    public var userId: String?
    public var phase: String
    public var title: String
    public var subtitle: String
    public var detail: String?
    public var badge: Badge
    public var startedAt: Date?

    public init(
        sessionId: String?, userId: String?, phase: String, title: String, subtitle: String,
        detail: String?, badge: Badge, startedAt: Date?
    ) {
        self.sessionId = sessionId
        self.userId = userId
        self.phase = phase
        self.title = title
        self.subtitle = subtitle
        self.detail = detail
        self.badge = badge
        self.startedAt = startedAt
    }

    /// The content when `s` does not open here (no key yet, a foreign
    /// install): the generic title, nothing else. Never fetched.
    public static let sealed = LiveActivityDisplay(
        sessionId: nil, userId: nil, phase: "working", title: LiveActivityModel.encryptedTitle,
        subtitle: "", detail: nil, badge: .text(""), startedAt: nil
    )

    public var isDone: Bool { phase == "done" }
    public var needsYou: Bool { badge == .needsYou }

    /// The badge's text when it is not a running timer (an ended timer shows
    /// the phase label: nothing counts any more).
    public var badgeText: String? {
        switch badge {
        case .timer: return startedAt == nil || isDone ? LiveActivityModel.label(phase: phase) : nil
        case .needsYou: return LiveActivityModel.label(phase: "needsYou")
        case .text(let text): return text.isEmpty ? nil : text
        }
    }

    /// The tap: the session under the account it was pushed to
    /// (`exponential://session/<id>?userId=<id>`, inbox-route.json).
    public var deepLink: URL? {
        guard let sessionId, !sessionId.isEmpty else { return nil }
        var components = URLComponents()
        components.scheme = "exponential"
        components.host = "session"
        components.path = "/\(sessionId)"
        if let userId, !userId.isEmpty { components.queryItems = [URLQueryItem(name: "userId", value: userId)] }
        return components.url
    }

    /// The opened content → the display; nil when it is not an activity.
    public static func from(title: String, body: String?, data: [String: String]) -> LiveActivityDisplay? {
        guard data["type"] == LiveActivityModel.pushType else { return nil }
        let raw = data["badge"] ?? LiveActivityModel.badgeTimer
        let badge: Badge = switch raw {
        case LiveActivityModel.badgeTimer: .timer
        case LiveActivityModel.badgeNeedsYou: .needsYou
        default: .text(raw)
        }
        return LiveActivityDisplay(
            sessionId: data["sessionId"].flatMap { $0.isEmpty ? nil : $0 },
            userId: data["userId"].flatMap { $0.isEmpty ? nil : $0 },
            phase: data["phase"] ?? "working",
            title: title.isEmpty ? LiveActivityModel.encryptedTitle : title,
            subtitle: body ?? "",
            detail: data["detail"].flatMap { $0.isEmpty ? nil : $0 },
            badge: badge,
            startedAt: data["startedAt"].flatMap(parseDate)
        )
    }

    /// From `SealedPush.map`'s opened content.
    public static func from(opened: SealedPush.Opened) -> LiveActivityDisplay? {
        var data: [String: String] = [:]
        for (key, value) in opened.userInfo {
            if let key = key as? String, let value = value as? String { data[key] = value }
        }
        return from(title: opened.title, body: opened.body, data: data)
    }

    /// Opens a content-state's sealed `s` with every key a push may be sealed
    /// to (current first), else `sealed`.
    public static func open(sealed: String, keys: [PushKeypair]) -> LiveActivityDisplay {
        keys.lazy
            .compactMap { SealedPush.open(userInfo: [SealedPush.sealedKey: sealed], secret: $0.secret) }
            .compactMap { from(opened: $0) }
            .first ?? .sealed
    }

    static func parseDate(_ raw: String) -> Date? {
        let fractional = ISO8601DateFormatter()
        fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return fractional.date(from: raw) ?? ISO8601DateFormatter().date(from: raw)
    }
}

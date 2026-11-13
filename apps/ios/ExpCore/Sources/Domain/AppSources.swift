import Foundation

/// VAPP-119: the `exp:` binding SOURCES beyond devices and records ×4
/// (`packages/domain-contract/fixtures/app-sources.json` is the spec, TS
/// reference `src/app-sources.ts`; desktop `domain::app_sources`, Android
/// `AppSources.kt`): what `exp:issues`, `exp:boards`, `exp:members`,
/// `exp:sessions`, `exp:comments` and `exp:attachments` emit over the synced
/// rows of one team. `AppSourcesTests` replays the fixture's `cases`. Values
/// are `CardJSON` (keys sorted when encoded).
public enum AppSources {
    // MARK: - Fixture constants

    /// The source names this module derives (`scheme:name` without `exp:`).
    public static let kinds = ["issues", "boards", "members", "me", "sessions", "comments", "attachments"]
    public static let limitDefault = 50
    public static let limitMax = 500

    // MARK: - The store (one team's synced rows, ISO timestamps)

    public struct Board: Sendable {
        public let id: String
        public let slug: String
        public let name: String
        public let prefix: String
        public let icon: String?
        public let color: String?
        public let sortOrder: Double
        public let deletedAt: String?

        public init(id: String, slug: String, name: String, prefix: String, icon: String?, color: String?, sortOrder: Double, deletedAt: String?) {
            self.id = id
            self.slug = slug
            self.name = name
            self.prefix = prefix
            self.icon = icon
            self.color = color
            self.sortOrder = sortOrder
            self.deletedAt = deletedAt
        }
    }

    public struct Issue: Sendable {
        public let id: String
        public let boardId: String
        public let identifier: String
        public let title: String
        public let status: String
        public let priority: String?
        public let assigneeId: String?
        public let creatorId: String?
        public let dueDate: String?
        public let sortOrder: Double
        public let createdAt: String
        public let updatedAt: String

        public init(
            id: String, boardId: String, identifier: String, title: String, status: String, priority: String?,
            assigneeId: String?, creatorId: String?, dueDate: String?, sortOrder: Double, createdAt: String, updatedAt: String
        ) {
            self.id = id
            self.boardId = boardId
            self.identifier = identifier
            self.title = title
            self.status = status
            self.priority = priority
            self.assigneeId = assigneeId
            self.creatorId = creatorId
            self.dueDate = dueDate
            self.sortOrder = sortOrder
            self.createdAt = createdAt
            self.updatedAt = updatedAt
        }
    }

    public struct Member: Sendable {
        public let userId: String
        public let role: String

        public init(userId: String, role: String) {
            self.userId = userId
            self.role = role
        }
    }

    public struct User: Sendable {
        public let id: String
        public let name: String
        public let email: String?
        public let image: String?

        public init(id: String, name: String, email: String?, image: String?) {
            self.id = id
            self.name = name
            self.email = email
            self.image = image
        }
    }

    public struct Session: Sendable {
        public let id: String
        public let parentId: String?
        public let agentId: String?
        public let appId: String?
        public let userId: String?
        public let status: String
        public let busy: Bool
        public let needsInput: Bool
        public let title: String?
        public let createdAt: String
        public let updatedAt: String

        public init(
            id: String, parentId: String?, agentId: String?, appId: String?, userId: String?, status: String,
            busy: Bool, needsInput: Bool, title: String?, createdAt: String, updatedAt: String
        ) {
            self.id = id
            self.parentId = parentId
            self.agentId = agentId
            self.appId = appId
            self.userId = userId
            self.status = status
            self.busy = busy
            self.needsInput = needsInput
            self.title = title
            self.createdAt = createdAt
            self.updatedAt = updatedAt
        }
    }

    public struct Comment: Sendable {
        public let id: String
        public let issueId: String
        public let authorId: String?
        public let parentId: String?
        public let source: String
        public let body: String
        public let createdAt: String

        public init(id: String, issueId: String, authorId: String?, parentId: String?, source: String, body: String, createdAt: String) {
            self.id = id
            self.issueId = issueId
            self.authorId = authorId
            self.parentId = parentId
            self.source = source
            self.body = body
            self.createdAt = createdAt
        }
    }

    public struct Attachment: Sendable {
        public let id: String
        public let issueId: String?
        public let commentId: String?
        public let uploaderId: String?
        public let filename: String
        public let contentType: String
        public let sizeBytes: Double
        public let width: Double?
        public let height: Double?
        public let createdAt: String

        public init(
            id: String, issueId: String?, commentId: String?, uploaderId: String?, filename: String,
            contentType: String, sizeBytes: Double, width: Double?, height: Double?, createdAt: String
        ) {
            self.id = id
            self.issueId = issueId
            self.commentId = commentId
            self.uploaderId = uploaderId
            self.filename = filename
            self.contentType = contentType
            self.sizeBytes = sizeBytes
            self.width = width
            self.height = height
            self.createdAt = createdAt
        }
    }

    /// FEED-243: an issue's timeline row (`issue_events`), only who and when.
    public struct IssueEvent: Sendable {
        public let id: String
        public let issueId: String
        public let actorUserId: String?
        public let createdAt: String

        public init(id: String, issueId: String, actorUserId: String?, createdAt: String) {
            self.id = id
            self.issueId = issueId
            self.actorUserId = actorUserId
            self.createdAt = createdAt
        }
    }

    /// FEED-243: a session linked to an issue (`session_issues`).
    public struct SessionIssue: Sendable {
        public let sessionId: String
        public let issueId: String

        public init(sessionId: String, issueId: String) {
            self.sessionId = sessionId
            self.issueId = issueId
        }
    }

    /// The synced rows of ONE team, as the host reads them.
    public struct Store: Sendable {
        public var boards: [Board]
        public var issues: [Issue]
        public var members: [Member]
        public var users: [User]
        public var sessions: [Session]
        public var comments: [Comment]
        public var attachments: [Attachment]
        public var issueEvents: [IssueEvent]
        public var sessionIssues: [SessionIssue]

        public init(
            boards: [Board] = [], issues: [Issue] = [], members: [Member] = [], users: [User] = [],
            sessions: [Session] = [], comments: [Comment] = [], attachments: [Attachment] = [],
            issueEvents: [IssueEvent] = [], sessionIssues: [SessionIssue] = []
        ) {
            self.boards = boards
            self.issues = issues
            self.members = members
            self.users = users
            self.sessions = sessions
            self.comments = comments
            self.attachments = attachments
            self.issueEvents = issueEvents
            self.sessionIssues = sessionIssues
        }
    }

    // MARK: - Helpers (TS `text`, `cmp`, `byId`, `limitOf`, …)

    private typealias Params = [String: String]

    /// JS truthiness of a param (absent or "" = false).
    private static func param(_ params: Params, _ key: String) -> String? {
        guard let value = params[key], !value.isEmpty else { return nil }
        return value
    }

    /// Code-point order, no locale.
    private static func cmp(_ a: String, _ b: String) -> Int {
        let less: (String, String) -> Bool = { x, y in
            x.unicodeScalars.lexicographicallyPrecedes(y.unicodeScalars) { $0.value < $1.value }
        }
        return less(a, b) ? -1 : less(b, a) ? 1 : 0
    }

    private static func num(_ a: Double, _ b: Double) -> Int { a < b ? -1 : a > b ? 1 : 0 }

    /// JS `Number(text)` (nil = NaN).
    private static func jsNumber(_ raw: String) -> Double? {
        let t = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !t.isEmpty else { return 0 }
        let lower = t.lowercased()
        if lower.contains("nan") || lower.contains("inf") || lower.contains("0x") || lower.contains("p") {
            return t == "Infinity" || t == "+Infinity" ? .infinity : t == "-Infinity" ? -.infinity : nil
        }
        return Double(t)
    }

    private static func limitOf(_ params: Params) -> Int {
        guard let raw = params["limit"], let n = jsNumber(raw), n.isFinite, n == n.rounded(), n > 0 else {
            return limitDefault
        }
        return n >= Double(limitMax) ? limitMax : Int(n)
    }

    private static func userName(_ store: Store, _ id: String?) -> String {
        guard let id, !id.isEmpty else { return "" }
        return store.users.first { $0.id == id }?.name ?? ""
    }

    /// A board by id or slug.
    private static func boardOf(_ store: Store, _ ref: String?) -> Board? {
        guard let ref, !ref.isEmpty else { return nil }
        return store.boards.first { $0.id == ref || $0.slug == ref }
    }

    /// An issue by identifier (case-insensitive) or id.
    private static func issueOf(_ store: Store, _ ref: String?) -> Issue? {
        guard let ref, !ref.isEmpty else { return nil }
        let upper = ref.uppercased()
        return store.issues.first { $0.id == ref || $0.identifier.uppercased() == upper }
    }

    private static func str(_ value: String?) -> CardJSON { value.map(CardJSON.string) ?? .null }
    private static func number(_ value: Double?) -> CardJSON { value.map(CardJSON.number) ?? .null }

    // MARK: - FEED-243: touched (TS `boundOf`, `dayStart`, `touchesOf`)

    private static let dayPattern = try! NSRegularExpression(pattern: #"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"#)
    private static let instantPattern = try! NSRegularExpression(
        pattern: #"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]+)?(Z|[+-][0-9]{2}:[0-9]{2})$"#
    )

    private static func matches(_ pattern: NSRegularExpression, _ value: String) -> Bool {
        pattern.firstMatch(in: value, range: NSRange(value.startIndex..., in: value)) != nil
    }

    /// JS `Date.parse` of a stored / bound timestamp, epoch ms (nil = NaN).
    private static func epochMs(_ value: String) -> Double? {
        WireTimestamps.parse(value).map { ($0.timeIntervalSince1970 * 1000).rounded() }
    }

    /// A `since` / `until` bound (epoch ms; `open` = exclusive).
    private struct Bound {
        let ms: Double
        let open: Bool
    }

    /// A date-only bound reads in the zone: `since` = the day's start
    /// (inclusive), `until` = the NEXT day's start (exclusive); an instant is
    /// inclusive both ways; anything else = no bound.
    private static func boundOf(_ raw: String?, end: Bool, zone: TimeZone) -> Bound? {
        guard let v = raw?.trimmingCharacters(in: .whitespacesAndNewlines), !v.isEmpty else { return nil }
        if matches(dayPattern, v) {
            guard epochMs("\(v)T00:00:00Z") != nil else { return nil }
            let parts = v.split(separator: "-").compactMap { Int($0) }
            guard parts.count == 3 else { return nil }
            var calendar = Calendar(identifier: .gregorian)
            calendar.timeZone = zone
            guard let start = calendar.date(from: DateComponents(year: parts[0], month: parts[1], day: parts[2])) else { return nil }
            if !end { return Bound(ms: (start.timeIntervalSince1970 * 1000).rounded(), open: false) }
            guard let next = calendar.date(byAdding: .day, value: 1, to: start) else { return nil }
            return Bound(ms: (calendar.startOfDay(for: next).timeIntervalSince1970 * 1000).rounded(), open: true)
        }
        guard matches(instantPattern, v), let ms = epochMs(v) else { return nil }
        return Bound(ms: ms, open: false)
    }

    /// One touched issue: the latest in-window touch and its kinds.
    private struct Touch {
        var at: String
        var ms: Double
        var touches: [String]
    }

    /// The issues `actor` touched inside [since, until]: an `issue_events`
    /// row they acted, a comment they wrote, a session they started linked
    /// to the issue (at its start and its last update).
    private static func touchesOf(_ store: Store, actor: String, params: Params, timeZone: String?) -> [String: Touch] {
        let zone = TeamZone.hostTimeZone(timeZone)
        let since = boundOf(params["since"], end: false, zone: zone)
        let until = boundOf(params["until"], end: true, zone: zone)
        var out: [String: Touch] = [:]
        func add(_ issueId: String, _ kind: String, _ at: String) {
            guard let ms = epochMs(at) else { return }
            if let since, ms < since.ms { return }
            if let until, (until.open ? ms >= until.ms : ms > until.ms) { return }
            guard var t = out[issueId] else {
                out[issueId] = Touch(at: at, ms: ms, touches: [kind])
                return
            }
            if !t.touches.contains(kind) { t.touches.append(kind) }
            if ms > t.ms {
                t.at = at
                t.ms = ms
            }
            out[issueId] = t
        }
        for e in store.issueEvents where e.actorUserId == actor { add(e.issueId, "event", e.createdAt) }
        for c in store.comments where c.authorId == actor { add(c.issueId, "comment", c.createdAt) }
        var sessions: [String: Session] = [:]
        for s in store.sessions { sessions[s.id] = s }
        for link in store.sessionIssues {
            guard let s = sessions[link.sessionId], s.userId == actor else { continue }
            add(link.issueId, "session", s.createdAt)
            add(link.issueId, "session", s.updatedAt)
        }
        return out.mapValues { t in
            var sorted = t
            sorted.touches.sort { cmp($0, $1) < 0 }
            return sorted
        }
    }

    private static func page(_ rows: [CardJSON], _ params: Params, extra: [String: CardJSON] = [:]) -> CardJSON {
        var out = extra
        out["rows"] = .array(Array(rows.prefix(limitOf(params))))
        out["count"] = .number(Double(rows.count))
        return .object(out)
    }

    // MARK: - The values

    public static func issuesValue(_ store: Store, params: [String: String], me: String?, timeZone: String? = nil) -> CardJSON {
        let boardRef = param(params, "board")
        let board = boardOf(store, boardRef)
        if boardRef != nil, board == nil { return .object(["rows": .array([]), "count": .number(0)]) }
        let assigneeParam = param(params, "assignee")
        let assignee = assigneeParam == "me" ? me : assigneeParam
        let search = param(params, "search").map { $0.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() }
        let status = param(params, "status")
        let priority = param(params, "priority")
        // FEED-243: `touched=me|<userId>` (signed out `me` = no rows).
        let touched: [String: Touch]? = param(params, "touched").map { raw -> [String: Touch] in
            guard let actor = raw == "me" ? me : raw else { return [:] }
            return touchesOf(store, actor: actor, params: params, timeZone: timeZone)
        }
        var rows = store.issues.filter { i in
            if let touched, touched[i.id] == nil { return false }
            if let board, i.boardId != board.id { return false }
            if let status, i.status != status { return false }
            if let priority, (i.priority ?? "") != priority { return false }
            if assigneeParam == "none", i.assigneeId != nil { return false }
            if let assigneeParam, assigneeParam != "none", i.assigneeId != assignee { return false }
            if let search, !search.isEmpty,
               !i.title.lowercased().contains(search), !i.identifier.lowercased().contains(search) { return false }
            return true
        }
        let requested = params["sort"]
        let explicit = requested == "updatedAt" || requested == "createdAt" || requested == "sortOrder"
            || (touched != nil && requested == "touchedAt")
        let sort = explicit ? requested! : touched != nil ? "touchedAt" : "sortOrder"
        let order = params["order"] == "desc" ? "desc" : params["order"] == "asc" ? "asc" : sort == "sortOrder" ? "asc" : "desc"
        rows.sort { a, b in
            let c: Int = switch sort {
            case "updatedAt": cmp(a.updatedAt, b.updatedAt)
            case "createdAt": cmp(a.createdAt, b.createdAt)
            case "touchedAt": num(touched?[a.id]?.ms ?? 0, touched?[b.id]?.ms ?? 0)
            default: num(a.sortOrder, b.sortOrder)
            }
            if c != 0 { return order == "asc" ? c < 0 : c > 0 }
            return cmp(a.id, b.id) < 0
        }
        var slugs: [String: String] = [:]
        for b in store.boards { slugs[b.id] = b.slug }
        return page(rows.map { i -> CardJSON in
            var row: [String: CardJSON] = [
                "id": .string(i.id),
                "identifier": .string(i.identifier),
                "title": .string(i.title),
                // A Select / ChipInput option (shape `option`): bind the rows as its options.
                "value": .string(i.identifier),
                "label": .string("\(i.identifier) \(i.title)"),
                "status": .string(i.status),
                "priority": str(i.priority),
                "assignee": str(i.assigneeId),
                "assigneeName": .string(userName(store, i.assigneeId)),
                "board": .string(i.boardId),
                "boardSlug": .string(slugs[i.boardId] ?? ""),
                "dueDate": str(i.dueDate),
                "updatedAt": .string(i.updatedAt),
            ]
            if let t = touched?[i.id] {
                row["touchedAt"] = .string(t.at)
                row["touches"] = .array(t.touches.map(CardJSON.string))
            }
            return .object(row)
        }, params)
    }

    public static func boardsValue(_ store: Store, params: [String: String]) -> CardJSON {
        let rows = store.boards
            .filter { $0.deletedAt == nil }
            .sorted { a, b in
                let c = num(a.sortOrder, b.sortOrder)
                if c != 0 { return c < 0 }
                let n = cmp(a.name, b.name)
                if n != 0 { return n < 0 }
                return cmp(a.id, b.id) < 0
            }
        return page(rows.map { b in
            .object([
                "id": .string(b.id), "slug": .string(b.slug), "name": .string(b.name), "prefix": .string(b.prefix),
                "icon": str(b.icon), "color": str(b.color), "value": .string(b.id), "label": .string(b.name),
            ])
        }, params)
    }

    public static func membersValue(_ store: Store, params: [String: String], me: String?) -> CardJSON {
        let rows = store.members
            .map { m -> (id: String, name: String, value: CardJSON) in
                let u = store.users.first { $0.id == m.userId }
                let name = u?.name ?? ""
                return (m.userId, name, .object([
                    "id": .string(m.userId), "name": .string(name), "email": str(u?.email), "avatar": str(u?.image),
                    "role": .string(m.role), "value": .string(m.userId), "label": .string(name),
                ]))
            }
            .sorted { a, b in
                let c = cmp(a.name, b.name)
                return c != 0 ? c < 0 : cmp(a.id, b.id) < 0
            }
        return page(rows.map(\.value), params, extra: ["me": str(me)])
    }

    public static func sessionsValue(_ store: Store, params: [String: String], me: String?) -> CardJSON {
        let status = param(params, "status")
        let mine = params["mine"] == "1" || params["mine"] == "true"
        let rows = store.sessions
            .filter { $0.parentId == nil && $0.appId == nil }
            .filter { status == nil || $0.status == status }
            .filter { !mine || (me != nil && $0.userId == me) }
            .sorted { a, b in
                let c = -cmp(a.updatedAt, b.updatedAt)
                return c != 0 ? c < 0 : cmp(a.id, b.id) < 0
            }
        return page(rows.map { s in
            .object([
                "id": .string(s.id),
                "title": .string(s.title ?? ""),
                "status": .string(s.status),
                "busy": .bool(s.busy),
                "needsInput": .bool(s.needsInput),
                "kind": .string((s.agentId ?? "").isEmpty ? "thread" : "agent"),
                "updatedAt": .string(s.updatedAt),
            ])
        }, params)
    }

    public static func commentsValue(_ store: Store, params: [String: String]) -> CardJSON? {
        guard let issue = issueOf(store, params["issue"]) else { return nil }
        let rows = store.comments
            .filter { $0.issueId == issue.id }
            .sorted { a, b in
                let c = cmp(a.createdAt, b.createdAt)
                return c != 0 ? c < 0 : cmp(a.id, b.id) < 0
            }
        return page(rows.map { c in
            .object([
                "id": .string(c.id),
                "body": .string(c.body),
                "author": str(c.authorId),
                "authorName": .string(userName(store, c.authorId)),
                "parent": str(c.parentId),
                "source": .string(c.source),
                "createdAt": .string(c.createdAt),
            ])
        }, params)
    }

    /// VAPP-206: the viewer, so a package's `visible` reads their team role
    /// and app access (nil = write).
    public static func meValue(_ store: Store, me: String?, access: String?) -> CardJSON {
        let user = store.users.first { $0.id == me }
        return .object([
            "id": str(me), "name": .string(user?.name ?? ""), "email": str(user?.email), "avatar": str(user?.image),
            "role": str(store.members.first { $0.userId == me }?.role), "access": .string(access ?? "write"),
        ])
    }

    public static func attachmentsValue(_ store: Store, params: [String: String]) -> CardJSON? {
        guard let issue = issueOf(store, params["issue"]) else { return nil }
        let rows = store.attachments
            .filter { $0.issueId == issue.id }
            .sorted { a, b in
                let c = cmp(a.createdAt, b.createdAt)
                return c != 0 ? c < 0 : cmp(a.id, b.id) < 0
            }
        return page(rows.map { a in
            .object([
                "id": .string(a.id),
                "name": .string(a.filename),
                "type": .string(a.contentType),
                "size": .number(a.sizeBytes),
                "url": .string("/api/attachments/\(a.id)"),
                "width": number(a.width),
                "height": number(a.height),
                "uploader": str(a.uploaderId),
                "uploaderName": .string(userName(store, a.uploaderId)),
                "createdAt": .string(a.createdAt),
            ])
        }, params)
    }

    /// One `exp:<name>?params` over the store: the value the host emits at the
    /// bound path (nil = null: a name this module does not derive, or a
    /// comments / attachments source without a resolvable issue).
    public static func sourceValue(name: String, params: [String: String], store: Store, me: String?, access: String? = nil, timeZone: String? = nil) -> CardJSON? {
        switch name {
        case "me": return meValue(store, me: me, access: access)
        case "issues": return issuesValue(store, params: params, me: me, timeZone: timeZone)
        case "boards": return boardsValue(store, params: params)
        case "members": return membersValue(store, params: params, me: me)
        case "sessions": return sessionsValue(store, params: params, me: me)
        case "comments": return commentsValue(store, params: params)
        case "attachments": return attachmentsValue(store, params: params)
        default: return nil
        }
    }
}

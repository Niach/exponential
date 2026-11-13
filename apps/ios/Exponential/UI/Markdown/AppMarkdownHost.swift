#if canImport(ExponentialUI)
import ExpCore
import ExpUI
@preconcurrency import ExponentialUI
import Foundation
import GRDB
import SwiftUI

/// #VAPP-258: the read-only Markdown's host (catalog/host.json `markdown`):
/// one genUI context surface per account × team × chip vocabulary, whose
/// `HostPlugin` answers the painter's hooks:
/// - `textLinks`: `#IDENT` refs that resolve in the team (an issue chip:
///   identifier + title, the status glyph), the known `@email` mentions (a
///   mention chip: the member's name) and the rewritten in-app links
///   (prose-links.json, an entity chip), every other piece plain;
/// - `imageSize` / `attachment`: the synced attachment rows (a picture's
///   size, a one-link paragraph's video / audio / file);
/// - `mediaRequest`: an attachment on the account's own instance with its
///   bearer (`AttachmentMediaAuth`), anything else anonymous;
/// - `onImagePress`: the pressed picture's text opens THE picture viewer;
/// - `openUrl`: an issue chip opens the issue, a mention is inert, the rest
///   goes to THE link opener (#EXP-1674);
/// - `icon`: the app's registry (a status glyph in its colour).
@MainActor
final class AppMarkdownHost: HostPlugin {
    let model: SurfaceModel
    private let baseURL: URL?
    private let accountId: String
    private let teamId: String?
    private let bareRefs: Bool
    private let origin: String?
    private weak var deps: AppDependencies?
    /// The latest context's (a screen rebuilds its context every pass).
    private var onOpenIssue: ((String) -> Void)?
    private var members: [String: String] = [:]
    /// What the chips baked into a parse depend on (the members).
    private(set) var revision = ""

    /// EXP-1280 (plan-card.json `card`): the plan card's inline picture =
    /// `pictureRadius` corners + a `pictureStroke` hairline in the theme's
    /// border colour, as the `Markdown/image` part of a theme over the app's.
    static let planCardThemeJSON = """
    {"$schema": "https://ui.exponential.at/schemas/theme/v1.json", "id": "exponential-plan-card", "name": "Plan card", "extends": "exponential",
     "recipes": {"Markdown": {"image": [{"style": {"borderRadius": \(PlanCard.Card.pictureRadius), "borderWidth": \(PlanCard.Card.pictureStroke), "borderColor": "$color.border"}}]}}}
    """

    private static func theme(planCard: Bool) -> ThemeHandle? {
        guard planCard else { return ThemeHandle.builtin("exponential") }
        return ThemeHandle.loadOrDefault(json: planCardThemeJSON) { issues in
            assertionFailure("plan-card theme: \(issues.map { "\($0.path) \($0.message)" })")
        }
    }

    private init(context: AgentMarkdownContext?, deps: AppDependencies?, planCard: Bool) throws {
        baseURL = context?.baseURL
        accountId = context?.accountId ?? ""
        teamId = context?.issueRefs?.teamId
        bareRefs = context?.issueRefs?.bareRefs ?? false
        self.deps = deps
        let base = baseURL?.absoluteString
        origin = base.map { $0.hasSuffix("/") ? String($0.dropLast()) : $0 }
        model = try SurfaceModel(
            id: "app-markdown-\(Self.hosts.count)",
            options: SurfaceOptions(theme: Self.theme(planCard: planCard), mode: .dark),
            host: nil
        )
        model.host = self
    }

    // MARK: - The shared hosts

    private static var hosts: [String: AppMarkdownHost] = [:]

    /// The host of `context` (nil = none could be made: no theme core).
    static func host(_ context: AgentMarkdownContext?, deps: AppDependencies?, planCard: Bool = false) -> AppMarkdownHost? {
        let refs = context?.issueRefs
        let key = [
            context?.accountId ?? "", context?.baseURL?.absoluteString ?? "",
            refs?.teamId ?? "-", refs.map { $0.bareRefs ? "bare" : "hash" } ?? "-", planCard ? "plan" : "-",
        ].joined(separator: "|")
        let host: AppMarkdownHost
        if let hit = hosts[key] {
            host = hit
        } else {
            guard let made = try? AppMarkdownHost(context: context, deps: deps, planCard: planCard) else { return nil }
            if hosts.count > 32 { hosts.removeAll(keepingCapacity: true) }
            hosts[key] = made
            host = made
        }
        host.adopt(context, deps: deps)
        return host
    }

    private func adopt(_ context: AgentMarkdownContext?, deps: AppDependencies?) {
        if let deps { self.deps = deps }
        onOpenIssue = context?.issueRefs?.onOpen
        let key = ChatMentionMembers.fingerprint((context?.issueRefs?.mentionMembers ?? []).sorted { $0.email < $1.email })
        if key != revision {
            revision = key
            var map: [String: String] = [:]
            for m in context?.issueRefs?.mentionMembers ?? [] { map[m.email.lowercased()] = m.name }
            members = map
        }
    }

    // MARK: - The source

    /// An entity chip spliced out of the source (prose-links.json).
    private struct Entity {
        let label: String
        let href: String
        let kind: ProseLinks.ChipKind
    }

    private var entities: [Entity] = []
    private var entityIndex: [String: Int] = [:]
    /// Each link's label by destination (an attachment's name).
    private var labels: [String: String] = [:]
    private var prepared: [String: String] = [:]
    private var preparedTick = 0
    /// The pictures' press handlers by raw src (the latest text showing it).
    private var pictureHandlers: [String: (String) -> Void] = [:]

    private static let tokenOpen: Character = "\u{E000}"
    private static let tokenClose: Character = "\u{E001}"
    private static let link = try! NSRegularExpression(pattern: #"(?<!!)\[((?:\\.|[^\\\]])*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)"#)
    private static let image = try! NSRegularExpression(pattern: #"!\[[^\]]*\]\(([^)\s]+)"#)
    private static let urlStops = Set(" \t<>()[]`\"'")
    private static let urlTrailing = Set(".,;:!?*_~")

    /// The text handed to the painter: the agent's bare in-app urls chipped
    /// (prose-links.json, bare-ref surfaces), every rewritten in-app link a
    /// private-use token the text links answer as its entity chip (a
    /// Markdown link's label never asks the host) and, with `autolink`, every
    /// bare `http(s)://` url outside code written as a link.
    func prepare(_ raw: String, autolink: Bool) -> String {
        // A store answer landed: the chips baked into a text may differ.
        if preparedTick != StoreMemoLoads.shared.tick {
            preparedTick = StoreMemoLoads.shared.tick
            prepared.removeAll(keepingCapacity: true)
        }
        let memoKey = "\(autolink ? 1 : 0)\(raw)"
        if let hit = prepared[memoKey] { return hit }
        let markdown = chipped(raw)
        let ns = markdown as NSString
        for m in Self.link.matches(in: markdown, range: NSRange(location: 0, length: ns.length)) {
            let href = ns.substring(with: m.range(at: 2))
            if labels[href] == nil { labels[href] = Self.unescape(ns.substring(with: m.range(at: 1))) }
        }
        let lowered = markdown.lowercased()
        let hasChips = markdown.contains("#exp-chip-")
        let bareUrls = autolink && (lowered.contains("http://") || lowered.contains("https://"))
        var out = markdown
        if hasChips || bareUrls {
            var fence: String?
            out = markdown.components(separatedBy: "\n").map { line -> String in
                if let mark = Self.fenceMark(line) {
                    if fence == nil { fence = mark } else if fence == mark { fence = nil }
                    return line
                }
                return fence == nil ? rewrite(line, chips: hasChips, urls: bareUrls) : line
            }.joined(separator: "\n")
        }
        if prepared.count > 400 { prepared.removeAll(keepingCapacity: true) }
        prepared[memoKey] = out
        return out
    }

    private static func fenceMark(_ line: String) -> String? {
        let rest = line.drop { $0 == " " }
        guard line.count - rest.count <= 3 else { return nil }
        return rest.hasPrefix("```") || rest.hasPrefix("~~~") ? String(rest.prefix(3)) : nil
    }

    /// One line outside a fence: code spans kept, chip links tokenized, bare urls linked.
    private func rewrite(_ line: String, chips: Bool, urls: Bool) -> String {
        let chars = Array(line)
        var out = ""
        var at = 0
        while at < chars.count {
            let ch = chars[at]
            if ch == "`" {
                var run = at
                while run < chars.count, chars[run] == "`" { run += 1 }
                let fence = String(chars[at..<run])
                let rest = String(chars[run...])
                let end: Int
                if let close = rest.range(of: fence) {
                    end = run + rest.distance(from: rest.startIndex, to: close.lowerBound) + fence.count
                } else {
                    end = run
                }
                out += String(chars[at..<end])
                at = end
                continue
            }
            if ch == "[", at == 0 || chars[at - 1] != "!" {
                let rest = String(chars[at...])
                let ns = rest as NSString
                if let m = Self.link.firstMatch(in: rest, options: .anchored, range: NSRange(location: 0, length: ns.length)) {
                    let whole = ns.substring(with: m.range)
                    let href = ns.substring(with: m.range(at: 2))
                    if chips, let kind = ProseLinks.chipKind(ofHref: href) {
                        let index = entityIndex[href] ?? {
                            entities.append(Entity(label: Self.unescape(ns.substring(with: m.range(at: 1))), href: href, kind: kind))
                            entityIndex[href] = entities.count - 1
                            return entities.count - 1
                        }()
                        out += "\(Self.tokenOpen)\(index)\(Self.tokenClose)"
                    } else {
                        out += whole
                    }
                    at += whole.count
                    continue
                }
            }
            if urls, Self.startsURL(chars, at) {
                let prev: Character? = at > 0 ? chars[at - 1] : nil
                let glued = prev.map { $0.isLetter || $0.isNumber || $0 == "_" || $0 == "[" || $0 == "<" || $0 == "(" } ?? false
                if !glued {
                    var end = at
                    while end < chars.count, !Self.urlStops.contains(chars[end]) { end += 1 }
                    while end > at, Self.urlTrailing.contains(chars[end - 1]) { end -= 1 }
                    let url = String(chars[at..<end])
                    if url.count > "https://".count {
                        let label = url.replacingOccurrences(of: "[", with: "\\[").replacingOccurrences(of: "]", with: "\\]")
                        out += "[\(label)](\(url))"
                        at = end
                        continue
                    }
                }
            }
            out.append(ch)
            at += 1
        }
        return out
    }

    private static func startsURL(_ chars: [Character], _ at: Int) -> Bool {
        for scheme in ["http://", "https://"] where at + scheme.count <= chars.count {
            if String(chars[at..<(at + scheme.count)]).lowercased() == scheme { return true }
        }
        return false
    }

    private static func unescape(_ label: String) -> String {
        guard label.contains("\\") else { return label }
        var out = ""
        var escaped = false
        for c in label {
            if escaped { out.append(c); escaped = false } else if c == "\\" { escaped = true } else { out.append(c) }
        }
        return out
    }

    /// #EXP-1674 (prose-links.json): agent prose with its bare in-app URLs
    /// rewritten to their chips, in the run's team. Display text only.
    private func chipped(_ text: String) -> String {
        guard bareRefs, let teamId, let deps, text.contains("://"), let origin else { return text }
        let db = deps.db
        let accountId = accountId
        func cell(_ sql: String, _ arguments: [String]) -> String? {
            Self.cells.peek(Cell(accountId: accountId, sql: sql, arguments: arguments)) {
                guard let pool = try? db.pool(forAccountId: accountId) else { return nil }
                return (try? await pool.read { db in
                    try String.fetchOne(db, sql: sql, arguments: StatementArguments(arguments))
                }) ?? nil
            }
        }
        func cell(_ sql: String, _ first: String, _ second: String) -> String? { cell(sql, [first, second]) }
        let team = cell(#"SELECT "slug" FROM "teams" WHERE "id" = ? LIMIT 1"#, [teamId])
        return ProseLinks.chipBareAppLinks(text, origin: origin, team: team) { kind, id in
            switch kind {
            case .issue:
                return IssueRefChipCache.chip(id.uppercased(), scope: .team(id: teamId), db: db, accountId: accountId)?.title
            case .board:
                return cell(#"SELECT "name" FROM "boards" WHERE "team_id" = ? AND "slug" = ? LIMIT 1"#, teamId, id)
            case .session:
                guard cell(#"SELECT "id" FROM "sessions" WHERE "team_id" = ? AND "id" = ? LIMIT 1"#, teamId, id) != nil else { return nil }
                let title = cell(#"SELECT "title" FROM "sessions" WHERE "team_id" = ? AND "id" = ? LIMIT 1"#, teamId, id)
                return title.flatMap { $0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : $0 } ?? "Session"
            case .agent:
                return cell(#"SELECT "name" FROM "agents" WHERE "team_id" = ? AND "id" = ? LIMIT 1"#, teamId, id)
            }
        }
    }

    /// One cell of the store, for `chipped` (#PERF-5: never a blocking read).
    private struct Cell: Hashable, Sendable {
        let accountId: String
        let sql: String
        let arguments: [String]
    }

    private static let cells = StoreMemo<Cell, String>()

    /// The pictures of `source` open through `open` (the raw src).
    func registerPictures(in source: String, open: @escaping (String) -> Void) {
        let ns = source as NSString
        for m in Self.image.matches(in: source, range: NSRange(location: 0, length: ns.length)) {
            pictureHandlers[ns.substring(with: m.range(at: 1))] = open
        }
    }

    // MARK: - HostPlugin

    private static let issueScheme = "exp-issue:"
    private static let mentionScheme = "exp-mention:"
    private static let statusIcon = "issue-status:"

    private var scope: IssueRefLookup.Scope? { teamId.map { .team(id: $0) } }

    func textLinks(_ piece: String) -> [TextLinkSegment]? {
        struct Hit {
            let range: NSRange
            let segment: TextLinkSegment
        }
        let ns = piece as NSString
        var hits: [Hit] = []
        if !entities.isEmpty, piece.contains(Self.tokenOpen) {
            let token = try! NSRegularExpression(pattern: "\u{E000}(\\d+)\u{E001}")
            for m in token.matches(in: piece, range: NSRange(location: 0, length: ns.length)) {
                guard let i = Int(ns.substring(with: m.range(at: 1))), entities.indices.contains(i) else { continue }
                let e = entities[i]
                hits.append(Hit(range: m.range, segment: TextLinkSegment(text: ns.substring(with: m.range), href: e.href, chip: TextLinkChip(kind: "entity", label: e.label, icon: e.kind.icon))))
            }
        }
        if let scope, let deps {
            for match in IssueRefs.matches(in: piece, bare: bareRefs) {
                guard let chip = IssueRefChipCache.chip(match.identifier, scope: scope, db: deps.db, accountId: accountId) else { continue }
                let title = IssueRefs.chipTitle(chip.title)
                let label = title.isEmpty ? match.identifier : "\(match.identifier) \(title)"
                hits.append(Hit(range: match.range, segment: TextLinkSegment(
                    text: ns.substring(with: match.range), href: Self.issueScheme + match.identifier,
                    chip: TextLinkChip(kind: "issue", label: label, icon: Self.statusIcon + match.identifier)
                )))
            }
        }
        if !members.isEmpty {
            for match in MentionRefs.matches(in: piece) {
                guard let name = members[match.email.lowercased()] else { continue }
                let label = name.trimmingCharacters(in: .whitespaces)
                hits.append(Hit(range: match.range, segment: TextLinkSegment(
                    text: ns.substring(with: match.range), href: Self.mentionScheme + match.email,
                    chip: TextLinkChip(kind: "mention", label: label.isEmpty ? match.email : label)
                )))
            }
        }
        guard !hits.isEmpty else { return nil }
        var out: [TextLinkSegment] = []
        var at = 0
        for hit in hits.sorted(by: { $0.range.location < $1.range.location }) where hit.range.location >= at {
            if hit.range.location > at { out.append(TextLinkSegment(text: ns.substring(with: NSRange(location: at, length: hit.range.location - at)))) }
            out.append(hit.segment)
            at = hit.range.location + hit.range.length
        }
        if at < ns.length { out.append(TextLinkSegment(text: ns.substring(from: at))) }
        return out
    }

    func icon(_ name: String, size: CGFloat) -> AnyView? {
        if name.hasPrefix(Self.statusIcon) {
            guard let scope, let deps,
                  let status = IssueRefChipCache.chip(String(name.dropFirst(Self.statusIcon.count)), scope: scope, db: deps.db, accountId: accountId)?.status
            else { return nil }
            return AnyView(AppIcon(status.iconName, size: size).foregroundStyle(status.color))
        }
        guard let resolved = AppIcons.conceptOrName(name) else { return nil }
        return AnyView(AppIcon(resolved, size: size))
    }

    private func attachmentInfo(_ src: String) -> AttachmentMediaInfo? {
        guard let deps, let id = AttachmentLinks.attachmentId(fromUrl: src, baseURL: baseURL) else { return nil }
        return AttachmentInfoCache.peek(id, db: deps.db, accountId: accountId)
    }

    func imageSize(_ src: String) -> CGSize? {
        guard let info = attachmentInfo(src), let w = info.width, let h = info.height, w > 0, h > 0 else { return nil }
        return CGSize(width: w, height: h)
    }

    func attachment(_ href: String) -> MarkdownAttachmentInfo? {
        guard let info = attachmentInfo(href) else { return nil }
        let kind = info.isVideo ? "video" : info.isAudio ? "audio" : "file"
        let label = labels[href]?.trimmingCharacters(in: .whitespaces) ?? ""
        let name = label.isEmpty ? (href.split(separator: "/").last.map(String.init) ?? href) : label
        return MarkdownAttachmentInfo(kind: kind, name: name)
    }

    var onImagePress: (@MainActor (String) -> Void)? {
        { [weak self] src in self?.pictureHandlers[src]?(src) }
    }

    var urlPolicy: UrlPolicy? {
        let linkBase = deps.flatMap { LinkOpener.linkBase(teamId: teamId, accountId: accountId, deps: $0) }
        return UrlPolicy(
            schemes: ["https", "http", "mailto", "tel", "exp-issue", "exp-mention"],
            allowPrivate: baseURL?.host.map { [$0] }, baseUrl: origin, linkBase: linkBase
        )
    }

    var mediaOptions: MediaOptions? {
        let schemes = ["https"] + (baseURL?.scheme == "http" ? ["http"] : []) + ["data"]
        return MediaOptions(baseUrl: origin, schemes: schemes, allowPrivate: baseURL?.host.map { [$0] })
    }

    /// Styleguide captures only: a specimen attachment paints its sample
    /// (`SpecimenPicture`) as a data url, never fetching.
    private static var specimens: [String: URLRequest] = [:]

    private static func specimen(_ src: String) -> URLRequest? {
        guard UITestingStyleguide.requested != nil else { return nil }
        if let hit = specimens[src] { return hit }
        guard let png = SpecimenPicture.image(src)?.pngData(),
              let url = URL(string: "data:image/png;base64,\(png.base64EncodedString())") else { return nil }
        let request = URLRequest(url: url)
        specimens[src] = request
        return request
    }

    func mediaRequest(_ src: String) -> URLRequest? {
        if let specimen = Self.specimen(src) { return specimen }
        guard AttachmentLinks.attachmentId(fromUrl: src, baseURL: baseURL) != nil else {
            return policyMediaRequest(src, options: mediaOptions)
        }
        let token = deps?.auth.accounts.first(where: { $0.id == accountId })?.token
        guard let resolved = AttachmentMediaAuth.request(for: src, instanceBaseURL: baseURL, token: token) else { return nil }
        var request = URLRequest(url: resolved.url)
        for (k, v) in resolved.headers { request.setValue(v, forHTTPHeaderField: k) }
        return request
    }

    func openUrl(_ url: String) {
        if url.hasPrefix(Self.issueScheme) {
            let identifier = String(url.dropFirst(Self.issueScheme.count))
            guard let onOpenIssue, let scope, let deps,
                  let chip = IssueRefChipCache.chip(identifier, scope: scope, db: deps.db, accountId: accountId)
            else { return }
            onOpenIssue(chip.issueId)
            return
        }
        if url.hasPrefix(Self.mentionScheme) { return }
        // A rewritten in-app link opens at its own url (the chip marker off).
        let href = url.range(of: "#exp-chip-", options: .backwards).map { String(url[..<$0.lowerBound]) } ?? url
        guard let tapped = URL(string: href) else { return }
        guard let deps else {
            UIApplication.shared.open(tapped)
            return
        }
        let handler = LinkOpener.surfaceHandler(accountId: accountId.isEmpty ? nil : accountId, baseURL: baseURL, teamId: teamId, deps: deps)
        if let action = handler(tapped) { action() } else { UIApplication.shared.open(tapped) }
    }
}
#endif

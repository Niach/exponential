import ExpCore
import ExpUI
#if canImport(ExponentialUI)
import ExponentialUI
#endif
import Foundation
import GRDB

/// #EXP-1674: where a link resolved to; `MainNavigator.openLinkLanding` lands
/// it (switching account and team first).
struct LinkLanding: Equatable {
    enum Destination: Equatable {
        /// The screen the app lands on (no access).
        case home
        /// A bar destination's root.
        case tab(MainTab)
        /// The Board tab on this board.
        case board(String)
        /// A page pushed over the current stack, or `over` a tab's root.
        case push(AppRoute, over: MainTab?)
    }

    let accountId: String
    /// The team to make active first; nil = stay.
    let teamId: String?
    let destination: Destination
}

/// #EXP-1674: THE link opener (app-link.json): markdown links, app surfaces'
/// links and universal links all come through here. `AppLink.classify` names
/// the link, `AppLink.linkOpenOutcome` decides, and every in-app kind lands on
/// its native screen. A page the phone has no screen for = the in-app Safari
/// sheet, never UIApplication.open: the app is entitled for the link and would
/// re-open itself.
@MainActor
enum LinkOpener {
    // MARK: - Entries

    /// A read-only markdown render's link tap. `accountId` nil = the active
    /// account at tap time; `teamId` nil = its active team.
    static func surfaceHandler(
        accountId: String?, baseURL: URL?, teamId: String?, deps: AppDependencies
    ) -> MarkdownLinkHandler {
        return { url in
            let account = accountId ?? deps.auth.activeAccountId ?? ""
            let base = baseURL ?? deps.auth.instanceBaseURL(forAccountId: account)
            return LinkOpener.tapAction(
                url, origin: base?.absoluteString ?? "", accountId: account, teamId: teamId, deps: deps
            )
        }
    }

    /// What a tapped link runs; nil = keep the system's own open (`mailto:`).
    /// `teamId` = the surface's team (nil = the active team), for `~/`.
    static func tapAction(
        _ tapped: URL, origin: String, accountId: String, teamId: String?, deps: AppDependencies
    ) -> (() -> Void)? {
        let written = tapped.absoluteString
        guard written.hasPrefix("~") else {
            return action(written, team: nil, origin: origin, accountId: accountId, deps: deps)
        }
        // #PERF-5: `~/` names the team; its slug loads off main at tap time.
        return {
            Task { @MainActor in
                let team = await loadTeamSlug(teamId: teamId, accountId: accountId, deps: deps)
                action(written, team: team, origin: origin, accountId: accountId, deps: deps)?()
            }
        }
    }

    private static func action(
        _ written: String, team: String?, origin: String, accountId: String, deps: AppDependencies
    ) -> (() -> Void)? {
        let link = AppLink.classify(written, origin: origin, team: team)
        switch link {
        case .ignore:
            return {}
        case let .external(href):
            // A web source opens in the in-app Safari sheet, like Android's
            // Custom Tab; mailto keeps the system handler.
            guard let url = URL(string: href),
                  let scheme = url.scheme?.lowercased(), scheme == "http" || scheme == "https"
            else { return nil }
            return { deps.deepLinkBus.openExternal(url) }
        default:
            break
        }
        // F37: a relative link means nothing without the account's instance.
        guard let url = absoluteURL(written, team: team, origin: origin) else { return {} }
        return {
            LinkOpener.open(link, url: url, accountIds: [accountId], waitsForSync: false, deps: deps)
        }
    }

    /// A universal link (EXP-92): under the signed-in account on its host that
    /// can open it, active account first. A foreign host = the Safari sheet.
    static func openWebLink(_ url: URL, deps: AppDependencies) {
        if case let .invite(token) = WebLinks.parse(url) {
            deps.deepLinkBus.navigateToInvite(token)
            return
        }
        let accountIds = accountsOnHost(of: url, deps: deps).map(\.id)
        guard !accountIds.isEmpty, let origin = ownOrigin(of: url) else {
            deps.deepLinkBus.openExternal(url)
            return
        }
        let link = AppLink.classify(url.absoluteString, origin: origin)
        switch link {
        case .external, .ignore:
            // Never swallow a link the user tapped.
            deps.deepLinkBus.openExternal(url)
        default:
            open(link, url: url, accountIds: accountIds, waitsForSync: true, deps: deps)
        }
    }

    /// #EXP-1674 (plan-card.json): a chat card's link, in-app and https only,
    /// with the card's team for `~/` (nil = the active team).
    static func openCardLink(_ url: String, origin: String, accountId: String, teamId: String?, deps: AppDependencies) {
        Task { @MainActor in
            let team = await loadTeamSlug(teamId: teamId, accountId: accountId, deps: deps)
            guard AppLink.cardLinkOpens(url, origin: origin, team: team), let tapped = URL(string: url) else { return }
            let written = tapped.absoluteString
            action(written, team: written.hasPrefix("~") ? team : nil, origin: origin, accountId: accountId, deps: deps)?()
        }
    }

    #if canImport(ExponentialUI)
    /// #EXP-1674: a host's `textLinks` for a `hostLinks` Text (the Row subtitle):
    /// the `#IDENT` refs that resolve in the team, on its short issue route; an
    /// unresolved ref stays text. nil = no link. #PERF-5: memory only (a
    /// render asks); an unknown slug or ref loads off main and re-renders.
    static func issueTextLinks(_ text: String, accountId: String, teamId: String?, deps: AppDependencies) -> [TextLinkSegment]? {
        let matches = IssueRefs.matches(in: text)
        guard !matches.isEmpty, let team = teamSlug(teamId: teamId, accountId: accountId, deps: deps),
              let id = teamId ?? SharedBoardMirror.readActiveTeam(accountId: accountId)
        else { return nil }
        let ns = text as NSString
        var segments: [TextLinkSegment] = []
        var at = 0
        for match in matches {
            let identifier = match.identifier.uppercased()
            guard IssueRefChipCache.chip(identifier, scope: .team(id: id), db: deps.db, accountId: accountId) != nil,
                  let href = AppLink.expandTeamLink("~/issues/\(match.identifier)", team: team)
            else { continue }
            if match.range.location > at {
                segments.append(TextLinkSegment(text: ns.substring(with: NSRange(location: at, length: match.range.location - at))))
            }
            segments.append(TextLinkSegment(text: ns.substring(with: match.range), href: href))
            at = match.range.location + match.range.length
        }
        if segments.isEmpty { return nil }
        if at < ns.length { segments.append(TextLinkSegment(text: ns.substring(from: at))) }
        return segments
    }
    #endif

    /// The slug of `teamId`, else of the account's active team.
    /// #EXP-1674: `~/x`'s base in a surface's URL policy (`/t/<team>/`).
    static func linkBase(teamId: String?, accountId: String, deps: AppDependencies) -> String? {
        AppLink.expandTeamLink("~/", team: teamSlug(teamId: teamId, accountId: accountId, deps: deps))
    }

    /// #PERF-5: memory only, for a render (`linkBase`, `issueTextLinks`): a
    /// miss is nil and loads off main (`StoreMemoLoads` re-renders). A tap
    /// awaits `loadTeamSlug`.
    static func teamSlug(teamId: String?, accountId: String, deps: AppDependencies) -> String? {
        let id = teamId ?? SharedBoardMirror.readActiveTeam(accountId: accountId)
        let db = deps.db
        return slugs.peek(SlugKey(accountId: accountId, teamId: id)) {
            await fetchTeamSlug(id, accountId: accountId, db: db)
        }
    }

    private struct SlugKey: Hashable, Sendable {
        let accountId: String
        let teamId: String?
    }

    /// Slugs barely change: a long TTL keeps renders off the store.
    private static let slugs = StoreMemo<SlugKey, String>(ttl: 60)

    static func loadTeamSlug(teamId: String?, accountId: String, deps: AppDependencies) async -> String? {
        await fetchTeamSlug(teamId ?? SharedBoardMirror.readActiveTeam(accountId: accountId), accountId: accountId, db: deps.db)
    }

    private nonisolated static func fetchTeamSlug(_ id: String?, accountId: String, db: DatabaseManager) async -> String? {
        guard let pool = try? db.pool(forAccountId: accountId) else { return nil }
        return (try? await pool.read { db -> String? in
            if let id, let slug = try String.fetchOne(
                db, sql: #"SELECT "slug" FROM "teams" WHERE "id" = ? LIMIT 1"#, arguments: [id]
            ) {
                return slug
            }
            return try String.fetchOne(db, sql: #"SELECT "slug" FROM "teams" LIMIT 1"#)
        }) ?? nil
    }

    // MARK: - Opening

    /// One account's answer for a link.
    private struct Resolution {
        let outcome: LinkOpenOutcome
        /// nil on `notFound` = no screen to show it on: the toast.
        var landing: LinkLanding?

        /// Which account's answer wins on a multi-account device.
        var rank: Int {
            switch outcome {
            case .open, .switchTeam: 3
            case .browser: 2
            case .notFound: 1
            case .noAccess: 0
            }
        }
    }

    private struct Target {
        var found = true
        var webOnly = false
        var teamId: String
        var destination: LinkLanding.Destination?
    }

    /// `waitsForSync`: a cold launch's link may name what has not synced yet:
    /// one sync pass, then (a target with no not-found screen) a bounded poll
    /// before giving up; opening the link has just restarted the pipelines.
    /// #PERF-5: resolves off main (async reads), then lands.
    private static func open(
        _ link: AppLink, url: URL, accountIds: [String], waitsForSync: Bool, deps: AppDependencies
    ) {
        Task { @MainActor in
            let first = await bestResolution(link, accountIds: accountIds, deps: deps)
            let missed = first.outcome == .noAccess || (first.outcome == .notFound && first.landing == nil)
            guard waitsForSync, missed else {
                apply(first, url: url, deps: deps)
                return
            }
            await deps.syncManager.initialSync()
            var result = await bestResolution(link, accountIds: accountIds, deps: deps)
            if result.outcome == .notFound, result.landing == nil {
                for _ in 0..<8 {
                    try? await Task.sleep(for: .milliseconds(500))
                    result = await bestResolution(link, accountIds: accountIds, deps: deps)
                    if result.outcome != .notFound { break }
                }
            }
            apply(result, url: url, deps: deps)
        }
    }

    private static func apply(_ resolution: Resolution, url: URL, deps: AppDependencies) {
        let bus = deps.deepLinkBus
        switch resolution.outcome {
        case .open, .switchTeam:
            if let landing = resolution.landing { bus.navigateToLink(landing) }
        case .browser:
            bus.openExternal(url)
        case .notFound:
            if let landing = resolution.landing {
                bus.navigateToLink(landing)
            } else {
                Toaster.shared.error(AppLink.noAccessCopy)
            }
        case .noAccess:
            Toaster.shared.error(AppLink.noAccessCopy)
            // Home where the viewer is: a refused link never switches account.
            bus.navigateToLink(LinkLanding(
                accountId: deps.auth.activeAccountId ?? "", teamId: nil, destination: .home
            ))
        }
    }

    private static func bestResolution(
        _ link: AppLink, accountIds: [String], deps: AppDependencies
    ) async -> Resolution {
        var best: Resolution?
        for accountId in accountIds {
            let resolution = await resolve(link, accountId: accountId, deps: deps)
            if let current = best, current.rank >= resolution.rank { continue }
            best = resolution
        }
        return best ?? Resolution(outcome: .noAccess, landing: nil)
    }

    private static func resolve(_ link: AppLink, accountId: String, deps: AppDependencies) async -> Resolution {
        guard let slug = link.teamSlug else {
            // No team to be a member of (`/admin`, the root): a web page.
            let outcome = AppLink.linkOpenOutcome(member: true, activeTeam: true, found: true, webOnly: true)
            return Resolution(outcome: outcome, landing: nil)
        }
        guard let pool = try? deps.db.pool(forAccountId: accountId),
              let teamId = (try? await pool.read { db -> String? in
                  try String.fetchOne(
                      db, sql: #"SELECT "id" FROM "teams" WHERE "slug" = ? LIMIT 1"#, arguments: [slug]
                  )
              }) ?? nil
        else {
            let outcome = AppLink.linkOpenOutcome(member: false, activeTeam: false, found: false, webOnly: false)
            return Resolution(outcome: outcome, landing: nil)
        }
        let resolved = await target(of: link, teamId: teamId, teamSlug: slug, accountId: accountId, pool: pool, deps: deps)
        let outcome = AppLink.linkOpenOutcome(
            member: true,
            activeTeam: resolved.teamId == SharedBoardMirror.readActiveTeam(accountId: accountId),
            found: resolved.found,
            webOnly: resolved.webOnly
        )
        let landing = resolved.destination.map {
            LinkLanding(accountId: accountId, teamId: resolved.teamId, destination: $0)
        }
        return Resolution(outcome: outcome, landing: landing)
    }

    /// The kind → screen map, for a member of the link's team. A destination
    /// with `found: false` = a screen that says not found by itself (chat,
    /// plan page, agent page).
    private static func target(
        of link: AppLink, teamId: String, teamSlug: String, accountId: String, pool: DatabasePool, deps: AppDependencies
    ) async -> Target {
        switch link {
        case let .issue(_, _, identifier), let .issueRef(_, identifier):
            guard let id = await IssueRefLookup.resolve(
                identifier: identifier, teamSlug: teamSlug, db: deps.db, accountId: accountId
            ) else { return Target(found: false, teamId: teamId) }
            return Target(teamId: teamId, destination: .push(.issue(accountId: accountId, id: id), over: nil))
        case let .board(_, boardSlug):
            let id = await rowId(
                #"SELECT "id" FROM "boards" WHERE "team_id" = ? AND "slug" = ? LIMIT 1"#, teamId, boardSlug, pool: pool
            )
            guard let id else { return Target(found: false, teamId: teamId) }
            return Target(teamId: teamId, destination: .board(id))
        case let .session(_, sessionId):
            // EXP-1291: a root (or an id not synced here: the chat screen
            // resolves or says not found) opens its chat, a child its session.
            let row = await session(sessionId, pool: pool)
            let route: AppRoute = if let row, !row.isRoot {
                await AppRoute.openingInbox(sessionId, accountId: accountId, deps: deps)
            } else {
                AppRoute.chat(accountId: accountId, chatId: sessionId, child: nil)
            }
            return Target(found: row != nil, teamId: row?.teamId ?? teamId, destination: .push(route, over: nil))
        case let .plan(_, sessionId, messageId):
            let row = await session(sessionId, pool: pool)
            return Target(
                found: row != nil, teamId: row?.teamId ?? teamId,
                destination: .push(await AppRoute.openingPlan(sessionId, messageId: messageId, accountId: accountId, deps: deps), over: nil)
            )
        case .chats:
            return Target(teamId: teamId, destination: .tab(.chats))
        case let .agent(_, agentId):
            let id = await rowId(
                #"SELECT "id" FROM "agents" WHERE "team_id" = ? AND "id" = ? LIMIT 1"#, teamId, agentId, pool: pool
            )
            return Target(
                found: id != nil, teamId: teamId,
                destination: .push(.agentChannel(accountId: accountId, agentId: agentId), over: nil)
            )
        case .agentHome:
            return Target(teamId: teamId, destination: .tab(.agent))
        case .inbox:
            // EXP-1373: the Inbox is a page pushed over the Chats tab.
            return Target(teamId: teamId, destination: .push(.myWork, over: .chats))
        case .devices:
            return Target(teamId: teamId, destination: .push(
                .teamSettingsSection(accountId: accountId, teamId: teamId, section: .devices), over: .team
            ))
        case let .settings(_, section):
            if section.isEmpty { return Target(teamId: teamId, destination: .tab(.team)) }
            // A category with no phone page (billing, statuses…) is web-only.
            guard let page = TeamSettingsSection(rawValue: section) else {
                return Target(webOnly: true, teamId: teamId)
            }
            // `settings-nav.json` `visible: owner`: not an owner reads as missing.
            if section == "general" || section == "widget" {
                let userId = deps.auth.accounts.first(where: { $0.id == accountId })?.userId
                let owner = (try? await pool.read { db in
                    try Int.fetchOne(
                        db,
                        sql: #"SELECT COUNT(*) FROM "team_members" WHERE "team_id" = ? AND "user_id" = ? AND "role" = ?"#,
                        arguments: [teamId, userId, DomainContract.teamRoleOwner]
                    )
                }) ?? nil
                if (owner ?? 0) == 0 { return Target(found: false, teamId: teamId) }
            }
            return Target(teamId: teamId, destination: .push(
                .teamSettingsSection(accountId: accountId, teamId: teamId, section: page), over: .team
            ))
        case let .appHome(_, appSlug):
            return await appTarget(appSlug, screen: nil, teamId: teamId, accountId: accountId, pool: pool)
        case let .appScreen(_, appSlug, screen, params):
            return await appTarget(
                appSlug, screen: Apps.ScreenRequest(screen: screen, params: params),
                teamId: teamId, accountId: accountId, pool: pool
            )
        case .app:
            return Target(webOnly: true, teamId: teamId)
        case .external, .ignore:
            return Target(found: false, teamId: teamId)
        }
    }

    /// VAPP-242 (apps.json `screens`): THE RUN VIEW over the Apps tab.
    private static func appTarget(
        _ appSlug: String, screen: Apps.ScreenRequest?, teamId: String, accountId: String, pool: DatabasePool
    ) async -> Target {
        let id = await rowId(
            #"SELECT "id" FROM "apps" WHERE "team_id" = ? AND "slug" = ? AND "deleted_at" IS NULL LIMIT 1"#,
            teamId, appSlug, pool: pool
        )
        guard let id else { return Target(found: false, teamId: teamId) }
        return Target(teamId: teamId, destination: .push(
            .appRun(accountId: accountId, appId: id, screen: screen), over: .apps
        ))
    }

    private static func rowId(_ sql: String, _ first: String, _ second: String, pool: DatabasePool) async -> String? {
        (try? await pool.read { db -> String? in
            try String.fetchOne(db, sql: sql, arguments: [first, second])
        }) ?? nil
    }

    private static func session(_ id: String, pool: DatabasePool) async -> SessionEntity? {
        (try? await pool.read { db in
            try SessionQueries.session(db: db, id: id)
        }) ?? nil
    }

    // MARK: - Hrefs

    /// The URL a web-only page opens at; nil = a relative link with no instance.
    private static func absoluteURL(_ written: String, team: String?, origin: String) -> URL? {
        let href = written.trimmingCharacters(in: .whitespacesAndNewlines)
        let path = AppLink.expandTeamLink(href, team: team) ?? href
        guard path.hasPrefix("/") else { return URL(string: path) }
        guard let base = WebLinks.normalizedBase(origin) else { return nil }
        return URL(string: base + path)
    }

    /// `scheme://host[:port]` of a link already matched to an account's host.
    private static func ownOrigin(of url: URL) -> String? {
        guard let scheme = url.scheme, let host = url.host else { return nil }
        let port = url.port.map { ":\($0)" } ?? ""
        return "\(scheme)://\(host)\(port)"
    }

    /// Signed-in accounts on the link's instance: active account first, then
    /// most recently used (a device can hold several accounts on one host).
    private static func accountsOnHost(of url: URL, deps: AppDependencies) -> [ServerAccount] {
        let host = url.host
        let activeId = deps.auth.activeAccountId
        return deps.auth.accounts
            .filter { $0.token != nil && URL(string: $0.instanceUrl)?.host == host }
            .sorted { a, b in
                if a.id == activeId { return true }
                if b.id == activeId { return false }
                return a.lastUsedAt > b.lastUsedAt
            }
    }
}

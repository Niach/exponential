import ExpCore
import ExpUI
import Foundation
import GRDB
import OSLog
import SwiftUI
import UIKit
import WidgetKit

private let logger = Logger(subsystem: "at.exponential", category: "HomeWidgets")

/// #EXP-1830 (home-widgets.json DATA): keeps the home screen widgets'
/// snapshot current. One observation of the ACTIVE account's teams, boards,
/// statuses and open issues (throttled: an issue row moves often), plus the
/// Needs you rows the navigator folds (`setNeedsYou`); every change writes
/// `HomeWidgets.Snapshot` into the app group and reloads the timelines.
/// Signed out = the signed-out snapshot.
@MainActor
final class HomeWidgetsPublisher {
    static let shared = HomeWidgetsPublisher()

    private static let throttle: Duration = .seconds(2)

    private var task: Task<Void, Never>?
    private var boundAccountId: String?
    private var me: String?
    private var lists: Lists?
    private var needsYou: [HomeWidgets.Snapshot.NeedsYouItem] = []

    /// Reconcile with the auth state (call when the accounts may have changed).
    func refresh(auth: AuthRepository, db: DatabaseManager) {
        guard auth.hasAuthenticatedAccount, let accountId = auth.activeAccountId,
              let account = auth.accounts.first(where: { $0.id == accountId && $0.token != nil })
        else {
            stop()
            write()
            return
        }
        guard accountId != boundAccountId else { return }
        stop()
        boundAccountId = accountId
        me = account.userId
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in try Self.read(db) }.removeDuplicates()
        task = Task { @MainActor [weak self] in
            do {
                for try await lists in observation.values(in: pool, bufferingPolicy: .bufferingNewest(1)) {
                    guard let self, self.boundAccountId == accountId else { return }
                    self.lists = lists
                    self.write()
                    try await Task.sleep(for: Self.throttle)
                }
            } catch is CancellationError {
            } catch {
                logger.error("Home widgets stream ended: \(error.localizedDescription)")
            }
        }
    }

    /// THE Needs you rows of the active account (every team), newest first.
    func setNeedsYou(_ rows: [NeedsYou.Row]) {
        let items = rows.map { row in
            HomeWidgets.Snapshot.NeedsYouItem(
                row: HomeWidgets.NeedsYouInput(
                    messageId: row.messageId, sessionId: row.sessionId, rootId: row.rootId,
                    // The widget reads the age off `createdAtMs` as its clock moves.
                    kind: row.kind.rawValue, opens: row.opens.rawValue, title: row.title, age: ""
                ),
                createdAtMs: SessionTree.ms(row.createdAt) ?? 0
            )
        }
        guard items != needsYou else { return }
        needsYou = items
        write()
    }

    private func stop() {
        task?.cancel()
        task = nil
        boundAccountId = nil
        me = nil
        lists = nil
        needsYou = []
    }

    private func write() {
        let snapshot: HomeWidgets.Snapshot
        if let me, let lists {
            snapshot = HomeWidgets.Snapshot(
                me: me,
                teams: lists.teams,
                boards: lists.boards,
                statuses: lists.statuses,
                issues: lists.issues,
                statusGlyphs: lists.statusGlyphs,
                needsYou: needsYou,
                sealedRootIds: lists.sealedRootIds,
                glyphs: Self.glyphs
            )
        } else if boundAccountId != nil {
            // Bound, first read not landed: keep what the widgets show.
            return
        } else {
            snapshot = .signedOut
        }
        if HomeWidgets.writeSnapshot(snapshot) {
            WidgetCenter.shared.reloadAllTimelines()
        }
    }

    private static let glyphs = HomeWidgets.Snapshot.Glyphs(
        myIssues: asset(HomeWidgets.Icons.myIssues),
        needsYou: asset(HomeWidgets.Icons.needsYou),
        plan: asset(HomeWidgets.Icons.plan),
        question: asset(HomeWidgets.Icons.question),
        pr: asset(HomeWidgets.Icons.pr)
    )

    nonisolated private static func asset(_ concept: String) -> String {
        AppIcons.assetName(AppIcons.concepts[concept] ?? concept) ?? ""
    }

    // MARK: - The account's lists

    private struct Lists: Equatable, Sendable {
        let teams: [HomeWidgets.Snapshot.Team]
        let boards: [HomeWidgets.Snapshot.Board]
        let statuses: [HomeWidgets.Status]
        let issues: [HomeWidgets.Issue]
        let statusGlyphs: [String: String]
        let sealedRootIds: [String]
    }

    nonisolated private static func read(_ db: Database) throws -> Lists {
        let teams = try TeamEntity.fetchAll(db)
        let boards = try BoardEntity.fetchAll(db)
        let statusRows = try IssueStatusEntity.fetchAll(db)
        let issues = try IssueEntity.fetchAll(db)
        let sealedRootIds = try String.fetchAll(db, sql: """
            SELECT "id" FROM "sessions"
            WHERE "parent_id" IS NULL AND ("seal_public_key" IS NOT NULL OR "sealed_title" IS NOT NULL)
            """)
        let boardTeam = Dictionary(boards.map { ($0.id, $0.teamId) }, uniquingKeysWith: { first, _ in first })
        let rowsByTeam = Dictionary(grouping: statusRows, by: \.teamId)
        let sortOrders = Dictionary(statusRows.map { ($0.id, $0.sortOrder ?? 0) }, uniquingKeysWith: { first, _ in first })
        var teamStatuses: [String: [ResolvedIssueStatus]] = [:]
        func resolvedTeam(_ teamId: String) -> [ResolvedIssueStatus] {
            if let cached = teamStatuses[teamId] { return cached }
            let resolved = IssueStatusResolver.teamStatusesOrFallback(rowsByTeam[teamId] ?? [])
            teamStatuses[teamId] = resolved
            return resolved
        }
        var statuses: [String: HomeWidgets.Status] = [:]
        var glyphs: [String: String] = [:]
        var open: [HomeWidgets.Issue] = []
        for issue in issues {
            guard let teamId = boardTeam[issue.boardId] else { continue }
            let team = resolvedTeam(teamId)
            let status = IssueStatusResolver.resolve(issue, team: team)
            guard HomeWidgets.openCategories.contains(status.category.rawValue) else { continue }
            if statuses[status.id] == nil {
                statuses[status.id] = HomeWidgets.Status(
                    id: status.id,
                    category: status.category.rawValue,
                    color: hex(status.color),
                    sortOrder: status.rowId.flatMap { sortOrders[$0] }
                        ?? Double(team.firstIndex { $0.id == status.id } ?? 0)
                )
                glyphs[status.id] = AppIcons.assetName(status.iconName) ?? ""
            }
            open.append(HomeWidgets.Issue(
                id: issue.id,
                identifier: issue.identifier ?? "",
                title: issue.title,
                statusId: status.id,
                boardId: issue.boardId,
                teamId: teamId,
                priority: issue.priority,
                sortOrder: issue.sortOrder ?? 0,
                assigneeId: issue.assigneeId,
                updatedAt: issue.updatedAt
            ))
        }
        return Lists(
            teams: teams.map { HomeWidgets.Snapshot.Team(id: $0.id, name: $0.name) },
            boards: boards.map { board in
                HomeWidgets.Snapshot.Board(
                    id: board.id,
                    teamId: board.teamId,
                    name: board.name,
                    glyph: AppIcons.assetName(BoardTypeDisplay.iconName(for: board)) ?? "",
                    color: board.color,
                    sortOrder: board.sortOrder ?? 0
                )
            },
            statuses: statuses.values.sorted { $0.id < $1.id },
            issues: open,
            statusGlyphs: glyphs,
            sealedRootIds: sealedRootIds
        )
    }

    /// The status colour as the issue rows draw it (builtins = the tokens).
    nonisolated private static func hex(_ color: Color) -> String {
        var red: CGFloat = 0, green: CGFloat = 0, blue: CGFloat = 0, alpha: CGFloat = 0
        UIColor(color).getRed(&red, green: &green, blue: &blue, alpha: &alpha)
        func byte(_ value: CGFloat) -> Int { Int((min(max(value, 0), 1) * 255).rounded()) }
        return String(format: "#%02X%02X%02X", byte(red), byte(green), byte(blue))
    }
}

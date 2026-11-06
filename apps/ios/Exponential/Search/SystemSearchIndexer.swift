import CoreSpotlight
import ExpCore
import Foundation
import GRDB
import OSLog
import UniformTypeIdentifiers

private let logger = Logger(subsystem: "at.exponential", category: "SystemSearch")

/// #EXP-1642 (system-search.json): every signed-in account's issues and plain
/// chats in Spotlight, one domain per account (`account:<id>`), from the
/// account's own synced store (`SystemSearch.items`, production caps).
/// Re-indexed at most once per `debounceMs` after the store changes: the
/// current items upserted, the ids that left deleted. A signed-out account's
/// domain is deleted (and nothing else). Never a description, comment or
/// message; never an encrypted chat. A result opens through
/// `SystemSearchIndexer.open` (the app's `CSSearchableItemActionType`).
@MainActor
final class SystemSearchIndexer {
    /// Accounts whose domain the index may hold (persisted: a sign-out while
    /// the app was not running still deletes its domain on the next launch).
    private static let accountsKey = "system_search_accounts_v1"

    private let auth: AuthRepository
    private let db: DatabaseManager
    private let index: CSSearchableIndex
    private var tasks: [String: Task<Void, Never>] = [:]
    /// The item ids each domain holds (this process; the first pass of a
    /// domain rewrites it whole).
    private var indexed: [String: Set<String>] = [:]

    init(auth: AuthRepository, db: DatabaseManager, index: CSSearchableIndex = .default()) {
        self.auth = auth
        self.db = db
        self.index = index
    }

    /// Reconcile with `auth.accounts` (call when they may have changed).
    func refresh() {
        guard CSSearchableIndex.isIndexingAvailable() else { return }
        let signedIn = auth.accounts.filter { $0.token != nil }
        let desired = Set(signedIn.map(\.id))
        let defaults = UserDefaults.standard
        let known = Set(defaults.stringArray(forKey: Self.accountsKey) ?? []).union(tasks.keys)
        for gone in known.subtracting(desired) {
            tasks[gone]?.cancel()
            tasks[gone] = nil
            indexed[gone] = nil
            index.deleteSearchableItems(withDomainIdentifiers: [SystemSearch.domain(accountId: gone)]) { error in
                if let error { logger.error("Spotlight domain delete failed: \(error.localizedDescription)") }
            }
        }
        defaults.set(desired.sorted(), forKey: Self.accountsKey)
        for account in signedIn where tasks[account.id] == nil {
            start(accountId: account.id, userId: account.userId)
        }
    }

    // MARK: - Opening a result

    /// A Spotlight item's unique id: the domain, then the item id (unique
    /// across accounts holding the same issue).
    nonisolated static func uniqueIdentifier(accountId: String, itemId: String) -> String {
        "\(SystemSearch.domain(accountId: accountId))/\(itemId)"
    }

    /// Routes an opened result like a push tap, under its domain's account:
    /// an issue opens the issue page, a chat the session. Anything that does
    /// not parse (or a signed-out account) leaves the normal start screen.
    static func open(_ activity: NSUserActivity, deps: AppDependencies) {
        guard let unique = activity.userInfo?[CSSearchableItemActivityIdentifier] as? String,
              let slash = unique.firstIndex(of: "/"),
              let accountId = SystemSearch.accountId(domain: String(unique[..<slash])),
              let account = deps.auth.accounts.first(where: { $0.id == accountId && $0.token != nil }),
              let target = SystemSearch.parse(String(unique[unique.index(after: slash)...]))
        else { return }
        switch target {
        case .issue(let issueId):
            deps.deepLinkBus.navigateToIssue(issueId, accountId: account.id)
        case .chat(let sessionId):
            deps.deepLinkBus.navigateToSession(sessionId, userId: account.userId)
        }
    }

    // MARK: - Private

    private func start(accountId: String, userId: String?) {
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in
            try Self.read(db, userId: userId)
        }.removeDuplicates()
        let debounce = Duration.milliseconds(SystemSearch.debounceMs)
        tasks[accountId] = Task { @MainActor [weak self] in
            do {
                // The newest snapshot only: a pass at most once per debounce.
                let values = observation.values(in: pool, bufferingPolicy: .bufferingNewest(1))
                for try await items in values {
                    try await Task.sleep(for: debounce)
                    guard let self else { return }
                    await self.write(accountId: accountId, items: items)
                }
            } catch is CancellationError {
            } catch {
                logger.error("Spotlight stream ended: \(error.localizedDescription)")
            }
        }
    }

    private func write(accountId: String, items: [SystemSearch.Item]) async {
        let domain = SystemSearch.domain(accountId: accountId)
        let next = Set(items.map(\.id))
        do {
            if let previous = indexed[accountId] {
                let left = previous.subtracting(next)
                if !left.isEmpty {
                    try await index.deleteSearchableItems(
                        withIdentifiers: left.map { Self.uniqueIdentifier(accountId: accountId, itemId: $0) }
                    )
                }
            } else {
                // First pass in this process: whatever an earlier run left goes.
                try await index.deleteSearchableItems(withDomainIdentifiers: [domain])
            }
            // Cancelled meanwhile (signed out): its domain is already gone.
            guard !Task.isCancelled else { return }
            let searchable = items.map { item -> CSSearchableItem in
                let attributes = CSSearchableItemAttributeSet(contentType: .content)
                attributes.title = item.title
                attributes.contentDescription = item.subtitle
                attributes.keywords = item.keywords
                return CSSearchableItem(
                    uniqueIdentifier: Self.uniqueIdentifier(accountId: accountId, itemId: item.id),
                    domainIdentifier: domain,
                    attributeSet: attributes
                )
            }
            if !searchable.isEmpty { try await index.indexSearchableItems(searchable) }
            indexed[accountId] = next
        } catch {
            // Best effort: the next store change retries; titles never reach the log.
            logger.error("Spotlight index failed: \(error.localizedDescription)")
        }
    }

    nonisolated private static func read(_ db: Database, userId: String?) throws -> [SystemSearch.Item] {
        let caps = SystemSearch.productionCaps
        let boards = try BoardEntity.fetchAll(db).map { SystemSearch.Board(id: $0.id, name: $0.name) }
        // The store holds live issues only (the shapes leave trash and
        // archive out); an issue without an identifier yet is not findable.
        let issues = try IssueEntity.fetchAll(db, sql: """
            SELECT * FROM "issues" ORDER BY "updated_at" DESC LIMIT ?
            """, arguments: [caps.maxIssues]).compactMap { issue -> SystemSearch.Issue? in
            guard let identifier = issue.identifier, !identifier.isEmpty else { return nil }
            return SystemSearch.Issue(
                id: issue.id, identifier: identifier, title: issue.title, boardId: issue.boardId,
                updatedAt: issue.updatedAt
            )
        }
        // The share-targets filter: plain roots, never an agent's
        // conversation, an app's editor or an encrypted chat.
        let roots = try SessionEntity.fetchAll(db, sql: """
            SELECT * FROM "sessions"
            WHERE "parent_id" IS NULL AND "agent_id" IS NULL AND "app_id" IS NULL
              AND "seal_public_key" IS NULL AND "sealed_title" IS NULL
            ORDER BY "updated_at" DESC LIMIT ?
            """, arguments: [caps.maxChats])
        let chats = roots
            .filter { userId == nil || $0.userId == userId || $0.shared }
            .map { root in
                let title = (root.title ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
                return ShareTargets.Chat(
                    id: root.id,
                    title: title.isEmpty ? SessionTitle.Copy.untitled : title,
                    updatedAt: root.updatedAt,
                    agentId: root.agentId,
                    sealed: root.sealPublicKey != nil || root.sealedTitle != nil
                )
            }
        return SystemSearch.items(boards: boards, issues: issues, chats: chats, caps: caps)
    }
}

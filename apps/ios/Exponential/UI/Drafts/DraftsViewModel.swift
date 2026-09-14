import ExpCore
import ExpUI
import Foundation
import GRDB

/// EXP-878 — the account's issue drafts, ACCOUNT-WIDE (every team; each row
/// carries its board's name). One `ValueObservation` over
/// `IssueDraftQueries.resolved`, which joins each row to its board and its
/// team's statuses and drops anything that does not resolve locally — the
/// `issue_drafts` shape is per-user and never trash-scoped, so a draft can
/// outlive the board it names.
@MainActor @Observable
final class DraftsViewModel {
    var rows: [IssueDraftRow] = []
    var error: String?

    private let accountId: String
    private let db: DatabaseManager
    private let api: IssueDraftsApi
    private var task: Task<Void, Never>?

    init(accountId: String, db: DatabaseManager, api: IssueDraftsApi) {
        self.accountId = accountId
        self.db = db
        self.api = api
    }

    func startObserving() {
        stopObserving() // restartable: the view re-arms on every appear
        guard let pool = try? db.pool(forAccountId: accountId) else { return }

        // Resolution (board + statuses) happens INSIDE the observation, so the
        // list re-renders when a board or a team's statuses change too, not
        // only when a draft row does.
        let observation = ValueObservation.tracking { db in
            try IssueDraftQueries.resolved(db: db)
        }
        task = Task { [weak self] in
            do {
                for try await rows in observation.values(in: pool) {
                    self?.rows = rows
                }
            } catch {}
        }
    }

    func stopObserving() {
        task?.cancel()
        task = nil
    }

    /// Delete a draft: the server row first (it owns the attachments), then
    /// the local mirror so the row leaves the list without waiting for sync.
    func delete(_ row: IssueDraftRow) async {
        let id = row.draft.id
        do {
            try await api.delete(accountId: accountId, id: id)
        } catch {
            self.error = error.userFacingMessage
            return
        }
        error = nil
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        _ = try? await pool.write { db in try IssueDraftEntity.deleteOne(db, key: id) }
    }
}

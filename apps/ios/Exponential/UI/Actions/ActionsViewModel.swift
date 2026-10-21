import ExpCore
import Foundation
import GRDB

/// Backs the Actions list (EXP-253): the action prompts of EVERY member team
/// (EXP-1186, cross-team like the Inbox; the view groups them per team) LIVE
/// from the synced local store (EXP-268 — the actions shape, minus `body`,
/// which nothing here needs). EXP-825: running an action is NAVIGATION — the
/// composer owns the send — so no start plumbing lives here.
@MainActor @Observable
final class ActionsViewModel {

    var actions: [ActionDto] = []
    var isLoading = false

    private let accountId: String
    private let db: DatabaseManager

    private var loadedTeamId: String?
    private var actionsObservationTask: Task<Void, Never>?

    init(
        accountId: String,
        db: DatabaseManager
    ) {
        self.accountId = accountId
        self.db = db
    }

    /// Observe every member team's synced actions (EXP-268: the local GRDB
    /// store, not tRPC — the list stays live as sync lands rows; EXP-1186:
    /// the synced set IS the member teams'). Real rows sort
    /// sortOrder-then-name like the server list did; the view splits them
    /// per team. `teamId` = the ACTIVE team: a team switch re-arms the
    /// observation.
    func load(teamId: String) async {
        loadedTeamId = teamId
        if actions.isEmpty { isLoading = true }
        actionsObservationTask?.cancel()
        guard let pool = try? db.pool(forAccountId: accountId) else {
            // FEED-79: the list loads on its own, so a failure is quiet: the
            // empty state, never a toast.
            isLoading = false
            return
        }
        // NEW-2: the team's saved PROMPTS (the `actions` shape is served
        // empty; NEW-7 moves this page to Settings → Prompts).
        let observation = ValueObservation.tracking { db in
            try PromptEntity.fetchAll(db)
        }
        actionsObservationTask = Task { [weak self] in
            do {
                for try await rows in observation.values(in: pool) {
                    guard let self, !Task.isCancelled else { return }
                    guard self.loadedTeamId == teamId else { return }
                    let dtos = rows
                        .sorted { ($0.sortOrder, $0.name) < ($1.sortOrder, $1.name) }
                        .map { ActionDto(prompt: $0) }
                    self.actions = dtos
                    self.isLoading = false
                }
            } catch {
                guard let self, !Task.isCancelled, self.loadedTeamId == teamId else { return }
                // Quiet (FEED-79): the empty state.
                self.isLoading = false
            }
        }
    }
}

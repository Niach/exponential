import ExpCore
import Foundation
import GRDB

/// EXP-981 — the Workflows list: the active team's workflows LIVE off the
/// synced store (the 23rd Electric shape), banded Running / Draft / Done by the
/// shared rule and newest first inside a band. Reads only; every write lives on
/// the detail.
@MainActor @Observable
final class WorkflowsViewModel {

    /// One band and the rows under it. Empty bands never render.
    struct Band: Identifiable {
        let key: WorkflowView.Band
        let title: String
        let rows: [WorkflowEntity]
        var id: String { key.rawValue }
    }

    private(set) var workflows: [WorkflowEntity] = []
    /// EXP-1086: the workflows with an open question — the row's red dot.
    private(set) var asking: Set<String> = []
    /// True until the first emission — an empty list means "none", not
    /// "still loading".
    private(set) var isLoading = true

    private let accountId: String
    private let db: DatabaseManager
    private var loadedTeamId: String?
    /// Whether `observe` has run at all (see its guard).
    private var observed = false
    private var observationTask: Task<Void, Never>?

    init(accountId: String, db: DatabaseManager) {
        self.accountId = accountId
        self.db = db
    }

    /// The three bands in the rule's order, each newest first.
    var bands: [Band] {
        WorkflowView.bands.compactMap { band in
            let rows = workflows.filter { WorkflowView.band($0.status) == band.key }
            return rows.isEmpty
                ? nil
                : Band(key: band.key, title: band.title, rows: rows)
        }
    }

    func observe(teamId: String?) {
        // `observed` rather than a nil check on `loadedTeamId`: with no active
        // team the first call must still settle the loading flag, or the list
        // spins forever over an empty store.
        guard !observed || loadedTeamId != teamId else { return }
        observed = true
        loadedTeamId = teamId
        observationTask?.cancel()
        workflows = []
        isLoading = true
        guard let teamId, let pool = try? db.pool(forAccountId: accountId) else {
            isLoading = false
            return
        }
        let observation = ValueObservation.tracking { db -> ([WorkflowEntity], [CodingSessionEntity]) in
            let rows = try WorkflowEntity.filter(Column("team_id") == teamId).fetchAll(db)
            let sessions = try CodingSessionEntity
                .filter(rows.map(\.id).contains(Column("workflow_id")))
                .fetchAll(db)
            return (rows, sessions)
        }
        observationTask = Task { [weak self] in
            do {
                for try await (rows, sessions) in observation.values(in: pool) {
                    guard let self, !Task.isCancelled, self.loadedTeamId == teamId else { return }
                    // Newest first inside a band; the id breaks ties so two
                    // workflows created in the same millisecond keep a stable
                    // order.
                    self.workflows = rows.sorted {
                        ($0.createdAt, $0.id) > ($1.createdAt, $1.id)
                    }
                    self.asking = Set(rows.map(\.id).filter {
                        !WorkflowQuestions.open(sessions, workflowId: $0).isEmpty
                    })
                    self.isLoading = false
                }
            } catch {
                // Non-fatal — the list simply stays as it was.
                self?.isLoading = false
            }
        }
    }

    func stop() {
        observationTask?.cancel()
        observationTask = nil
        loadedTeamId = nil
        observed = false
    }
}

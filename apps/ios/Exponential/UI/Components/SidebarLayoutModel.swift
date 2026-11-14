import ExpCore
import Foundation
import GRDB

/// #EXP-1832 (sidebar-folders.json): my synced `sidebar_layout` entries of
/// one team's section (`board` | `app`), live off GRDB. Phones only READ
/// them: the board picker and the Apps list fold their rows into the
/// folders the member arranged on the desktop IDE or the web.
@MainActor
@Observable
final class SidebarLayoutModel {
    private(set) var entries: [SidebarFolders.Entry] = []

    @ObservationIgnored private var task: Task<Void, Never>?
    @ObservationIgnored private var boundKey = ""

    nonisolated init() {}

    func bind(accountId: String, teamId: String?, section: String, db: DatabaseManager) {
        let key = "\(accountId)|\(teamId ?? "")|\(section)"
        guard key != boundKey else { return }
        stop()
        boundKey = key
        guard let teamId, let pool = try? db.pool(forAccountId: accountId) else {
            entries = []
            return
        }
        let observation = ValueObservation.tracking { db in
            try SidebarLayoutEntity
                .filter(Column("team_id") == teamId)
                .filter(Column("section") == section)
                .fetchAll(db)
        }
        task = Task { [weak self] in
            do {
                for try await rows in observation.values(in: pool) {
                    self?.entries = rows.compactMap(\.entry)
                }
            } catch {}
        }
    }

    func stop() {
        task?.cancel()
        task = nil
        boundKey = ""
    }
}

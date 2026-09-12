import ExpCore
import GRDB
import SwiftUI

// EXP-778: personal pins on mobile. The `pins` shape is per-user and never
// team-scoped, so the reader here filters on the team it renders for and
// resolves the rows locally.
//
// EXP-858: the phone has NO pin CONTROLS any more — a pin lands in a sidebar
// and mobile has none, so `PinStore` / `PinMenuItem` / `PinToolbarButton` and
// the `pins.toggle` client are gone. What is left is read-only: pins made on
// the desktop (or web at md and up) still render in the board switcher.

/// The active team's pins RESOLVED against the local store, in display
/// order — the board switcher's Pinned section. Rows whose target has not
/// synced (or is gone) are hidden, never shown as dead rows.
@MainActor @Observable
final class PinnedItemsModel {
    private(set) var items: [PinnedItem] = []
    private var observation: Task<Void, Never>?

    func observe(accountId: String, teamId: String, db: DatabaseManager) {
        observation?.cancel()
        items = []
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        // Tracks every table the resolver reads, so a title edit or a
        // session ending re-renders the pinned row too.
        let tracked = ValueObservation.tracking { db in
            try PinQueries.resolved(db: db, teamId: teamId)
        }
        observation = Task { [weak self] in
            do {
                for try await rows in tracked.values(in: pool) {
                    guard let self, !Task.isCancelled else { return }
                    self.items = rows
                }
            } catch {
                // Non-fatal: the section simply stays as it was.
            }
        }
    }

    func stop() {
        observation?.cancel()
        observation = nil
    }
}

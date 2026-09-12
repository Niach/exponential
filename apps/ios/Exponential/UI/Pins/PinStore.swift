import ExpCore
import ExpUI
import GRDB
import SwiftUI

// EXP-778: personal pins on mobile. The `pins` shape is per-user and never
// team-scoped, so every reader here filters on the team it renders for and
// reads pinned-ness from the LOCAL table — `pins.toggle` is fire-and-forget
// and the row's arrival (or removal) over sync is what flips the control.

/// The caller's pins of ONE kind in ONE team, as the set of target ids —
/// what a Pin/Unpin control reads. One store per surface (the issue detail,
/// the steering screen, the actions list), never one per row.
@MainActor @Observable
final class PinStore {
    private(set) var pinnedIds: Set<String> = []

    private let accountId: String
    private let teamId: String
    private let kind: String
    private let db: DatabaseManager
    private let api: PinsApi
    private var observation: Task<Void, Never>?

    init(accountId: String, teamId: String, kind: String, db: DatabaseManager, api: PinsApi) {
        self.accountId = accountId
        self.teamId = teamId
        self.kind = kind
        self.db = db
        self.api = api
    }

    func isPinned(_ targetId: String) -> Bool {
        pinnedIds.contains(targetId)
    }

    /// Observe the local pins table; stays live until `stop()`.
    func start() {
        guard observation == nil else { return }
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let teamId = teamId
        let kind = kind
        let tracked = ValueObservation.tracking { db in
            try PinEntity
                .filter(Column("team_id") == teamId)
                .filter(Column("kind") == kind)
                .fetchAll(db)
                .compactMap { $0.targetId }
        }
        observation = Task { [weak self] in
            do {
                for try await ids in tracked.values(in: pool) {
                    guard let self, !Task.isCancelled else { return }
                    self.pinnedIds = Set(ids)
                }
            } catch {
                // Non-fatal: the control keeps its last state.
            }
        }
    }

    func stop() {
        observation?.cancel()
        observation = nil
    }

    /// Flip the pin. Optimistic locally (the menu closes on tap, so the
    /// next open must already read right); the synced row settles it. A
    /// failed call puts the flip back, so the control does not read wrong
    /// until the next table emission.
    func toggle(_ targetId: String) {
        let wasPinned = pinnedIds.contains(targetId)
        if wasPinned {
            pinnedIds.remove(targetId)
        } else {
            pinnedIds.insert(targetId)
        }
        let api = api
        let accountId = accountId
        let teamId = teamId
        let kind = kind
        Task { [weak self] in
            do {
                _ = try await api.toggle(
                    accountId: accountId, teamId: teamId, kind: kind, targetId: targetId
                )
            } catch {
                // Non-fatal: restore the pre-tap state (idempotent against a
                // table emission that landed meanwhile).
                guard let self else { return }
                if wasPinned {
                    self.pinnedIds.insert(targetId)
                } else {
                    self.pinnedIds.remove(targetId)
                }
            }
        }
    }
}

/// The Pin / Unpin `…` menu row, one look on every surface (the sidebar's
/// Pinned section is where the row then appears).
struct PinMenuItem: View {
    let store: PinStore
    let targetId: String

    var body: some View {
        let pinned = store.isPinned(targetId)
        GlassMenuItem(pinned ? "Unpin" : "Pin", icon: pinned ? AppIcons.uiUnpin : AppIcons.uiPin) {
            store.toggle(targetId)
        }
    }
}

/// EXP-845: the pin BUTTON beside a screen's `…` — the issue detail's and the
/// steering header's trailing control, the twin of web's `PinToggleButton`.
/// Bare glyph in a 32pt frame, like every other toolbar-hosted control here
/// (iOS 26 draws its own capsule around a bar item, so ours would read as two
/// rings). A pinned target takes the `ui-unpin` glyph at full white, so the
/// state reads without a label.
///
/// Deliberately NOT on list rows (EXP-845 decision): pins are set from the
/// thing itself — its header or its `…` — exactly as issues are.
struct PinToolbarButton: View {
    let store: PinStore
    let targetId: String

    var body: some View {
        let pinned = store.isPinned(targetId)
        Button {
            store.toggle(targetId)
        } label: {
            AppIcon(
                pinned ? AppIcons.uiUnpin : AppIcons.uiPin,
                size: AppIcon.Size.medium,
                weight: .medium
            )
            .foregroundStyle(.white.opacity(pinned ? TextOpacity.primary : TextOpacity.secondary))
            .frame(width: GlassTokens.controlSize, height: GlassTokens.controlSize)
            .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(pinned ? "Unpin" : "Pin")
        .accessibilityIdentifier("pin-toggle")
    }
}

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

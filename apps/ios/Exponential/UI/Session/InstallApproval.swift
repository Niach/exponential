import ExpCore
import ExpUI
import GRDB
import SwiftUI

// #EXP-1950 (fixtures/install-trust.json, decisions a1 + b1; web
// `components/devices/install-approval.tsx`): the approval of a new install
// and a team-mate's reset, drawn ONLY from core genUI parts
// (`InstallApprovalView`: Section, Row, Badge, Button, EmptyState) through
// `CiSurface`, so the four platforms paint the same tree. Rows: Needs you,
// the phone's Devices too. Only rows `InstallTrust.reconcile` yields, never
// the server's word. A failed action = the toast (feedback-states.json).

/// This install's trust per account, for the views (the keeper's snapshot,
/// re-read whenever it posts `installTrustChanged`).
@MainActor
@Observable
final class InstallTrustModel {
    private(set) var snapshots: [String: InstallTrustSnapshot] = [:]
    @ObservationIgnored private let keeper: InstallTrustKeeper?
    @ObservationIgnored private var observer: NSObjectProtocol?

    init(keeper: InstallTrustKeeper?) {
        self.keeper = keeper
        observer = NotificationCenter.default.addObserver(forName: .installTrustChanged, object: nil, queue: .main) { [weak self] note in
            guard let accountId = note.userInfo?["accountId"] as? String else { return }
            Task { @MainActor in await self?.refresh(accountId) }
        }
    }

    func snapshot(_ accountId: String) -> InstallTrustSnapshot { snapshots[accountId] ?? .none }

    func refresh(_ accountId: String) async {
        guard let keeper else { return }
        let next = await keeper.snapshot(accountId: accountId)
        if snapshots[accountId] != next { snapshots[accountId] = next }
    }

    func approve(_ row: TrustRow, accountId: String) async throws {
        try await keeper?.approve(accountId: accountId, row: row)
        await refresh(accountId)
    }

    func remove(_ row: TrustRow, accountId: String) async throws {
        try await keeper?.remove(accountId: accountId, row: row)
        await refresh(accountId)
    }
}

/// The band of installs waiting for this install's approval (nothing when none).
struct PendingInstalls: View {
    let rows: [TrustRow]
    /// A team-mate's name by user id (a Reset row's title leads with it).
    var memberName: (String) -> String? = { _ in nil }
    let onApprove: (TrustRow) -> Void
    let onRemove: (TrustRow) -> Void

    var body: some View {
        if !rows.isEmpty {
            CiSurface(
                surfaceId: InstallApprovalView.TestIds.pending,
                tree: InstallApprovalView.pendingTree(rows.map { ($0, InstallApprovalView.title($0, memberName: memberName($0.userId))) })
            ) { name, context, _ in
                guard case let .string(id)? = context?["id"], let row = rows.first(where: { $0.installId == id }) else { return }
                switch name {
                case InstallApprovalView.Events.approve: onApprove(row)
                case InstallApprovalView.Events.remove: onRemove(row)
                default: break
                }
            }
            .frame(maxWidth: .infinity)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(InstallApprovalView.TestIds.pending)
        }
    }
}

/// This install, not approved yet: THE page EmptyState with its own code.
struct InstallWaiting: View {
    let code: String

    var body: some View {
        CiSurface(surfaceId: InstallApprovalView.TestIds.waiting, tree: InstallApprovalView.waitingTree(code: code)) { _, _, _ in }
            .frame(maxWidth: .infinity)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(InstallApprovalView.TestIds.waiting)
    }
}

/// Where an encrypted chat would open on an install that waits for approval
/// and holds no key of it: the waiting state instead of locked rows.
struct InstallWaitingGate: View {
    let rootId: String
    let sealed: Bool
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId

    var body: some View {
        let trust = deps.installTrust.snapshot(accountId)
        if sealed, trust.waiting, let code = trust.code, deps.treeKeyHolder.held(rootId: rootId).isEmpty {
            ZStack {
                AppBackground()
                InstallWaiting(code: code)
            }
            .task(id: accountId) { await deps.installTrust.refresh(accountId) }
        }
    }
}

/// The live band: this account's rows, Approve / Remove through the keeper.
struct LivePendingInstalls: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @State private var names: [String: String] = [:]

    var body: some View {
        let rows = deps.installTrust.snapshot(accountId).rows
        PendingInstalls(
            rows: rows,
            memberName: { names[$0] },
            onApprove: { row in run(verb: "approve") { try await deps.installTrust.approve(row, accountId: accountId) } },
            onRemove: { row in run(verb: "remove") { try await deps.installTrust.remove(row, accountId: accountId) } }
        )
        .task(id: accountId) { await deps.installTrust.refresh(accountId) }
        .task(id: rows.map(\.userId)) { names = await loadNames(rows.map(\.userId)) }
    }

    private func run(verb: String, _ body: @escaping () async throws -> Void) {
        Task {
            do {
                try await body()
            } catch {
                toaster.error(FeedbackStates.Errors.title(verb: verb, thing: "install"), description: error.localizedDescription)
            }
        }
    }

    private func loadNames(_ userIds: [String]) async -> [String: String] {
        guard !userIds.isEmpty, let pool = try? deps.db.pool(forAccountId: accountId) else { return [:] }
        let users = (try? await pool.read { db in try UserEntity.filter(keys: userIds).fetchAll(db) }) ?? []
        return Dictionary(users.compactMap { user in user.name.map { (user.id, $0) } }, uniquingKeysWith: { first, _ in first })
    }
}

/// #EXP-1950 capture-only specimens (`-uiTestingStyleguide install-approval
/// | install-waiting`, shots sg_install-approval / sg_install-waiting; web
/// `?specimen=`): the fixture's keys N (New install) and T3 (Reset), and N
/// waiting with its code, over THE Needs you page content.
struct InstallTrustSpecimens: View {
    let waiting: Bool

    private static let rows = InstallTrustSpecimenRows.rows

    var body: some View {
        ZStack {
            AppBackground()
            NeedsYouPageContent(
                rows: [],
                waitingCode: waiting ? Self.rows[0].code : nil,
                pendingInstalls: waiting ? 0 : Self.rows.count,
                onOpen: { _ in }
            ) {
                PendingInstalls(rows: Self.rows, onApprove: { _ in }, onRemove: { _ in })
            }
        }
    }
}

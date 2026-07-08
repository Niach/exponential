import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// The feedback board's project, resolved after the gate clears, handed back
/// to the caller for an in-app push.
struct FeedbackBoardTarget: Hashable, Identifiable {
    let accountId: String
    let projectId: String
    var id: String { "\(accountId)/\(projectId)" }
}

/// In-app opener for the public feedback board. Authed sync is membership-only,
/// so a signed-in member's board syncs locally and opens in-app; a non-member
/// can't sync it (public boards are read-only for non-members and there is no
/// join flow anymore), so they hand off to the web board at `/feedback`, which
/// serves the anonymous public view. This sheet resolves the board via
/// `workspaces.getBySlug` and routes accordingly; anything unresolvable in-app
/// (self-hosted instance without a public board, sync timeout) also falls back
/// to the browser handoff.
struct FeedbackBoardGateSheet: View {
    let onOpenBoard: (FeedbackBoardTarget) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss

    private enum Phase: Equatable {
        case resolving
        case failed(String)
    }

    @State private var phase: Phase = .resolving

    // Slugs seeded by bootstrap-cloud.ts — the same constants the web
    // `/feedback` redirect hardcodes (workspace `feedback`, its single
    // project `exponential`).
    private static let feedbackWorkspaceSlug = "feedback"
    private static let feedbackProjectSlug = "exponential"

    var body: some View {
        VStack(spacing: 16) {
            switch phase {
            case .resolving:
                progress("Opening feedback board…")
            case let .failed(message):
                failed(message)
            }
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .task { await resolve() }
    }

    // MARK: - Phases

    private func progress(_ label: String) -> some View {
        VStack(spacing: 12) {
            ProgressView().tint(.white)
            Text(label)
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        }
    }

    private func failed(_ message: String) -> some View {
        VStack(spacing: 16) {
            Image(systemName: "exclamationmark.triangle")
                .font(.system(size: 36))
                .foregroundStyle(.orange.opacity(0.85))

            Text(message)
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .multilineTextAlignment(.center)

            primaryButton("Open in browser") { fallbackToBrowser() }

            Button("Cancel") { dismiss() }
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        }
    }

    private func primaryButton(_ title: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(title)
                .font(.body.weight(.medium))
                .foregroundStyle(.white)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 14)
        }
        .background(Color.white.opacity(0.15))
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(Color.white.opacity(0.1), lineWidth: 0.5)
        )
    }

    // MARK: - Flow

    private func resolve() async {
        guard let accountId = deps.auth.activeAccountId else {
            fallbackToBrowser()
            return
        }
        do {
            let workspace = try await deps.workspacesApi.getBySlug(
                accountId: accountId, slug: Self.feedbackWorkspaceSlug
            )
            if workspace.membership != nil {
                // A member — the board syncs normally, so open it in-app.
                await openBoard(accountId: accountId, workspaceId: workspace.id, timeout: 5)
            } else {
                // Non-member: the board is read-only for them and never syncs
                // locally (membership-only sync). The web board serves the
                // anonymous public view, so hand off to the browser.
                fallbackToBrowser()
            }
        } catch {
            // NOT_FOUND (no public board on this instance) or the endpoint is
            // unreachable — keep the browser handoff as the safety net.
            fallbackToBrowser()
        }
    }

    /// Wait for the board's project to land in the local cache, then hand it
    /// to the caller. The feedback workspace holds a single project (slug
    /// `exponential`); prefer it, tolerate anything else. Timing out falls
    /// back to the browser — after a successful join the web board works.
    private func openBoard(accountId: String, workspaceId: String, timeout: TimeInterval) async {
        let start = Date()
        while Date().timeIntervalSince(start) < timeout {
            if let project = await fetchBoardProject(accountId: accountId, workspaceId: workspaceId) {
                onOpenBoard(FeedbackBoardTarget(accountId: accountId, projectId: project.id))
                return
            }
            try? await Task.sleep(for: .milliseconds(300))
        }
        fallbackToBrowser()
    }

    private func fetchBoardProject(accountId: String, workspaceId: String) async -> ProjectEntity? {
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return nil }
        let projects = (try? await pool.read { db in
            try ProjectEntity
                .filter(Column("workspace_id") == workspaceId)
                .filter(Column("archived_at") == nil)
                .fetchAll(db)
        }) ?? []
        return projects.first { $0.slug == Self.feedbackProjectSlug } ?? projects.first
    }

    /// The pre-existing handoff: the web `/feedback` route redirects to the
    /// board (and web shows its own join gate to non-members).
    private func fallbackToBrowser() {
        if let baseUrl = deps.auth.instanceUrl, let url = URL(string: "\(baseUrl)/feedback") {
            UIApplication.shared.open(url)
        }
        dismiss()
    }
}

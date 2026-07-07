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

/// In-app join gate for the public feedback board — the iOS analog of web's
/// `WorkspaceJoinGate` at `/w/feedback`. Authed sync is membership-only, so a
/// signed-in non-member can't see the board at all until they join: this
/// sheet resolves it via `workspaces.getBySlug`, offers the self-service
/// `workspaceMembers.join` (public workspaces only), restarts the shape
/// pipeline so the new membership's where clauses take effect immediately,
/// then hands the board's single project to the caller for an in-app push.
/// Anything that can't be resolved in-app (self-hosted instance without a
/// public board, join rejected, sync timeout) falls back to the pre-existing
/// browser handoff to `/feedback`.
struct FeedbackBoardGateSheet: View {
    let onOpenBoard: (FeedbackBoardTarget) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss

    private enum Phase: Equatable {
        case resolving
        case joinGate(workspaceId: String, workspaceName: String)
        case joining
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
            case let .joinGate(workspaceId, workspaceName):
                joinGate(workspaceId: workspaceId, workspaceName: workspaceName)
            case .joining:
                progress("Joining…")
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

    private func joinGate(workspaceId: String, workspaceName: String) -> some View {
        VStack(spacing: 16) {
            Image(systemName: "person.2")
                .font(.system(size: 36))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))

            Text("Join \(workspaceName)")
                .font(.title3.weight(.bold))
                .foregroundStyle(.white)
                .multilineTextAlignment(.center)

            Text("This is a public board. Join it to browse issues, follow discussions and share feedback. You can leave again anytime.")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .multilineTextAlignment(.center)

            primaryButton("Join board") {
                Task { await join(workspaceId: workspaceId) }
            }

            Button("Not now") { dismiss() }
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
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
                // Already a member — the board syncs normally, its project is
                // (or is about to be) in the local cache.
                await openBoard(accountId: accountId, workspaceId: workspace.id, timeout: 5)
            } else if workspace.isPublic {
                phase = .joinGate(workspaceId: workspace.id, workspaceName: workspace.name)
            } else {
                // Not public and not a member — nothing joinable in-app.
                fallbackToBrowser()
            }
        } catch {
            // NOT_FOUND (no public board on this instance) or the endpoint is
            // unreachable — keep the old browser handoff as the safety net.
            fallbackToBrowser()
        }
    }

    private func join(workspaceId: String) async {
        guard let accountId = deps.auth.activeAccountId else {
            fallbackToBrowser()
            return
        }
        phase = .joining
        do {
            try await deps.workspaceMembersApi.join(accountId: accountId, workspaceId: workspaceId)
            // The membership row rotates every shape's server-side where
            // clause; restart the pipeline so in-flight long-polls on the old
            // scope don't hold the board back for up to a minute (the iOS
            // analog of the web join gate's hard reload).
            await deps.syncManager.restartPipeline(accountId: accountId)
            await openBoard(accountId: accountId, workspaceId: workspaceId, timeout: 15)
        } catch {
            phase = .failed(error.trpcUserMessage)
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

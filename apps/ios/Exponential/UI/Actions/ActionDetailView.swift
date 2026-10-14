import ExpCore
import ExpUI
import SwiftUI

/// The ACTION PAGE — pushed from an Actions row (and from an `action` entity
/// ref). EXP-1291: an action is a saved prompt, so the page is its editor
/// (`ActionPromptTab`), read-only for non-owners, under the nav bar (back ·
/// the action's icon + name · Run). Run opens the Home composer with the
/// action picked.
struct ActionDetailView: View {
    let actionId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @State private var viewModel: ActionDetailViewModel?

    private var action: ActionDto? { viewModel?.action }
    private var isOwner: Bool { viewModel?.permissions.isOwner == true }

    var body: some View {
        ZStack {
            AppBackground()

            if let action {
                ActionPromptTab(action: action, canEdit: isOwner)
                    .environment(\.accountId, accountId)
                    // One editor per action: a different row starts clean.
                    .id(action.id)
            } else if viewModel?.resolved == true {
                EmptyState(.actionUnavailable)
                    .frame(maxHeight: .infinity)
            } else {
                // EXP-1275 (D8): a first load is skeleton rows.
                SkeletonRows()
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            }
        }
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .toolbar {
            ToolbarItem(placement: .principal) { title }
            ToolbarItem(placement: .topBarTrailing) { runButton }
        }
        .task(id: actionId) {
            ensureViewModel()
            viewModel?.load(actionId: actionId)
        }
        .onAppear { ensureViewModel() }
    }

    private func ensureViewModel() {
        if viewModel == nil {
            viewModel = ActionDetailViewModel(accountId: accountId, db: deps.db, auth: deps.auth)
        }
    }

    /// The action's icon + name — the bar's title.
    @ViewBuilder
    private var title: some View {
        if let action {
            HStack(spacing: 6) {
                AppIcon(ActionIconDisplay.iconName(for: action.icon), size: AppIcon.Size.small)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text(action.name)
                    .font(.headline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("action-title")
        }
    }

    /// The existing run flow (EXP-825): the composer with this action picked.
    /// Bare content — the bar owns the capsule (EXP-698 r4).
    @ViewBuilder
    private var runButton: some View {
        if let action {
            Button {
                pushRoute(.agent(
                    accountId: accountId,
                    seed: AgentComposerSeed(actionId: action.id, teamId: action.teamId)
                ))
            } label: {
                AppIcon(AppIcons.actionRun, size: AppIcon.Size.medium, weight: .medium)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .frame(width: GlassTokens.controlSize, height: GlassTokens.controlSize)
            }
            .accessibilityLabel("Run")
            .accessibilityIdentifier("action-run")
        }
    }
}

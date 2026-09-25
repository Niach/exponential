import ExpCore
import ExpUI
import SwiftUI

/// EXP-981 — the team's workflows. A workflow is a picked set of backlog issues
/// of ONE repository, planned as a DAG and run as one parallel pass; this is the
/// list of them, banded `Running` / `Draft` / `Done` by the shared rule
/// (`WorkflowView.bands`) with the platform's filled group band over FLAT rows
/// and no row buttons.
///
/// A PUSHED detail off the Agent page's Workflows glyph: a phone gets no tab of
/// its own (web and the IDE put the entry in their sidebars, after Automations).
struct WorkflowsListView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(TeamState.self) private var teamState
    @Environment(\.pushRoute) private var pushRoute
    @State private var model: WorkflowsViewModel?

    var body: some View {
        ZStack {
            AppBackground()

            if let model {
                if model.workflows.isEmpty {
                    VStack(spacing: 0) {
                        Spacer()
                        if model.isLoading {
                            ProgressView().tint(.white)
                        } else {
                            emptyState
                        }
                        Spacer()
                    }
                } else {
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 0) {
                            ForEach(model.bands) { band in
                                GlassSectionBand(band.title)
                                ForEach(band.rows) { row($0) }
                            }
                        }
                        .padding()
                    }
                }
            } else {
                ProgressView().tint(.white)
            }
        }
        .navigationTitle(WorkflowView.title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("workflows-list")
        .onAppear {
            if model == nil {
                model = WorkflowsViewModel(accountId: accountId, db: deps.db)
            }
            model?.observe(teamId: teamState.activeTeamId)
        }
        .onChange(of: teamState.activeTeamId) { _, teamId in
            model?.observe(teamId: teamId)
        }
        .onDisappear { model?.stop() }
    }

    /// One workflow: its glyph, its name, the shared shape line under it, and
    /// the warning glyph a cyclic plan wears (it cannot start). No row buttons
    /// — the whole row opens the detail.
    private func row(_ workflow: WorkflowEntity) -> some View {
        let metrics = workflow.parsedMetrics
        return Button {
            pushRoute(.workflow(accountId: accountId, id: workflow.id))
        } label: {
            HStack(spacing: 10) {
                AppIcon(AppIcons.navWorkflows, size: AppIcon.Size.medium)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                VStack(alignment: .leading, spacing: 2) {
                    Text(workflow.name)
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                    // EXP-982: the shape line, led by the status word for the
                    // two statuses the band alone does not tell apart.
                    Text(WorkflowView.rowSubtitle(status: workflow.status, metrics: metrics))
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                }
                Spacer(minLength: 0)
                // EXP-1086: a run of it asked a person something.
                if model?.asking.contains(workflow.id) == true {
                    FloatingBarBadgeDot(color: DesignTokens.Semantic.red)
                        .accessibilityLabel(WorkflowView.needsYouLabel)
                }
                if !metrics.cycles.isEmpty {
                    AppIcon(AppIcons.uiWarning, size: AppIcon.Size.small)
                        .foregroundStyle(DesignTokens.Palette.destructive)
                        .accessibilityLabel("Blocked in a cycle")
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .flatRow()
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("workflow-row")
    }

    private var emptyState: some View {
        VStack(spacing: 12) {
            AppIcon(AppIcons.navWorkflows, size: 22)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text(WorkflowView.emptyTitle)
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text(WorkflowView.emptyBody)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .multilineTextAlignment(.center)
        }
        .padding(.horizontal, 40)
        .accessibilityIdentifier("workflows-empty")
    }
}

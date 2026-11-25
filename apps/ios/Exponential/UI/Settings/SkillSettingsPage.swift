import ExpCore
import ExpUI
import GRDB
import SwiftUI

// #EXP-1661 (skills.json `page`): one skill's page on a phone, READ-ONLY
// (phones never create or edit a skill): the prompt page's layout with the
// name, the description and the SKILL.md body in mono. The row syncs; the
// body is `skills.get` (the only body path).
struct SkillSettingsPage: View {
    let skillId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster

    @State private var skill: SkillEntity?
    @State private var resolved = false
    @State private var observation: Task<Void, Never>?
    /// nil = not loaded.
    @State private var bodyText: String?
    @State private var loadingBody = false
    @State private var loadFailed = false

    var body: some View {
        ZStack {
            AppBackground()
            if skill != nil {
                form
            } else if resolved {
                EmptyState(title: "Not found", description: "It may still be syncing, or it was deleted.",
                           icon: AppIcons.uiDocs)
                    .frame(maxHeight: .infinity)
            } else {
                SkeletonRows()
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            }
        }
        .navigationTitle(skill?.name ?? Skills.title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .task { await start() }
        .onDisappear {
            observation?.cancel()
            observation = nil
        }
    }

    // MARK: - Form

    /// The layout ×5 (web `SkillForm`, read-only): the band, one group of the
    /// glyph + mono name over the description, the body, the Pre-select switch.
    private var form: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 8) {
                GlassSectionBand(skill?.name ?? "")

                VStack(alignment: .leading, spacing: 0) {
                    HStack(spacing: 12) {
                        AppIcon(AppIcons.uiDocs, size: 16)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        Text(skill?.name ?? "")
                            .font(.expMono(.subheadline))
                            .foregroundStyle(.white)
                            .lineLimit(1)
                            .accessibilityIdentifier("skill-name")
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 12)
                    GlassDivider()
                    Text(skill?.description ?? "")
                        .font(.subheadline)
                        .foregroundStyle(.white)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(16)
                        .accessibilityIdentifier("skill-description")
                }
                .glassSection()

                bodySection
                    .padding(16)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .glassSection()

                GlassToggleRow(
                    Skills.Field.enabledByDefault,
                    isOn: .constant(skill?.enabledByDefault == true)
                )
                .disabled(true)
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
                .glassSection()
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
        }
        .tint(DesignTokens.Palette.primary)
        .accessibilityIdentifier("skill-form")
    }

    @ViewBuilder
    private var bodySection: some View {
        if loadingBody {
            // The prompt page's body placeholder (EXP-1275 D8).
            SkeletonRows(rows: 3, rowHeight: 22, dot: false, dividers: false)
        } else if loadFailed {
            VStack(spacing: 12) {
                Text("Skill unavailable")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                GlassPill(FeedbackStates.Errors.retry, icon: AppIcons.uiRefresh, mode: .action {
                    Task { await loadBody(retried: true) }
                })
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 12)
        } else {
            Text(bodyText ?? "")
                .font(.expMono(.footnote))
                .foregroundStyle(.white)
                .textSelection(.enabled)
                .accessibilityIdentifier("skill-body")
        }
    }

    // MARK: - Load

    @MainActor
    private func start() async {
        guard observation == nil else { return }
        guard let pool = try? deps.db.pool(forAccountId: accountId) else {
            resolved = true
            return
        }
        let id = skillId
        let values = ValueObservation.tracking { db in try SkillEntity.fetchOne(db, key: id) }
        observation = Task {
            do {
                for try await row in values.values(in: pool) {
                    skill = row
                    resolved = true
                }
            } catch {
                resolved = true
            }
        }
        await loadBody(retried: false)
    }

    /// The body through `skills.get`. The load on open fails quietly (the
    /// field's Retry); a failed Retry toasts (feedback-states.json).
    @MainActor
    private func loadBody(retried: Bool) async {
        loadingBody = true
        defer { loadingBody = false }
        do {
            let fetched = try await deps.skillsApi.get(accountId: accountId, id: skillId)
            bodyText = fetched.body
            loadFailed = false
        } catch {
            loadFailed = true
            if retried {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "load", thing: "the skill"),
                    description: error.userFacingMessage
                )
            }
        }
    }
}

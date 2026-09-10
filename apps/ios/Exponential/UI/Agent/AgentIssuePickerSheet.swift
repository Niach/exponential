import ExpCore
import ExpUI
import SwiftUI

/// EXP-825: the composer's issue picker — the Start-coding sheet's Issues
/// tab as a sheet of its own: a searchable checklist over the team's
/// eligible issues (`IssueOption`), the rows checked at OPEN pinned first.
/// The pin order is snapshotted at open and never re-sorts on toggle, so a
/// tapped row visibly checks in place instead of teleporting into a pinned
/// group (EXP-241). Checking an issue swaps out an action chip; the
/// single-repository and batch-cap guards caption the list.
struct AgentIssuePickerSheet: View {
    let model: AgentComposerModel

    @Environment(\.dismiss) private var dismiss
    @Environment(\.motion) private var motion
    @State private var searchText = ""
    @State private var pinnedIds: Set<String> = []

    var body: some View {
        GlassSheetChrome(
            title: "Issues",
            height: .full,
            pinnedHeader: {
                GlassSheetSearchField(placeholder: "Search issues", text: $searchText)
                    .padding(.horizontal, GlassSheetTokens.headerHPadding)
                    .padding(.bottom, 8)
            },
            content: {
                VStack(alignment: .leading, spacing: 0) {
                    if rows.isEmpty {
                        Text(model.issues.isEmpty ? "No eligible issues to code." : "No matching issues.")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .padding(.horizontal, GlassSheetTokens.headerHPadding)
                            .padding(.vertical, 12)
                    } else {
                        ForEach(Array(rows.enumerated()), id: \.element.id) { index, option in
                            if index > 0 { GlassDivider() }
                            issueRow(option)
                        }
                    }
                    footer
                }
                .padding(.horizontal, 8)
            },
            primaryAction: {
                GlassSubmitButton("Done") { dismiss() }
            }
        )
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-composer-issues-picker")
        .onAppear { pinnedIds = Set(model.effectiveChecked) }
    }

    /// Pinned rows first (the open-time snapshot), then the rest, search
    /// applied to both and the tail capped so a big team stays scrollable.
    private var rows: [IssueOption] {
        let pinned = model.issues.filter { pinnedIds.contains($0.id) && matches($0) }
        let others = model.issues.filter { !pinnedIds.contains($0.id) && matches($0) }.prefix(50)
        return pinned + others
    }

    private func matches(_ option: IssueOption) -> Bool {
        let trimmed = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return true }
        return option.title.localizedCaseInsensitiveContains(trimmed)
            || (option.identifier ?? "").localizedCaseInsensitiveContains(trimmed)
    }

    private func issueRow(_ option: IssueOption) -> some View {
        let isChecked = model.isChecked(option.id)
        return Button {
            withAnimation(motion.standard) {
                model.toggleIssue(option.id)
            }
        } label: {
            HStack(spacing: 10) {
                // Selection state must be unmissable (EXP-241): body-size
                // glyph swap plus a tinted row background.
                AppIcon(isChecked ? AppIcons.uiSelected : AppIcons.uiUnselected, size: AppIcon.Size.medium)
                    .foregroundStyle(isChecked ? Color.white : .secondary)

                // Issue-list row anatomy (EXP-173): priority icon, mono
                // identifier, status icon, title.
                AppIcon(IssuePriority.from(option.priority).iconName, size: AppIcon.Size.small)
                    .foregroundStyle(IssuePriority.from(option.priority).color)
                    .frame(width: 16)

                Text(option.identifier ?? "")
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                    .frame(minWidth: 60, alignment: .leading)

                AppIcon(IssueStatus.from(option.status).iconName, size: AppIcon.Size.small)
                    .foregroundStyle(IssueStatus.from(option.status).color)
                    .frame(width: 16)

                Text(option.title)
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)

                Spacer(minLength: 0)
            }
            .padding(.vertical, 10)
            .padding(.horizontal, 8)
            .background(
                isChecked ? Color.white.opacity(0.1) : Color.clear,
                in: RoundedRectangle(cornerRadius: 8)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder
    private var footer: some View {
        if model.multiRepo || model.overCap || model.costWarning {
            VStack(alignment: .leading, spacing: 4) {
                if model.multiRepo {
                    Text("Pick issues from a single repository per run.")
                        .foregroundStyle(DesignTokens.Semantic.red)
                }
                if model.overCap {
                    Text("At most \(AgentComposerModel.maxBatchIssues) issues per run. Split the batch.")
                        .foregroundStyle(DesignTokens.Semantic.red)
                } else if model.costWarning {
                    Text("Large batches are token-expensive.")
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }
            .font(.caption)
            .padding(.horizontal, 8)
            .padding(.vertical, 10)
        }
    }
}

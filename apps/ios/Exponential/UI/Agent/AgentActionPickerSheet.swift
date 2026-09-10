import ExpCore
import ExpUI
import SwiftUI

/// EXP-825: the composer's action picker — the Start-coding sheet's Actions
/// tab as a single-pick sheet: builtins pinned FIRST by the `builtin` flag
/// ("Fix merge conflicts", then "Create action" — creation is a subject of
/// this composer now, not a sheet of its own), then the team's rows. Chat is
/// never listed: it is what "no subject" means. A tap picks and dismisses;
/// picking swaps out any issue chips.
struct AgentActionPickerSheet: View {
    let model: AgentComposerModel

    @Environment(\.dismiss) private var dismiss
    @State private var searchText = ""

    var body: some View {
        GlassSheetChrome(
            title: "Actions",
            height: .full,
            pinnedHeader: {
                GlassSheetSearchField(placeholder: "Search actions", text: $searchText)
                    .padding(.horizontal, GlassSheetTokens.headerHPadding)
                    .padding(.bottom, 8)
            },
            content: {
                VStack(alignment: .leading, spacing: 0) {
                    if rows.isEmpty {
                        Text("No matching actions.")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .padding(.horizontal, GlassSheetTokens.headerHPadding)
                            .padding(.vertical, 12)
                    } else {
                        ForEach(Array(rows.enumerated()), id: \.element.id) { index, action in
                            if index > 0 { GlassDivider() }
                            actionRow(action)
                        }
                    }
                }
                .padding(.horizontal, 8)
            }
        )
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-composer-actions-picker")
    }

    private var rows: [ActionDto] {
        model.actions.filter { matches($0) }
    }

    private func matches(_ action: ActionDto) -> Bool {
        let trimmed = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return true }
        return action.name.localizedCaseInsensitiveContains(trimmed)
            || (action.description ?? "").localizedCaseInsensitiveContains(trimmed)
    }

    private func actionRow(_ action: ActionDto) -> some View {
        let isSelected = action.id == model.selectedAction?.id
        return Button {
            model.pickAction(action)
            dismiss()
        } label: {
            HStack(spacing: 10) {
                AppIcon(isSelected ? AppIcons.uiSelected : AppIcons.uiUnselected, size: AppIcon.Size.medium)
                    .foregroundStyle(isSelected ? Color.white : .secondary)

                // EXP-273: the action's own curated glyph (the builtins set
                // one too), falling back to the generic action mark. EXP-721:
                // 16pt, Android's 16dp.
                AppIcon(action.icon ?? AppIcons.actionDefault, size: 16)
                    .foregroundStyle(.secondary)
                    .frame(width: 16)

                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 6) {
                        Text(action.name)
                            .font(.subheadline)
                            .foregroundStyle(.white)
                            .lineLimit(1)
                        if action.repositoryId != nil {
                            // Small repo indicator: this action clones its repo.
                            AppIcon(AppIcons.actionRepository, size: 11)
                                .foregroundStyle(.secondary)
                                .accessibilityLabel("Runs in a repository")
                        }
                    }
                    if let description = action.description, !description.isEmpty {
                        Text(description)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }

                Spacer(minLength: 0)
            }
            .padding(.vertical, 10)
            .padding(.horizontal, 8)
            .background(
                isSelected ? Color.white.opacity(0.1) : Color.clear,
                in: RoundedRectangle(cornerRadius: 8)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

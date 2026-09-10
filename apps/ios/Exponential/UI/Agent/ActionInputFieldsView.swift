import ExpCore
import ExpUI
import SwiftUI

/// EXP-825: the picked action's typed inputs, rendered inside the composer
/// card's leading slot. Only the four PICK types are left (`repo` / `board` /
/// `pr` / `icon`) — free text is the composer's own field now, and an
/// unknown type blocks the run instead of silently degrading (the desktop
/// mirrors this posture).
struct ActionInputFieldsView: View {
    let model: AgentComposerModel
    let inputs: [ActionInputDto]

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(inputs, id: \.key) { def in
                inputField(def)
            }
        }
    }

    @ViewBuilder
    private func inputField(_ def: ActionInputDto) -> some View {
        switch def.type {
        case "repo":
            // The leading `""` is the unset sentinel.
            GlassPickerRow(
                inputLabel(def),
                selection: binding(def),
                options: [""] + model.repos.map(\.id),
                label: { id in
                    guard !id.isEmpty else { return def.isRequired ? "Select a repository" : "None" }
                    return model.repos.first { $0.id == id }?.fullName ?? id
                }
            )
            .padding(.vertical, 4)
        case "board":
            GlassPickerRow(
                inputLabel(def),
                selection: binding(def),
                options: [""] + model.boards.map(\.id),
                label: { id in
                    guard !id.isEmpty else { return def.isRequired ? "Select a board" : "None" }
                    return model.boards.first { $0.id == id }?.name ?? id
                }
            )
            .padding(.vertical, 4)
        case "icon":
            // EXP-273: the value is a curated registry NAME (e.g. `rocket`) —
            // the same string a board stores — picked with the same picker as
            // the create-board form. Optional inputs start at none and can be
            // cleared again.
            HStack(spacing: 12) {
                Text(inputLabel(def))
                    .foregroundStyle(.white.opacity(TextOpacity.primary))
                Spacer(minLength: 8)
                IconPicker(selection: binding(def), allowsNone: !def.isRequired)
            }
            .padding(.vertical, 4)
        case "pr":
            // EXP-259: the value is the REPRESENTATIVE issue id of an open
            // issue-linked PR (batch PRs dedupe by prUrl, so one row can list
            // several identifiers).
            if model.pullRequests.isEmpty {
                HStack(spacing: 8) {
                    Text(inputLabel(def))
                        .foregroundStyle(.white.opacity(TextOpacity.primary))
                    Spacer(minLength: 8)
                    Text("No open pull requests.")
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                .font(.subheadline)
                .padding(.vertical, 4)
            } else {
                GlassPickerRow(
                    inputLabel(def),
                    selection: binding(def),
                    options: [""] + model.pullRequests.map(\.issueId),
                    label: { id in
                        guard !id.isEmpty else { return def.isRequired ? "Select a pull request" : "None" }
                        return model.pullRequests.first { $0.issueId == id }?.label ?? id
                    }
                )
                .padding(.vertical, 4)
            }
        default:
            // Unknown future input type (or a stale `text`/`textarea` row
            // the EXP-825 migration has not reached yet) — block the run.
            Text("This action needs a newer app version.")
                .font(.caption)
                .foregroundStyle(DesignTokens.Semantic.red)
        }
    }

    private func inputLabel(_ def: ActionInputDto) -> String {
        def.isRequired ? def.label : "\(def.label) (optional)"
    }

    private func binding(_ def: ActionInputDto) -> Binding<String> {
        Binding(
            get: { model.value(for: def) },
            set: { model.setValue($0, for: def) }
        )
    }
}

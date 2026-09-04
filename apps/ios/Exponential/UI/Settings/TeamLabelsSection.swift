import ExpUI
import ExpCore
import SwiftUI

struct TeamLabelsSection: View {
    let accountId: String
    let teamId: String
    let labels: [LabelEntity]
    let labelsApi: LabelsApi

    @State private var showCreate = false
    @State private var editingLabel: LabelEntity?
    @State private var deleteTarget: LabelEntity?
    @State private var actionError: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            GlassSectionHeader("Labels") {
                // "New label" rides the header (Boards' "New board" pattern,
                // EXP-331) — labels stay member-level, so no owner gating.
                GlassPill("New label", icon: AppIcons.uiAdd, mode: .action {
                    showCreate = true
                })
            }

            ForEach(labels, id: \.id) { label in
                HStack(spacing: 10) {
                    Circle()
                        .fill(Color(hex: label.color) ?? .gray)
                        .frame(width: 14, height: 14)

                    Text(label.name)
                        .font(.subheadline)
                        .foregroundStyle(.white)

                    Spacer()

                    // Explicit edit entry (EXP-331 — Android parity; replaces
                    // the undiscoverable tap-to-rename / swatch-menu recolor).
                    // EXP-698: the shared chromed circle, but the `ui-edit`
                    // pencil — this opens the editor straight away, so the
                    // overflow glyph would promise a menu that never appears.
                    CircleIconButton(AppIcons.uiEdit, accessibilityLabel: "Edit label") {
                        editingLabel = label
                    }

                    // Delete (confirmed — labels stay member-level, so no owner
                    // gating, only a confirmation).
                    CircleIconButton(
                        AppIcons.uiDelete,
                        accessibilityLabel: "Delete label",
                        tint: DesignTokens.Palette.destructive.opacity(0.7)
                    ) {
                        deleteTarget = label
                    }
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .glassRow()
            }

            if let actionError {
                Text(actionError)
                    .font(.caption)
                    .foregroundStyle(.red.opacity(0.8))
            }
        }
        .sheet(isPresented: $showCreate) {
            LabelEditorSheet { name, color in
                Task {
                    await run {
                        _ = try await labelsApi.create(accountId: accountId, CreateLabelInput(
                            name: name,
                            color: color,
                            teamId: teamId
                        ))
                    }
                }
            }
        }
        .sheet(item: $editingLabel) { label in
            LabelEditorSheet(
                title: "Edit label",
                confirmLabel: "Save",
                initialName: label.name,
                initialColor: label.color
            ) { name, color in
                let newName: String? = name == label.name ? nil : name
                let newColor: String? = color.lowercased() == label.color.lowercased() ? nil : color
                guard newName != nil || newColor != nil else { return }
                Task {
                    await run {
                        try await labelsApi.update(accountId: accountId, UpdateLabelInput(
                            teamId: teamId,
                            labelId: label.id,
                            name: newName,
                            color: newColor
                        ))
                    }
                }
            }
        }
        .alert("Delete label?", isPresented: Binding(
            get: { deleteTarget != nil },
            set: { if !$0 { deleteTarget = nil } }
        ), presenting: deleteTarget) { label in
            Button("Cancel", role: .cancel) { deleteTarget = nil }
            Button("Delete", role: .destructive) {
                Task { await run { try await labelsApi.delete(accountId: accountId, teamId: teamId, labelId: label.id) } }
            }
        } message: { label in
            Text("\"\(label.name)\" will be removed from all issues. This cannot be undone.")
        }
    }

    /// Run a label mutation, surfacing the server's clean message on failure
    /// instead of silently swallowing it. Labels are member-level, so failures
    /// here are transient (network/permission), not an owner gate.
    private func run(_ op: () async throws -> Void) async {
        do {
            try await op()
            actionError = nil
        } catch {
            actionError = error.trpcUserMessage
        }
    }
}

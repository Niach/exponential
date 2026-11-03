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
    @Environment(\.toaster) private var toaster

    var body: some View {
        // EXP-818: a filled group band over flat rows — the labels read as a
        // table (web `ListRow`, desktop `surface::flat_row`).
        VStack(alignment: .leading, spacing: 0) {
            GlassSectionBand("Labels") {
                // "New label" rides the header (Boards' "New board" pattern,
                // EXP-331) — labels stay member-level, so no owner gating.
                GlassPill("New label", icon: AppIcons.uiAdd, mode: .action {
                    showCreate = true
                })
            }

            // EXP-1274: the shared list empty state ×4.
            if labels.isEmpty {
                EmptyState(.labels)
            }

            // Web's order (`orderBy sortOrder`; unsorted rows last, id breaks ties).
            ForEach(Self.webOrder(labels), id: \.id) { label in
                HStack(spacing: 12) {
                    // Web edits the name + colour in the row itself; here the
                    // row opens the label editor (no pencil, ×4 = one trash).
                    Button { editingLabel = label } label: {
                        HStack(spacing: 12) {
                            Circle()
                                .fill(Color(hex: label.color) ?? .gray)
                                .frame(width: 16, height: 16)

                            Text(label.name)
                                .font(.subheadline)
                                .foregroundStyle(.white)
                                .lineLimit(1)

                            Spacer(minLength: 0)
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Edit label \(label.name)")

                    // Delete (confirmed — labels stay member-level, so no owner
                    // gating, only a confirmation). Web's grey ghost trash.
                    GhostIconButton(AppIcons.uiDelete, accessibilityLabel: "Delete label") {
                        deleteTarget = label
                    }
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
                .flatRow()
            }
        }
        .sheet(isPresented: $showCreate) {
            LabelEditorSheet { name, color in
                Task {
                    await run(verb: "create") {
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
                    await run(verb: "save") {
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
        // EXP-1215: the app's own alert card (`GlassAlert`), ×4.
        .glassAlert(item: $deleteTarget) { label in
            GlassAlert(prompt: Prompts.DeleteLabel.copy(name: label.name), handlers: [
                "delete": {
                    Task { await run(verb: "delete") { try await labelsApi.delete(accountId: accountId, teamId: teamId, labelId: label.id) } }
                },
            ])
        }
    }

    /// Web `labels` live query: `orderBy(sortOrder)`.
    static func webOrder(_ labels: [LabelEntity]) -> [LabelEntity] {
        labels.sorted { a, b in
            switch (a.sortOrder, b.sortOrder) {
            case let (x?, y?) where x != y: return x < y
            case (_?, nil): return true
            case (nil, _?): return false
            default: return a.id < b.id
            }
        }
    }

    /// Run a label mutation, surfacing the server's clean message on failure
    /// instead of silently swallowing it. Labels are member-level, so failures
    /// here are transient (network/permission), not an owner gate.
    private func run(verb: String, _ op: () async throws -> Void) async {
        do {
            try await op()
        } catch {
            // FEED-79: `Couldn't <verb> the label` + the server's message.
            toaster.error(
                FeedbackStates.Errors.title(verb: verb, thing: "the label"),
                description: error.trpcUserMessage
            )
        }
    }
}

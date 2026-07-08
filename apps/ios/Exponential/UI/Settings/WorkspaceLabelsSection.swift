import ExpUI
import ExpCore
import SwiftUI

private let labelColors = [
    "#ef4444", "#dc2626", "#f97316", "#f59e0b", "#eab308",
    "#84cc16", "#22c55e", "#10b981", "#14b8a6", "#06b6d4",
    "#0ea5e9", "#3b82f6", "#6366f1", "#8b5cf6", "#a855f7",
    "#ec4899", "#f43f5e", "#78716c", "#64748b", "#a3a3a3",
]

struct WorkspaceLabelsSection: View {
    let accountId: String
    let workspaceId: String
    let labels: [LabelEntity]
    let labelsApi: LabelsApi

    @State private var showCreate = false
    @State private var newLabelName = ""
    @State private var newLabelColor = "#3b82f6"
    @State private var editingLabelId: String?
    @State private var editingName = ""
    @State private var deleteTarget: LabelEntity?
    @State private var actionError: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("Labels")
                    .font(.headline)
                    .foregroundStyle(.white)
                Text("\(labels.count)")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }

            ForEach(labels, id: \.id) { label in
                HStack(spacing: 10) {
                    // Color swatch
                    Menu {
                        ForEach(labelColors, id: \.self) { color in
                            Button {
                                Task { await run { try await labelsApi.update(accountId: accountId, UpdateLabelInput(id: label.id, color: color)) } }
                            } label: {
                                HStack {
                                    Circle().fill(Color(hex: color) ?? .gray).frame(width: 12, height: 12)
                                    Text(color)
                                }
                            }
                        }
                    } label: {
                        Circle()
                            .fill(Color(hex: label.color) ?? .gray)
                            .frame(width: 14, height: 14)
                    }

                    // Name (editable)
                    if editingLabelId == label.id {
                        TextField("Name", text: $editingName)
                            .font(.subheadline)
                            .textFieldStyle(.plain)
                            .foregroundStyle(.white)
                            .onSubmit {
                                Task {
                                    await run { try await labelsApi.update(accountId: accountId, UpdateLabelInput(id: label.id, name: editingName)) }
                                    editingLabelId = nil
                                }
                            }
                    } else {
                        Text(label.name)
                            .font(.subheadline)
                            .foregroundStyle(.white)
                            .onTapGesture {
                                editingLabelId = label.id
                                editingName = label.name
                            }
                    }

                    Spacer()

                    // Delete (confirmed — labels stay member-level, so no owner
                    // gating, only a confirmation).
                    Button {
                        deleteTarget = label
                    } label: {
                        Image(systemName: "trash")
                            .font(.caption)
                            .foregroundStyle(.red.opacity(0.5))
                    }
                    .buttonStyle(.plain)
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                .glassRow()
            }

            if let actionError {
                Text(actionError)
                    .font(.caption)
                    .foregroundStyle(.red.opacity(0.8))
            }

            // Create new label
            if showCreate {
                VStack(spacing: 8) {
                    TextField("Label name", text: $newLabelName)
                        .font(.subheadline)
                        .textFieldStyle(.plain)
                        .foregroundStyle(.white)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 8)
                        .background(Color.white.opacity(0.06))
                        .clipShape(RoundedRectangle(cornerRadius: 8))

                    // Color palette
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 28), spacing: 6)], spacing: 6) {
                        ForEach(labelColors, id: \.self) { color in
                            Button {
                                newLabelColor = color
                            } label: {
                                Circle()
                                    .fill(Color(hex: color) ?? .gray)
                                    .frame(width: 22, height: 22)
                                    .overlay(
                                        Circle().stroke(Color.white, lineWidth: newLabelColor == color ? 2 : 0)
                                    )
                            }
                        }
                    }

                    HStack {
                        Button("Cancel") {
                            showCreate = false
                            newLabelName = ""
                        }
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))

                        Spacer()

                        Button("Create") {
                            Task {
                                await run {
                                    try await labelsApi.create(accountId: accountId, CreateLabelInput(
                                        name: newLabelName,
                                        color: newLabelColor,
                                        workspaceId: workspaceId
                                    ))
                                }
                                showCreate = false
                                newLabelName = ""
                            }
                        }
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.white)
                        .disabled(newLabelName.isEmpty)
                    }
                }
                .padding(12)
                .glassSection()
            } else {
                Button {
                    showCreate = true
                } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "plus")
                        Text("New label")
                    }
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 10)
                }
                .glassButton()
                .buttonStyle(.plain)
            }
        }
        .alert("Delete Label", isPresented: Binding(
            get: { deleteTarget != nil },
            set: { if !$0 { deleteTarget = nil } }
        ), presenting: deleteTarget) { label in
            Button("Cancel", role: .cancel) { deleteTarget = nil }
            Button("Delete", role: .destructive) {
                Task { await run { try await labelsApi.delete(accountId: accountId, id: label.id) } }
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

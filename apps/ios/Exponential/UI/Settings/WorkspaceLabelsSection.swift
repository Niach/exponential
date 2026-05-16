import SwiftUI

private let labelColors = [
    "#ef4444", "#f97316", "#eab308", "#22c55e", "#14b8a6",
    "#3b82f6", "#6366f1", "#a855f7", "#ec4899", "#6b7280",
]

struct WorkspaceLabelsSection: View {
    let workspaceId: String
    let labels: [LabelEntity]
    let labelsApi: LabelsApi

    @State private var showCreate = false
    @State private var newLabelName = ""
    @State private var newLabelColor = "#3b82f6"
    @State private var editingLabelId: String?
    @State private var editingName = ""

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
                                Task { try? await labelsApi.update(UpdateLabelInput(id: label.id, color: color)) }
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
                                    try? await labelsApi.update(UpdateLabelInput(id: label.id, name: editingName))
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

                    // Delete
                    Button {
                        Task { try? await labelsApi.delete(id: label.id) }
                    } label: {
                        Image(systemName: "trash")
                            .font(.caption)
                            .foregroundStyle(.red.opacity(0.5))
                    }
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                .glassRow()
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
                    HStack(spacing: 6) {
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
                                try? await labelsApi.create(CreateLabelInput(
                                    name: newLabelName,
                                    color: newLabelColor,
                                    workspaceId: workspaceId
                                ))
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
    }
}

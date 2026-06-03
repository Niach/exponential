import ExpCore
import ExpUI
import SwiftUI

/// Create a project in the given workspace (no iOS sibling — projects are
/// created on web today). Opened from the sidebar "+" in MacShell.
struct MacCreateProjectView: View {
    @Environment(MacAppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss
    let accountId: String
    let workspaceId: String
    var onCreated: (String) -> Void = { _ in }

    @State private var name = ""
    @State private var prefix = ""
    @State private var prefixEdited = false
    @State private var color = DEFAULT_LABEL_COLOR
    @State private var loading = false
    @State private var error: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("New Project").font(.title3.weight(.semibold))

            TextField("Name", text: $name)
                .textFieldStyle(.roundedBorder)
                .onChange(of: name) { _, newValue in
                    // Auto-suggest a prefix from the name until the user edits it.
                    if !prefixEdited {
                        prefix = String(newValue.filter(\.isLetter).prefix(3)).uppercased()
                    }
                }

            TextField("Prefix (e.g. ENG)", text: $prefix)
                .textFieldStyle(.roundedBorder)
                .onChange(of: prefix) { _, _ in prefixEdited = true }

            Text("Color").font(.caption).foregroundStyle(.secondary)
            ColorSwatchGrid(selection: $color)

            if let error { Text(error).foregroundStyle(.red).font(.callout) }

            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                Button("Create") { Task { await create() } }
                    .buttonStyle(.borderedProminent)
                    .tint(Accent.indigo)
                    .disabled(loading || name.trimmingCharacters(in: .whitespaces).isEmpty || prefix.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .padding(20)
        .frame(width: 420)
    }

    private func create() async {
        loading = true
        error = nil
        do {
            let id = try await deps.projectsApi.create(accountId: accountId, CreateProjectInput(
                workspaceId: workspaceId,
                name: name.trimmingCharacters(in: .whitespaces),
                prefix: prefix.trimmingCharacters(in: .whitespaces),
                color: color
            ))
            onCreated(id)
            dismiss()
        } catch {
            self.error = error.localizedDescription
        }
        loading = false
    }
}

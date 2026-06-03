import ExpCore
import ExpUI
import SwiftUI

/// Create a workspace on the given account. Opened from the workspace switcher
/// "New workspace" entry in MacShell.
struct MacCreateWorkspaceView: View {
    @Environment(MacAppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss
    let accountId: String
    var onCreated: (String) -> Void = { _ in }

    @State private var name = ""
    @State private var loading = false
    @State private var error: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("New Workspace").font(.title3.weight(.semibold))

            TextField("Name", text: $name).textFieldStyle(.roundedBorder)

            if let error { Text(error).foregroundStyle(.red).font(.callout) }

            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                Button("Create") { Task { await create() } }
                    .buttonStyle(.borderedProminent)
                    .tint(Accent.indigo)
                    .disabled(loading || name.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .padding(20)
        .frame(width: 380)
    }

    private func create() async {
        loading = true
        error = nil
        do {
            let workspace = try await deps.workspacesApi.create(accountId: accountId, name: name.trimmingCharacters(in: .whitespaces))
            onCreated(workspace.id)
            dismiss()
        } catch {
            self.error = error.localizedDescription
        }
        loading = false
    }
}

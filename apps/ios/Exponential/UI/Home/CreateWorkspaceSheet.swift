import ExpCore
import ExpUI
import SwiftUI

/// Create a workspace for an account. iOS mirror of `MacCreateWorkspaceView`.
/// `onCreated` hands the new workspace back so the caller can chain into
/// first-project creation (a brand-new workspace has no projects, so it stays
/// hidden from Home until it has one).
struct CreateWorkspaceSheet: View {
    let accountId: String
    var onCreated: (WorkspaceResult) -> Void = { _ in }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss

    @State private var name = ""
    @State private var loading = false
    @State private var error: String?

    private var canCreate: Bool {
        !name.trimmingCharacters(in: .whitespaces).isEmpty && !loading
    }

    var body: some View {
        NavigationStack {
            ZStack {
                AppBackground()
                VStack(alignment: .leading, spacing: 16) {
                    TextField("Workspace name", text: $name)
                        .textFieldStyle(.plain)
                        .font(.body)
                        .foregroundStyle(.white)
                        .padding(12)
                        .background(Color.white.opacity(0.05))
                        .clipShape(RoundedRectangle(cornerRadius: 8))
                    if let error {
                        Text(error).font(.caption).foregroundStyle(.red)
                    }
                    Spacer()
                }
                .padding(20)
            }
            .navigationTitle("New Workspace")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Create") { Task { await create() } }.disabled(!canCreate)
                }
            }
        }
    }

    private func create() async {
        loading = true
        error = nil
        do {
            let ws = try await deps.workspacesApi.create(
                accountId: accountId,
                name: name.trimmingCharacters(in: .whitespaces)
            )
            onCreated(ws)
            dismiss()
        } catch {
            self.error = error.localizedDescription
        }
        loading = false
    }
}

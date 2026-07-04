import ExpCore
import ExpUI
import SwiftUI

/// Create a project in a workspace. iOS mirror of `MacCreateProjectView` — name
/// → auto-derived prefix → `ColorSwatchGrid`. The APIs already exist in ExpCore.
struct CreateProjectSheet: View {
    let accountId: String
    let workspaceId: String
    var onCreated: (String) -> Void = { _ in }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss

    @State private var name = ""
    @State private var prefix = ""
    @State private var prefixEdited = false
    @State private var color = DEFAULT_LABEL_COLOR
    @State private var repository: ProjectRepositoryChoice?
    @State private var loading = false
    @State private var error: String?

    private var canCreate: Bool {
        !name.trimmingCharacters(in: .whitespaces).isEmpty
            && !prefix.trimmingCharacters(in: .whitespaces).isEmpty
            && repository != nil
            && !loading
    }

    var body: some View {
        NavigationStack {
            ZStack {
                AppBackground()
                ScrollView {
                    VStack(alignment: .leading, spacing: 16) {
                        field("Name", text: $name)
                            .onChange(of: name) { _, newValue in
                                // Auto-suggest a prefix from the name until edited.
                                if !prefixEdited {
                                    prefix = String(newValue.filter(\.isLetter).prefix(3)).uppercased()
                                }
                            }
                        field("Prefix (e.g. ENG)", text: $prefix, monospaced: true)
                            .textInputAutocapitalization(.characters)
                            .autocorrectionDisabled()
                            .onChange(of: prefix) { _, _ in prefixEdited = true }

                        Text("Color")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        ColorSwatchGrid(selection: $color)

                        Divider().background(Color.white.opacity(0.08))

                        // v4: every project is backed by a repo — required.
                        RepositorySelector(
                            accountId: accountId,
                            workspaceId: workspaceId,
                            selection: $repository
                        )

                        if let error {
                            Text(error).font(.caption).foregroundStyle(.red)
                        }
                    }
                    .padding(20)
                }
            }
            .navigationTitle("New Project")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Create") { Task { await create() } }.disabled(!canCreate)
                }
            }
        }
    }

    private func field(_ placeholder: String, text: Binding<String>, monospaced: Bool = false) -> some View {
        TextField(placeholder, text: text)
            .textFieldStyle(.plain)
            .font(monospaced ? .body.monospaced() : .body)
            .foregroundStyle(.white)
            .padding(12)
            .background(Color.white.opacity(0.05))
            .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    private func create() async {
        guard let repository else { return }
        loading = true
        error = nil
        do {
            let id = try await deps.projectsApi.create(accountId: accountId, CreateProjectInput(
                workspaceId: workspaceId,
                name: name.trimmingCharacters(in: .whitespaces),
                prefix: prefix.trimmingCharacters(in: .whitespaces),
                color: color,
                repository: repository
            ))
            onCreated(id)
            dismiss()
        } catch {
            self.error = error.localizedDescription
        }
        loading = false
    }
}

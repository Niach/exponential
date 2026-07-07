import ExpUI
import ExpCore
import SwiftUI

// The create-first-project form (web onboarding parity, wizard.tsx): name with
// an auto-derived prefix, color, and a REQUIRED backing repository (a project
// IS a repo). One `projects.create` call connects the repo inline. Reused by
// the first-run onboarding page and the empty-state "Create project" sheets.
struct CreateProjectForm: View {
    let accountId: String
    let workspaceId: String
    /// Onboarding renders the minimal spec form (shared mobile onboarding
    /// spec): just Project name + Repository. The prefix stays auto-derived
    /// from the name and the color keeps its default — the full form (prefix +
    /// color fields) remains for the regular create-project sheets.
    var minimal = false
    /// Called with the new project id once `projects.create` succeeds. The
    /// caller owns what happens next (finish onboarding, dismiss a sheet, …).
    let onCreated: (String) -> Void

    @Environment(AppDependencies.self) private var deps

    @State private var name = ""
    @State private var prefix = ""
    // Stop deriving the prefix from the name once the user edits it by hand.
    @State private var prefixEdited = false
    @State private var color = DEFAULT_LABEL_COLOR
    @State private var repository: ProjectRepositoryChoice?
    @State private var saving = false
    @State private var errorText: String?
    // Plan-cap failures render as a softer nudge than hard errors.
    @State private var limitText: String?

    private var canCreate: Bool {
        !name.trimmingCharacters(in: .whitespaces).isEmpty
            && !prefix.trimmingCharacters(in: .whitespaces).isEmpty
            && repository != nil
            && !saving
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            // Name + prefix
            VStack(alignment: .leading, spacing: 8) {
                fieldLabel("Project name")
                TextField("e.g. Backend API", text: Binding(
                    get: { name },
                    set: { onNameChange($0) }
                ))
                .font(.subheadline)
                .textFieldStyle(.plain)
                .foregroundStyle(.white)
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .background(Color.white.opacity(0.06))
                .clipShape(RoundedRectangle(cornerRadius: 8))
            }

            if !minimal {
                VStack(alignment: .leading, spacing: 8) {
                    fieldLabel("Prefix")
                    TextField("e.g. API", text: Binding(
                        get: { prefix },
                        set: { onPrefixChange($0) }
                    ))
                    .font(.subheadline.monospaced())
                    .textFieldStyle(.plain)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.characters)
                    .foregroundStyle(.white)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 10)
                    .background(Color.white.opacity(0.06))
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                }

                // Color
                VStack(alignment: .leading, spacing: 8) {
                    fieldLabel("Color")
                    ColorSwatchGrid(selection: $color)
                }
            }

            // Repository (required) — the selector renders its own label.
            RepositorySelector(
                accountId: accountId,
                workspaceId: workspaceId,
                selection: $repository
            )

            if let errorText {
                Text(errorText)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color.red.opacity(0.1))
                    .clipShape(RoundedRectangle(cornerRadius: 8))
            }
            if let limitText {
                HStack(alignment: .top, spacing: 8) {
                    Image(systemName: "sparkles")
                        .font(.caption)
                        .foregroundStyle(DesignTokens.Semantic.blue)
                    Text(limitText)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                .frame(maxWidth: .infinity, alignment: .leading)
                .glassRow()
            }

            Button {
                Task { await create() }
            } label: {
                Text(saving ? "Creating…" : "Create project")
                    .font(.body.weight(.medium))
                    .foregroundStyle(.white)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 14)
            }
            .disabled(!canCreate)
            .background(canCreate ? Color.white.opacity(0.15) : Color.white.opacity(0.06))
            .clipShape(RoundedRectangle(cornerRadius: 10))
            .overlay(
                RoundedRectangle(cornerRadius: 10)
                    .stroke(Color.white.opacity(0.1), lineWidth: 0.5)
            )
        }
    }

    private func fieldLabel(_ text: String) -> some View {
        Text(text)
            .font(.caption.weight(.medium))
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
    }

    // MARK: - Editing

    private func onNameChange(_ value: String) {
        name = value
        if !prefixEdited { prefix = Self.derivePrefix(value) }
    }

    private func onPrefixChange(_ value: String) {
        prefixEdited = true
        prefix = String(value.uppercased().prefix(10))
    }

    /// Port of web `derivePrefix` (lib/project.ts): first letter of each word,
    /// uppercased, capped at 5. Separators are whitespace / `-` / `_`.
    static func derivePrefix(_ name: String) -> String {
        let letters = name
            .split(whereSeparator: { $0 == "-" || $0 == "_" || $0.isWhitespace })
            .compactMap { $0.first.map(String.init) }
            .joined()
            .uppercased()
        return String(letters.prefix(5))
    }

    // MARK: - Create

    private func create() async {
        guard let repository, canCreate else { return }
        saving = true
        errorText = nil
        limitText = nil
        do {
            let projectId = try await deps.projectsApi.create(
                accountId: accountId,
                CreateProjectInput(
                    workspaceId: workspaceId,
                    name: name.trimmingCharacters(in: .whitespaces),
                    prefix: prefix.trimmingCharacters(in: .whitespaces),
                    color: color,
                    repository: repository
                )
            )
            // Leave `saving` set — the caller swaps this view out on success.
            onCreated(projectId)
        } catch {
            if error.isPlanLimitError {
                limitText = error.trpcUserMessage
            } else {
                errorText = error.trpcUserMessage
            }
            saving = false
        }
    }
}

// Sheet wrapper for the empty-state "Create project" entry points (Issues home,
// workspace settings). Dismisses itself once the project is created.
struct CreateProjectSheet: View {
    let accountId: String
    let workspaceId: String
    var onCreated: (String) -> Void = { _ in }

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            ZStack {
                AppBackground()
                ScrollView {
                    CreateProjectForm(
                        accountId: accountId,
                        workspaceId: workspaceId,
                        onCreated: { projectId in
                            onCreated(projectId)
                            dismiss()
                        }
                    )
                    .padding(16)
                }
            }
            .navigationTitle("New project")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) { Button("Cancel") { dismiss() } }
            }
        }
    }
}

import ExpUI
import ExpCore
import SwiftUI

// The create-first-project form (web onboarding parity, wizard.tsx): pick a
// quickstart template (Dev / Task / Feedback board — each pre-sets the public
// switch + icon), then one form of name, prefix, color, an ALWAYS-optional
// repository, and a public toggle. One `projects.create` call carries
// `isPublic` + `icon` (never the deprecated `type`). Reused by the first-run
// onboarding page and the empty-state "Create project" sheets.
struct CreateProjectForm: View {
    let accountId: String
    let workspaceId: String
    /// Onboarding renders the minimal spec form (shared mobile onboarding
    /// spec): template + name + optional repository. The prefix stays
    /// auto-derived from the name and the color keeps its default — the full
    /// form (prefix + color + icon fields) remains for the regular sheets.
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
    // Selected quickstart (stable template id); drives the presets below.
    @State private var templateId = ProjectTypeDisplay.templates[0].id
    // The two values actually sent to the server. Seeded from the template but
    // independently overridable (public toggle + icon picker).
    @State private var isPublic = false
    @State private var icon = ProjectTypeDisplay.templates[0].icon
    // Whether the repo section is disclosed (repos are ALWAYS optional now —
    // this only decides the initial reveal, seeded from the template).
    @State private var showRepository = true
    @State private var repository: ProjectRepositoryChoice?
    @State private var saving = false
    @State private var errorText: String?
    // Plan-cap failures render as a softer nudge than hard errors.
    @State private var limitText: String?

    // A repository is optional on every project — creation only needs a name
    // and prefix.
    private var canCreate: Bool {
        !name.trimmingCharacters(in: .whitespaces).isEmpty
            && !prefix.trimmingCharacters(in: .whitespaces).isEmpty
            && !saving
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            // Quickstart templates (3 cards) — pre-set the public switch, icon,
            // and whether the repo section starts shown.
            VStack(alignment: .leading, spacing: 8) {
                fieldLabel("Template")
                VStack(spacing: 8) {
                    ForEach(ProjectTypeDisplay.templates) { template in
                        templateCard(template)
                    }
                }
            }

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

                // Icon (curated glyphs) — grid of the 16 supported names.
                VStack(alignment: .leading, spacing: 8) {
                    fieldLabel("Icon")
                    iconGrid
                }
            }

            // Public toggle — the board's read-only-visitor switch. Seeded by
            // the template; freely overridable.
            Toggle(isOn: $isPublic) {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Public board")
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.white)
                    Text("A read-only roadmap anyone can view. Visitors can't sign in to write.")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .multilineTextAlignment(.leading)
                }
            }
            .tint(Accent.indigo)

            // Repository (always optional) — the selector renders its own label.
            // Disclosed by default for the Dev template; a button reveals it for
            // the others.
            if showRepository {
                RepositorySelector(
                    accountId: accountId,
                    workspaceId: workspaceId,
                    selection: $repository
                )
            } else {
                Button {
                    showRepository = true
                } label: {
                    HStack(spacing: 8) {
                        Image(systemName: "plus.circle")
                            .font(.caption)
                        Text("Connect a repository (optional)")
                            .font(.subheadline)
                    }
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
                .buttonStyle(.plain)
            }

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

    @ViewBuilder
    private func templateCard(_ template: ProjectTemplate) -> some View {
        let selected = templateId == template.id
        Button {
            applyTemplate(template)
        } label: {
            HStack(spacing: 12) {
                Image(systemName: template.symbol)
                    .font(.body)
                    .foregroundStyle(.white.opacity(selected ? 1 : TextOpacity.secondary))
                    .frame(width: 24)
                VStack(alignment: .leading, spacing: 2) {
                    Text(template.label)
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.white)
                    Text(template.summary)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .multilineTextAlignment(.leading)
                }
                Spacer(minLength: 8)
                if selected {
                    Image(systemName: "checkmark.circle.fill")
                        .font(.body)
                        .foregroundStyle(Accent.indigo)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.white.opacity(selected ? 0.1 : 0.04))
            .clipShape(RoundedRectangle(cornerRadius: 8))
            .overlay(
                RoundedRectangle(cornerRadius: 8)
                    .stroke(selected ? Accent.indigo.opacity(0.6) : Color.white.opacity(0.08), lineWidth: 1)
            )
        }
        .buttonStyle(.plain)
    }

    // Grid of the 16 curated glyphs (DomainContract.projectIconValues). Tapping
    // one overrides the template's preset icon.
    private var iconGrid: some View {
        LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 8), count: 8), spacing: 8) {
            ForEach(DomainContract.projectIconValues, id: \.self) { name in
                let selected = icon == name
                Button {
                    icon = name
                } label: {
                    Image(systemName: ProjectTypeDisplay.iconSymbol(for: name) ?? "square")
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(selected ? 1 : TextOpacity.secondary))
                        .frame(maxWidth: .infinity)
                        .frame(height: 32)
                        .background(Color.white.opacity(selected ? 0.12 : 0.04))
                        .clipShape(RoundedRectangle(cornerRadius: 6))
                        .overlay(
                            RoundedRectangle(cornerRadius: 6)
                                .stroke(selected ? Accent.indigo.opacity(0.6) : Color.white.opacity(0.08), lineWidth: 1)
                        )
                }
                .buttonStyle(.plain)
            }
        }
    }

    /// Adopt a quickstart template: seed the public switch, icon, and repo
    /// disclosure. The name/prefix/color the user already typed are preserved.
    private func applyTemplate(_ template: ProjectTemplate) {
        templateId = template.id
        isPublic = template.isPublic
        icon = template.icon
        showRepository = template.showsRepository
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
        guard canCreate else { return }
        saving = true
        errorText = nil
        limitText = nil
        do {
            // The repo is optional on every project; send whatever's selected
            // (only when the section is disclosed) and let coding affordances
            // gate on its presence later.
            let projectId = try await deps.projectsApi.create(
                accountId: accountId,
                CreateProjectInput(
                    workspaceId: workspaceId,
                    name: name.trimmingCharacters(in: .whitespaces),
                    prefix: prefix.trimmingCharacters(in: .whitespaces),
                    color: color,
                    isPublic: isPublic,
                    icon: icon,
                    repository: showRepository ? repository : nil
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

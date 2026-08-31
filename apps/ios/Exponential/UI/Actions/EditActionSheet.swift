import ExpCore
import ExpUI
import SwiftUI

// EXP-694: the mobile action editor — until now editing an action was
// web/desktop-only, so the phone could run a prompt it could never read. The
// form is the create sheet's, minus the run options and plus the prompt: icon +
// name on ONE row, the description, the repository picker, and the markdown
// body itself.
//
// The body is deliberately NOT on the Electric `actions` shape (a ≤64KB
// markdown blob has no business riding sync), so the sheet fetches it through
// tRPC `actions.get` on open — exactly what the web and desktop editors do —
// and saves ONLY the fields that actually changed via `actions.update`
// (`icon`/`repositoryId` clear as an explicit null). Writes are owner-only
// server-side, so a non-owner gets the same sheet read-only: fields disabled,
// no Save, and a plain "Action" title.
struct EditActionSheet: View {
    /// The synced list row (its `body` is empty by shape design — the prompt
    /// arrives from `actions.get`).
    let action: ActionDto
    /// Team owners write; every other member reads (the server refuses their
    /// update anyway).
    var canEdit: Bool = false

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss

    @State private var name = ""
    @State private var descriptionText = ""
    @State private var icon = ""
    @State private var repoId = ""
    @State private var prompt = ""
    @State private var repos: [TeamRepo] = []
    /// The fetched row's own values — the dirty check and the patch are both
    /// against what the server last said, never against the synced list row
    /// (which carries no body at all).
    @State private var loadedBody: String?
    @State private var loading = true
    @State private var saving = false
    @State private var errorText: String?

    private var loaded: Bool { loadedBody != nil }

    /// Only what actually changed rides the patch: an untouched field must not
    /// be re-sent (a rename pre-check, a repo membership check and the
    /// automation guard all hang off the fields present in the input).
    private var patch: ActionPatch {
        var patch = ActionPatch()
        let trimmedName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmedName != action.name { patch.name = trimmedName }
        let trimmedDescription = descriptionText.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmedDescription != (action.description ?? "") {
            patch.description = .some(trimmedDescription.isEmpty ? nil : trimmedDescription)
        }
        if icon != (action.icon ?? "") {
            patch.icon = .some(icon.isEmpty ? nil : icon)
        }
        if repoId != (action.repositoryId ?? "") {
            patch.repositoryId = .some(repoId.isEmpty ? nil : repoId)
        }
        if let loadedBody, prompt != loadedBody { patch.body = prompt }
        return patch
    }

    private var canSave: Bool {
        canEdit && loaded && !saving
            && !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && !prompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && !patch.isEmpty
    }

    var body: some View {
        Group {
            if canEdit {
                GlassSheetChrome(
                    title: "Edit action",
                    height: .full,
                    content: { form },
                    primaryAction: {
                        GlassSubmitButton("Save changes", enabled: canSave, loading: saving) {
                            save()
                        }
                    }
                )
            } else {
                // Read-only: no primary action at all, so the sheet resolves to
                // the action-less chrome and draws no bottom strip.
                GlassSheetChrome(title: "Action", height: .full) { form }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("edit-action-sheet")
        .task { await load() }
    }

    // MARK: - Form

    private var form: some View {
        Form {
            identitySection
            descriptionSection
            repositorySection
            promptSection
            if let errorText {
                Section {
                    Text(errorText)
                        .font(.caption)
                        .foregroundStyle(DesignTokens.Semantic.red)
                }
                .listRowBackground(glassFormRowFill)
            }
        }
        // EXP-603: the sheet's own background shows through the grouped list;
        // rows carry the glass fill.
        .scrollContentBackground(.hidden)
        .listSectionSpacing(8)
        // EXP-594: white control tint — system blue is retired.
        .tint(DesignTokens.Palette.primary)
    }

    /// Icon + name on ONE row (S7: byte-identical to the create sheet's).
    private var identitySection: some View {
        Section {
            HStack(spacing: 12) {
                IconPicker(selection: $icon, allowsNone: true)
                    .disabled(!canEdit)
                TextField("Name", text: $name)
                    .disabled(!canEdit)
                    .accessibilityIdentifier("edit-action-name")
            }
        }
        .listRowBackground(glassFormRowFill)
    }

    /// Inline placeholder title (S7) — no label above the field.
    private var descriptionSection: some View {
        Section {
            TextField("Description", text: $descriptionText, axis: .vertical)
                .lineLimit(4...10)
                .disabled(!canEdit)
                .accessibilityIdentifier("edit-action-description")
        }
        .listRowBackground(glassFormRowFill)
    }

    private var repositorySection: some View {
        Section {
            GlassPickerRow(
                "Repository",
                selection: $repoId,
                options: [""] + repos.map(\.id),
                label: { id in
                    guard !id.isEmpty else { return "None" }
                    return repos.first { $0.id == id }?.fullName ?? id
                },
                enabled: canEdit
            )
        }
        .listRowBackground(glassFormRowFill)
    }

    /// The markdown prompt itself — monospaced, and tall enough to read like
    /// the editor it is. It only exists once `actions.get` lands.
    @ViewBuilder
    private var promptSection: some View {
        Section {
            if loading {
                HStack(spacing: 8) {
                    ProgressView().controlSize(.small).tint(.white)
                    Text("Loading prompt…")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    Spacer(minLength: 0)
                }
            } else {
                TextField("Prompt", text: $prompt, axis: .vertical)
                    .font(.system(.footnote, design: .monospaced))
                    .lineLimit(8...24)
                    .disabled(!canEdit)
                    .accessibilityIdentifier("edit-action-prompt")
            }
        }
        .listRowBackground(glassFormRowFill)
    }

    // MARK: - Load / save

    @MainActor
    private func load() async {
        // The metadata is already synced — seed from it so the form is never
        // blank while the body is in flight.
        name = action.name
        descriptionText = action.description ?? ""
        icon = action.icon ?? ""
        repoId = action.repositoryId ?? ""
        repos = (try? await deps.repositoriesApi.list(
            accountId: accountId, teamId: action.teamId
        )) ?? []
        do {
            let fetched = try await deps.actionsApi.get(accountId: accountId, id: action.id)
            prompt = fetched.body
            loadedBody = fetched.body
        } catch {
            errorText = error.userFacingMessage
        }
        loading = false
    }

    private func save() {
        guard canSave else { return }
        let payload = patch
        saving = true
        errorText = nil
        Task {
            do {
                try await deps.actionsApi.update(
                    accountId: accountId, id: action.id, patch: payload
                )
                saving = false
                // The synced row echoes the metadata back — nothing local to
                // write, so the sheet just closes.
                dismiss()
            } catch {
                errorText = error.userFacingMessage
                saving = false
            }
        }
    }
}

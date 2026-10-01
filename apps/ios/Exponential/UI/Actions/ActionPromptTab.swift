import ExpCore
import ExpUI
import SwiftUI

// The action page's PROMPT tab (SLOP-2) — the action editor EXP-694 brought to
// the phone as a sheet, now the first of the page's three tabs: icon + name on
// ONE row, the description, the composer hint, the repository picker, and the
// markdown body itself.
//
// The body is deliberately NOT on the Electric `actions` shape (a ≤64KB
// markdown blob has no business riding sync), so the tab fetches it through
// tRPC `actions.get` on open — exactly what the web and desktop editors do —
// and saves ONLY the fields that actually changed via `actions.update`
// (`icon`/`repositoryId` clear as an explicit null). Writes are owner-only
// server-side, so a non-owner gets the same form read-only: fields disabled,
// no Save.
struct ActionPromptTab: View {
    /// The synced list row (its `body` is empty by shape design — the prompt
    /// arrives from `actions.get`).
    let action: ActionDto
    /// Team owners write; every other member reads (the server refuses their
    /// update anyway).
    var canEdit: Bool = false

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId

    @State private var name = ""
    @State private var descriptionText = ""
    /// EXP-825: the composer's field hint while this action is picked.
    @State private var promptPlaceholder = ""
    @State private var icon = ""
    @State private var repoId = ""
    @State private var prompt = ""
    @State private var repos: [TeamRepo] = []
    /// The fetched row's own values — the dirty check and the patch are both
    /// against what the server last said, never against the synced list row
    /// (which carries no body at all).
    @State private var loadedBody: String?
    /// What the last save answered with. The synced row echoes it a moment
    /// later; until then the dirty check runs against THIS, so a saved form
    /// does not keep offering Save.
    @State private var savedRow: ActionDto?
    @State private var loading = true
    @State private var saving = false
    @State private var errorText: String?

    private var loaded: Bool { loadedBody != nil }

    /// The metadata the form is dirty AGAINST: the last save's answer while the
    /// synced row has not caught up with it, else the synced row.
    private var baseline: ActionDto { savedRow ?? action }

    /// Only what actually changed rides the patch: an untouched field must not
    /// be re-sent (a rename pre-check, a repo membership check and the
    /// trigger guard all hang off the fields present in the input).
    private var patch: ActionPatch {
        let action = baseline
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
        let trimmedHint = promptPlaceholder.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmedHint != (action.promptPlaceholder ?? "") {
            // Blank clears (an explicit null, like the description).
            patch.promptPlaceholder = .some(trimmedHint.isEmpty ? nil : trimmedHint)
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
        form
            // Owners only: a read-only form draws no bottom strip at all.
            .safeAreaInset(edge: .bottom, spacing: 0) {
                if canEdit {
                    GlassSubmitButton("Save changes", enabled: canSave, loading: saving) {
                        save()
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
                }
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("action-prompt-tab")
            .task { await load() }
            // The synced row caught up (or moved on): it is the baseline again.
            .onChange(of: action.updatedAt) { savedRow = nil }
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
        // EXP-603: the page's own background shows through the grouped list;
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

    /// Inline placeholder title (S7) — no label above the field. EXP-825:
    /// the composer hint sits under the description (the web dialog's
    /// order) — what the requester should type beside this action, shown as
    /// the composer field's placeholder while it is picked.
    private var descriptionSection: some View {
        Section {
            GlassTextField(
                "Description",
                text: $descriptionText,
                lines: 4...10,
                bordered: false,
                accessibilityIdentifier: "edit-action-description"
            )
            .disabled(!canEdit)
            GlassTextField(
                "Composer hint, e.g. Scope: which platforms, which version",
                text: $promptPlaceholder,
                bordered: false,
                accessibilityIdentifier: "edit-action-prompt-placeholder"
            )
            .disabled(!canEdit)
            // Client parity with the server's cap (UTF-16 units, the JS
            // length), so a long paste is refused at the field instead of
            // at submit.
            .onChange(of: promptPlaceholder) { _, value in
                let clamped = ActionDto.clampPromptPlaceholder(value)
                if clamped != value { promptPlaceholder = clamped }
            }
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
                GlassTextField(
                    "Prompt",
                    text: $prompt,
                    lines: 8...24,
                    bordered: false,
                    accessibilityIdentifier: "edit-action-prompt"
                )
                .font(.system(.footnote, design: .monospaced))
                .disabled(!canEdit)
            }
        }
        .listRowBackground(glassFormRowFill)
    }

    // MARK: - Load / save

    @MainActor
    private func load() async {
        // Once per mount: the pager may re-run the task when the page comes
        // back, and that must not throw away an edit in progress.
        guard loading else { return }
        // The metadata is already synced — seed from it so the form is never
        // blank while the body is in flight.
        name = action.name
        descriptionText = action.description ?? ""
        promptPlaceholder = action.promptPlaceholder ?? ""
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
                let saved = try await deps.actionsApi.update(
                    accountId: accountId, id: action.id, patch: payload
                )
                // The page stays open: the answer becomes the baseline, so the
                // form reads clean until the synced row echoes it.
                savedRow = saved
                if let body = payload.body { loadedBody = body }
                saving = false
            } catch {
                errorText = error.userFacingMessage
                saving = false
            }
        }
    }
}

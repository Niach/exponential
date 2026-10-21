import ExpCore
import ExpUI
import SwiftUI

// The saved prompt's editor (SLOP-2; NEW-2: prompts replace actions; NEW-7
// moves it to Settings → Prompts) — icon + name on ONE row and the markdown
// body itself (prompts carry no composer hint).
//
// The body is deliberately NOT on the Electric `prompts` shape (a ≤64KB
// markdown blob has no business riding sync), so the page fetches it through
// tRPC `prompts.get` on open — exactly what the web and desktop editors do —
// and saves ONLY the fields that actually changed via `prompts.update`
// (`icon` clears as an explicit null). Writes are owner-only
// server-side, so a non-owner gets the same form read-only: fields disabled,
// no Save.
struct ActionPromptTab: View {
    /// The synced list row (its `body` is empty by shape design — the prompt
    /// arrives from `prompts.get`).
    let action: ActionDto
    /// Team owners write; every other member reads (the server refuses their
    /// update anyway).
    var canEdit: Bool = false

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId

    @State private var name = ""
    @State private var icon = ""
    @State private var prompt = ""
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
    /// FEED-79: a name already taken (CONFLICT) is INPUT, inline under the
    /// name; a failed save is a toast. The body loads on appear, so a failed
    /// load is quiet: the prompt's empty state + Retry (a failed Retry tap
    /// toasts).
    @State private var nameError: String?
    @State private var loadFailed = false
    @Environment(\.toaster) private var toaster

    private var loaded: Bool { loadedBody != nil }

    /// The metadata the form is dirty AGAINST: the last save's answer while the
    /// synced row has not caught up with it, else the synced row.
    private var baseline: ActionDto { savedRow ?? action }

    /// Only what actually changed rides the patch: an untouched field must not
    /// be re-sent (the rename pre-check hangs off the fields present in the
    /// input).
    private var patch: PromptPatch {
        let action = baseline
        var patch = PromptPatch()
        let trimmedName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmedName != action.name { patch.name = trimmedName }
        if icon != (action.icon ?? "") {
            patch.icon = .some(icon.isEmpty ? nil : icon)
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
            promptSection
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
                    .onChange(of: name) { _, _ in nameError = nil }
            }
            if let nameError {
                Text(nameError)
                    .font(.system(size: CGFloat(FeedbackStates.Errors.inlineSize)))
                    .foregroundStyle(DesignTokens.Semantic.red)
            }
        }
        .listRowBackground(glassFormRowFill)
    }

    /// The markdown prompt itself — monospaced, and tall enough to read like
    /// the editor it is. It only exists once `prompts.get` lands.
    @ViewBuilder
    private var promptSection: some View {
        Section {
            if loading {
                // EXP-1275 (D8): the prompt's placeholder, not "Loading…".
                SkeletonRows(rows: 3, rowHeight: 22, dot: false, dividers: false)
            } else if loadFailed {
                VStack(spacing: 12) {
                    Text("Prompt unavailable")
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    GlassPill(FeedbackStates.Errors.retry, icon: AppIcons.uiRefresh, mode: .action {
                        Task { await retryLoad() }
                    })
                }
                .frame(maxWidth: .infinity)
                .padding(.vertical, 12)
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
        // Once per mount: a re-run task must not throw away an edit in
        // progress.
        guard loading else { return }
        // The metadata is already synced — seed from it so the form is never
        // blank while the body is in flight.
        name = action.name
        icon = action.icon ?? ""
        await loadBody(accountId: accountId, retried: false)
        loading = false
    }

    /// The Retry tap: the body fetch again (the rest of the form keeps what
    /// was typed), and a toast only if it fails again.
    @MainActor
    private func retryLoad() async {
        loading = true
        await loadBody(accountId: accountId, retried: true)
        loading = false
    }

    @MainActor
    private func loadBody(accountId: String, retried: Bool) async {
        do {
            let fetched = try await deps.promptsApi.get(accountId: accountId, id: action.id)
            prompt = fetched.body ?? ""
            loadedBody = fetched.body ?? ""
            loadFailed = false
        } catch {
            loadFailed = true
            if retried {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "load", thing: "the prompt"),
                    description: error.userFacingMessage
                )
            }
        }
    }

    private func save() {
        guard canSave else { return }
        let payload = patch
        saving = true
        nameError = nil
        Task {
            do {
                let saved = try await deps.promptsApi.update(
                    accountId: accountId, id: action.id, patch: payload
                )
                // The page stays open: the answer becomes the baseline, so the
                // form reads clean until the synced row echoes it.
                savedRow = ActionDto(prompt: saved)
                if let body = payload.body { loadedBody = body }
                saving = false
            } catch {
                if error.trpcErrorCode == "CONFLICT" {
                    nameError = error.userFacingMessage
                } else {
                    toaster.error(
                        FeedbackStates.Errors.title(verb: "save", thing: "the prompt"),
                        description: error.userFacingMessage
                    )
                }
                saving = false
            }
        }
    }
}

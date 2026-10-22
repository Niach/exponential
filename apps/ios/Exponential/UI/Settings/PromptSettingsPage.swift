import ExpCore
import ExpUI
import GRDB
import SwiftUI

// NEW-2: Settings → Prompts, built like Settings → Agents (web
// `components/team/prompts-section.tsx` + `prompt-form.tsx`, desktop
// `settings/prompts.rs`). A PROMPT = a saved first message (actions became
// these): a name, a curated icon and the markdown body, nothing else. Running
// one is the composer's `/` menu. Writes are owner-only (the server enforces
// it); a member reads the same page with every control disabled and no Save
// or Delete.

/// The Prompts band in Team settings: "New prompt" (owners) on the band over
/// one flat row per prompt (icon + name); a row opens the prompt's page.
struct TeamPromptsSection: View {
    let prompts: [PromptEntity]
    let isOwner: Bool
    /// nil = a new prompt.
    let onOpen: (String?) -> Void

    static let title = "Prompts"
    static let newPrompt = "New prompt"

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            GlassSectionBand(Self.title) {
                if isOwner {
                    GlassPill(Self.newPrompt, icon: AppIcons.uiAdd, mode: .action {
                        onOpen(nil)
                    })
                    .accessibilityIdentifier("prompts-new")
                }
            }

            if prompts.isEmpty {
                EmptyState(title: "No prompts yet", icon: AppIcons.navActions, size: .list)
                    .padding(.vertical, 8)
            }
            ForEach(prompts) { prompt in
                Button {
                    onOpen(prompt.id)
                } label: {
                    HStack(spacing: 10) {
                        AppIcon(ActionIconDisplay.iconName(for: prompt.icon), size: 16)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        Text(prompt.name)
                            .font(.subheadline)
                            .foregroundStyle(.white)
                            .lineLimit(1)
                        Spacer(minLength: 8)
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 10)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .flatRow()
                .accessibilityIdentifier("prompts-list-row-\(prompt.id)")
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("prompts-list")
    }
}

/// One prompt's page (`promptId`), or "New prompt" (nil, owners only): THE
/// form (icon + name on ONE row, the markdown body) with Save changes /
/// Create prompt, and Delete prompt for owners behind the app's own confirm.
struct PromptSettingsPage: View {
    let teamId: String
    let promptId: String?

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss
    @Environment(\.toaster) private var toaster

    /// The prompt this page edits: the route's, or the one a create made.
    @State private var currentId: String?
    @State private var prompt: PromptEntity?
    @State private var resolved = false
    @State private var isOwner = false
    @State private var observation: Task<Void, Never>?

    @State private var name = ""
    @State private var icon = ""
    @State private var bodyText = ""
    /// The body as `prompts.get` (or the last save) said it; nil = not loaded.
    @State private var loadedBody: String?
    /// The metadata the form is dirty against: the last save's answer until
    /// the synced row catches up.
    @State private var savedRow: PromptDto?
    @State private var loadingBody = false
    @State private var loadFailed = false
    @State private var saving = false
    @State private var submitted = false
    /// FEED-79: a taken name (CONFLICT) is input, inline under the name.
    @State private var nameError: String?
    @State private var showDelete = false
    @State private var deleting = false

    /// The server's `MAX_PROMPT_BODY` (64 KB of characters).
    static let bodyMaxChars = 64 * 1024

    private var creating: Bool { currentId == nil }
    private var trimmedName: String { name.trimmingCharacters(in: .whitespacesAndNewlines) }

    private var baselineName: String { savedRow?.name ?? prompt?.name ?? "" }
    private var baselineIcon: String { (savedRow != nil ? savedRow?.icon : prompt?.icon) ?? "" }

    private var patch: PromptPatch {
        var patch = PromptPatch()
        if trimmedName != baselineName { patch.name = trimmedName }
        if icon != baselineIcon { patch.icon = .some(icon.isEmpty ? nil : icon) }
        if let loadedBody, bodyText != loadedBody { patch.body = bodyText }
        return patch
    }

    private var bodyTooLong: String? {
        bodyText.count > Self.bodyMaxChars
            ? "Too long: \(bodyText.count) of \(Self.bodyMaxChars) characters"
            : nil
    }

    private var canSave: Bool {
        guard isOwner, !saving, !trimmedName.isEmpty,
              !bodyText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              bodyTooLong == nil
        else { return false }
        if creating { return true }
        return loadedBody != nil && !loadFailed && !patch.isEmpty
    }

    var body: some View {
        ZStack {
            AppBackground()
            if creating || prompt != nil {
                form
            } else if resolved {
                EmptyState(title: "Not found", description: "It may still be syncing, or it was deleted.",
                           icon: AppIcons.navActions)
                    .frame(maxHeight: .infinity)
            } else {
                SkeletonRows()
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            }
        }
        .navigationTitle(creating ? TeamPromptsSection.newPrompt : (prompt?.name ?? TeamPromptsSection.title))
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if isOwner, creating || prompt != nil {
                GlassSubmitButton(
                    creating ? "Create prompt" : "Save changes",
                    enabled: canSave,
                    loading: saving
                ) {
                    save()
                }
                .accessibilityIdentifier("prompt-submit")
                .padding(.horizontal, 16)
                .padding(.vertical, 8)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("prompt-page")
        .task { await start() }
        .onDisappear {
            observation?.cancel()
            observation = nil
        }
        .onChange(of: prompt?.updatedAt) { savedRow = nil }
        // EXP-1215: the app's own confirm card (prompts.json-style copy).
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(isPresented: $showDelete) {
                    GlassAlert(
                        title: "Delete \"\(prompt?.name ?? "")\"?",
                        actions: [
                            GlassAlertAction("Cancel", role: .outline, isCancel: true, id: "cancel") {},
                            GlassAlertAction("Delete", role: .destructive, enabled: !deleting, id: "prompt-delete-confirm") {
                                Task { await deletePrompt() }
                            },
                        ]
                    )
                }
        }
    }

    // MARK: - Form

    private var form: some View {
        Form {
            Section {
                // Icon and name are ONE row on every client.
                HStack(spacing: 12) {
                    IconPicker(selection: $icon, allowsNone: true)
                        .disabled(!isOwner)
                    TextField("Name", text: $name)
                        .disabled(!isOwner)
                        .accessibilityIdentifier("prompt-name")
                        .onChange(of: name) { _, _ in nameError = nil }
                }
                if let error = nameError ?? (submitted && trimmedName.isEmpty ? "Enter a name" : nil) {
                    Text(error)
                        .font(.system(size: CGFloat(FeedbackStates.Errors.inlineSize)))
                        .foregroundStyle(DesignTokens.Semantic.red)
                }
            }
            .listRowBackground(glassFormRowFill)

            Section {
                if loadingBody {
                    // EXP-1275 (D8): the body's placeholder, not "Loading…".
                    SkeletonRows(rows: 3, rowHeight: 22, dot: false, dividers: false)
                } else if loadFailed {
                    VStack(spacing: 12) {
                        Text("Prompt unavailable")
                            .font(.subheadline)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        GlassPill(FeedbackStates.Errors.retry, icon: AppIcons.uiRefresh, mode: .action {
                            Task { await loadBody(retried: true) }
                        })
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 12)
                } else {
                    GlassTextField(
                        "Prompt",
                        text: $bodyText,
                        lines: 8...24,
                        bordered: false,
                        accessibilityIdentifier: "prompt-body"
                    )
                    .font(.system(.footnote, design: .monospaced))
                    .disabled(!isOwner)
                }
                if let bodyTooLong {
                    Text(bodyTooLong)
                        .font(.system(size: CGFloat(FeedbackStates.Errors.inlineSize)))
                        .foregroundStyle(DesignTokens.Semantic.red)
                }
            }
            .listRowBackground(glassFormRowFill)

            if isOwner, !creating, prompt != nil {
                Section {
                    Button(role: .destructive) {
                        showDelete = true
                    } label: {
                        HStack(spacing: 8) {
                            AppIcon(AppIcons.uiDelete, size: AppIcon.Size.medium)
                            Text("Delete prompt")
                        }
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(DesignTokens.Semantic.red)
                    }
                    .accessibilityIdentifier("prompt-delete")
                }
                .listRowBackground(glassFormRowFill)
            }
        }
        // EXP-603: the page's own background shows through the grouped list.
        .scrollContentBackground(.hidden)
        .listSectionSpacing(8)
        .tint(DesignTokens.Palette.primary)
        .accessibilityIdentifier("prompt-form")
    }

    // MARK: - Load

    @MainActor
    private func start() async {
        guard observation == nil else { return }
        if currentId == nil { currentId = promptId }
        guard let pool = try? deps.db.pool(forAccountId: accountId) else {
            resolved = true
            return
        }
        let teamId = teamId
        let userId = deps.auth.userId
        let isAdmin = deps.auth.isAdmin
        isOwner = (try? await pool.read { db in
            try TeamPermissions.resolve(
                team: try TeamEntity.fetchOne(db, key: teamId), currentUserId: userId, isAdmin: isAdmin, db: db
            ).isOwner
        }) ?? false
        if let id = currentId {
            observe(id, pool: pool)
            await loadBody(retried: false)
        } else {
            resolved = true
        }
    }

    private func observe(_ id: String, pool: DatabasePool) {
        observation?.cancel()
        let values = ValueObservation.tracking { db in try PromptEntity.fetchOne(db, key: id) }
        var seeded = false
        observation = Task {
            do {
                for try await row in values.values(in: pool) {
                    prompt = row
                    resolved = true
                    // Seed the metadata ONCE (a resync must not wipe typing).
                    if let row, !seeded {
                        seeded = true
                        name = row.name
                        icon = row.icon ?? ""
                    }
                }
            } catch {
                resolved = true
            }
        }
    }

    /// The body through `prompts.get` (the ONLY body path). The load on open
    /// fails quietly (the field's empty state + Retry); a failed Retry toasts.
    @MainActor
    private func loadBody(retried: Bool) async {
        guard let id = currentId else { return }
        loadingBody = true
        defer { loadingBody = false }
        do {
            let fetched = try await deps.promptsApi.get(accountId: accountId, id: id)
            bodyText = fetched.body ?? ""
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

    // MARK: - Save / delete

    private func save() {
        submitted = true
        guard canSave else { return }
        saving = true
        nameError = nil
        Task {
            defer { saving = false }
            do {
                if let id = currentId {
                    let payload = patch
                    let saved = try await deps.promptsApi.update(accountId: accountId, id: id, patch: payload)
                    savedRow = saved
                    if let body = payload.body { loadedBody = body }
                } else {
                    let created = try await deps.promptsApi.create(
                        accountId: accountId,
                        PromptCreateInput(
                            teamId: teamId, name: trimmedName, icon: icon.isEmpty ? nil : icon, body: bodyText
                        )
                    )
                    // The page becomes the new prompt's (it reads the live row).
                    currentId = created.id
                    savedRow = created
                    loadedBody = created.body ?? bodyText
                    if let pool = try? deps.db.pool(forAccountId: accountId) { observe(created.id, pool: pool) }
                }
            } catch {
                if error.trpcErrorCode == "CONFLICT" {
                    nameError = error.userFacingMessage
                } else {
                    toaster.error(
                        FeedbackStates.Errors.title(verb: creating ? "create" : "save", thing: "the prompt"),
                        description: error.userFacingMessage
                    )
                }
            }
        }
    }

    @MainActor
    private func deletePrompt() async {
        guard let id = currentId else { return }
        deleting = true
        defer { deleting = false }
        do {
            try await deps.promptsApi.delete(accountId: accountId, id: id)
            dismiss()
        } catch {
            toaster.error(
                FeedbackStates.Errors.title(verb: "delete", thing: "the prompt"),
                description: error.userFacingMessage
            )
        }
    }
}

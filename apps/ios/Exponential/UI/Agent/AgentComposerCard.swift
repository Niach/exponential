import ExpCore
import ExpUI
import PhotosUI
import SwiftUI
import UIKit

/// EXP-825: the ONE launcher's card — the same `GlassComposer` the steer and
/// comment composers wear. Slot order is leading · field · strip · tools:
///
/// - leading: the starting row after Start. EXP-1038: the subject chips are
///   NOT here — they head the page, beside the contract verb
///   (`AgentComposerHeadline`, mounted by the host above this card);
/// - field: the one-block markdown field with the `@` / `#` / `:` typeahead
///   (the host mounts `EditorAutocompleteMenu` under the card);
/// - strip: the pending images (the steer composer's tiles + markers) and,
///   since wave D, the pending files (a file tile each);
/// - tools: EXP-1249 / S-3 — the ONE "+" (`ComposerPlusMenu`, a bottom
///   sheet: Photo or file · Issue › | Device › · Account › · Model › ·
///   Repository › (pushed pages) | Effort › · Subagents › · Ultracode | MCP
///   servers › · Computer use) and NOTHING else: no options line under the
///   card, no inline image/file/#/emoji buttons. The agent row above the
///   card (composer-home.json, the host's) picks the recipient through
///   `model.pickAgent` / `model.clearAgent`; while an agent is picked
///   (`model.agentId != nil`) the "+" hides its runner and work rows and
///   the send is `sessions.send({agentId})` (`AgentComposerModel`); NEW-2:
///   prompts are the `/` menu's (the host mounts it under the card); the
///   submit is the round send GLYPH (EXP-827: icon-only on every client),
///   whose title ("Start chat" / "Start coding" / "Start session" / "Send")
///   is its accessibility name.
struct AgentComposerCard: View {
    let model: AgentComposerModel
    /// Where the composer routes a tapped `#EXP-1` chip.
    let onIssueRefTap: (String) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var showPhotoPicker = false
    @State private var photoItems: [PhotosPickerItem] = []
    /// Wave D: "Add file or image" = the Photo | File sub-choice, floated
    /// from the "+" once its sheet closed.
    @State private var showAttachMenu = false
    @State private var showFileImporter = false
    @State private var plusAnchor: CGRect = .zero
    /// EXP-1249: the "+" sheet, and the row it closed on — promoted to its
    /// picker once the sheet finished dismissing.
    @State private var showMenu = false
    @State private var menuPick: ComposerMenu.RowId?
    @State private var showEffortPicker = false
    @State private var showMcpPicker = false
    /// The team's MCP servers (`mcpServers.list`), read once per team.
    @State private var mcpServers: [McpServerRow] = []
    /// Review #19 (D8): the starting row shows only once the start has been
    /// pending `FeedbackStates.Pending.delayMs`, like every client's.
    @State private var startingShown = false

    var body: some View {
        GlassComposer {
            leading
        } field: {
            MarkdownComposerField(
                model: model.draftEditor,
                placeholder: model.placeholder,
                onReturn: { handleReturn() },
                onPasteImage: { image in ingestPastedImage(image) },
                onIssueRefTap: onIssueRefTap,
                minHeight: 44,
                maxHeight: 160,
                // EXP-1276: the keyboard-down chevron above the keyboard.
                showsDismissAccessory: true
            )
            // The text view carries a 6pt inset of its own (`singleLine`), so
            // the card's 12/12 band is spelled 6/6 here.
            .padding(.horizontal, 6)
            .padding(.top, 6)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("agent-composer-field")
        } strip: {
            if !model.pendingImages.isEmpty || !model.pendingFiles.isEmpty {
                PendingAttachmentStrip(
                    items: PendingStripItem.steer(images: model.pendingImages, files: model.pendingFiles)
                ) { id in
                    model.removeAttachment(id: id)
                }
                .padding(.horizontal, 8)
                .padding(.bottom, 4)
            }
            // A pick refused for its size or type is INPUT: it stays inline
            // (FEED-79), at the 13pt error size.
            if let imageError = model.imageError {
                Text(imageError)
                    .font(.system(size: CGFloat(FeedbackStates.Errors.inlineSize)))
                    .foregroundStyle(DesignTokens.Semantic.red)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 12)
                    .padding(.bottom, 4)
            }
        } tools: {
            // EXP-1249 ×4: the "+" is the composer's ONLY tool. Every
            // picker it hands off to hangs off ITS OWN zero-size node
            // (stacking presentations on one node is where SwiftUI starts
            // dropping them).
            GlassComposerToolButton(AppIcons.uiAdd, accessibilityLabel: ComposerMenu.plusLabel) {
                showMenu = true
            }
            .accessibilityIdentifier(ComposerMenu.plusTestId)
            .onGeometryChange(for: CGRect.self, of: { $0.frame(in: .global) }) { frame in
                plusAnchor = frame
            }
            .sheet(isPresented: $showMenu, onDismiss: promoteMenuPick) {
                ComposerPlusMenu(
                    model: model,
                    mcpServers: mcpServers,
                    onPick: { id in
                        menuPick = id
                        showMenu = false
                    },
                    onClose: { showMenu = false }
                )
            }
            .background { effortPicker }
            .background { mcpPicker }
            .photosPicker(
                isPresented: $showPhotoPicker,
                selection: $photoItems,
                maxSelectionCount: AgentComposerPrompt.maxImages,
                matching: .images
            )
            .onChange(of: photoItems) { _, newItems in
                guard !newItems.isEmpty else { return }
                Task { await ingestPhotos(newItems) }
            }
            .background {
                Color.clear
                    .steerAttachChoiceMenu(
                        isPresented: $showAttachMenu,
                        anchor: plusAnchor,
                        onPhoto: { showPhotoPicker = true },
                        onFile: { showFileImporter = true }
                    )
            }
            .background {
                Color.clear
                    .fileImporter(
                        isPresented: $showFileImporter,
                        allowedContentTypes: [.item],
                        allowsMultipleSelection: true
                    ) { result in
                        guard case let .success(urls) = result else { return }
                        Task { await ingestFiles(urls) }
                    }
            }
        } submit: {
            // EXP-827: the round send GLYPH, icon-only ×4 — the subject chips
            // already say what a send starts, and the per-subject label
            // (`Start coding` / `Start session` / `Send`) stays
            // the button's NAME for VoiceOver. Same
            // `GlassComposerSubmitButton` the comment and steer composers wear.
            GlassComposerSubmitButton(
                AppIcons.uiSubmit,
                accessibilityLabel: model.submitTitle,
                enabled: model.canSubmit,
                // EXP-1271 (D8 PENDING): from the press until the send answers.
                pending: model.sending
            ) {
                model.submit()
            }
            .accessibilityIdentifier("agent-composer-submit")
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-composer")
        .pendingDelay(model.starting != nil, shown: $startingShown)
        // EXP-792: the team's MCP servers, and the pick they seed.
        .task(id: model.teamId) { await loadMcpServers() }
        // EXP-1249 (web M12 rule ×4): the per-run computer use is the
        // person's explicit FLIP, nil = untouched — the toggle shows the
        // machine's current default and the start omits the key, so the
        // device's own setting applies. A device change clears the flip.
        .onChange(of: model.device?.deviceId, initial: true) { _, _ in
            model.launch.computerUse = nil
        }
    }

    // MARK: - The "+" menu's pickers (EXP-1249)

    /// The row the menu closed on opens its picker now the sheet is gone.
    private func promoteMenuPick() {
        guard let pick = menuPick else { return }
        menuPick = nil
        switch pick {
        case .addFile:
            var transaction = Transaction()
            transaction.disablesAnimations = true
            withTransaction(transaction) { showAttachMenu = true }
        case .effort: showEffortPicker = true
        case .mcpServers: showMcpPicker = true
        // Issue and the runner submenus are pages of the sheet itself.
        case .implementIssue, .prompts, .device, .account, .model, .repository, .subagents, .ultracode, .computerUse: break
        }
    }

    private var effortPicker: some View {
        let launch = model.launch
        let values = [LaunchVocabulary.cliDefault] + LaunchVocabulary.effortValues(for: launch.agent)
        return GlassPicker(
            items: values.map { value in
                PickerItem(
                    value: value,
                    label: value == LaunchVocabulary.cliDefault ? "CLI default" : LaunchVocabulary.effortLabel(value)
                )
            },
            mode: .single,
            value: [launch.effort],
            onChange: { picked in if let value = picked.first { launch.effort = value } },
            title: LaunchVocabulary.effortTitle(for: launch.agent),
            open: $showEffortPicker,
            hideTrigger: true,
            sheetIdentifier: "agent-effort-picker",
            trigger: { EmptyView() }
        )
    }

    private var mcpPicker: some View {
        let launch = model.launch
        return McpServerPicker(
            servers: mcpServers,
            value: launch.mcpServerIds,
            onChange: { launch.mcpServerIds = $0 },
            open: $showMcpPicker,
            hideTrigger: true,
            trigger: { EmptyView() }
        )
    }

    private func loadMcpServers() async {
        guard let teamId = model.teamId else { return }
        let rows = (try? await McpServersApi(trpc: deps.trpc).list(accountId: accountId, teamId: teamId)) ?? []
        mcpServers = rows
        model.launch.mcpServerIds = McpServers.preselect(rows, saved: nil)
    }

    // MARK: - Leading

    /// EXP-1038: the subject chips left the card — they are the run's SUBJECT
    /// and head the page (`AgentComposerHeadline`). The leading slot is the
    /// starting row alone.
    @ViewBuilder
    private var leading: some View {
        // EXP-1271 (EXP-1259 D11): after Start the card LEADS with the
        // starting row until the send answers (after the pending delay).
        if let starting = model.starting, startingShown {
            ComposerStartingRow(starting: starting)
                .padding(.horizontal, 12)
                .padding(.top, 12)
        }
    }

    // MARK: - Return

    /// The return key ACCEPTS an open `/` menu's top prompt or an open
    /// `@`/`#`/`:` menu's top row (the row every other client's Enter takes)
    /// and otherwise submits.
    private func handleReturn() {
        if let prompt = model.slashPrompts.first, model.draftEditor.isEditing {
            model.pickPrompt(prompt.id)
            model.draftText = ""
            return
        }
        let editor = model.draftEditor
        if editor.showsAutocompleteMenu {
            if let member = editor.mentionCandidates.first {
                editor.applyMention(member)
            } else if let candidate = editor.issueRefCandidates.first {
                editor.applyIssueRef(candidate)
            } else if let record = editor.emojiCandidates.first {
                editor.applyEmoji(record)
            }
            return
        }
        model.submit()
    }

    // MARK: - Images

    /// Turn a photo pick into pending images: transcode anything the
    /// server's inline-image pipeline doesn't accept (notably HEIC) to JPEG,
    /// exactly as the share extension does, and cap the strip at four.
    private func ingestPhotos(_ items: [PhotosPickerItem]) async {
        defer { photoItems = [] }
        model.imageError = nil
        for item in items {
            guard !model.imagesFull else { break }
            guard let data = try? await item.loadTransferable(type: Data.self) else { continue }
            let type = item.supportedContentTypes.first
            // EXP-554: one shared normalizer for every composer.
            let outcome = AttachmentPicks.normalizedPhoto(
                data: data,
                contentTypeHint: type?.preferredMIMEType,
                filenameExtensionHint: type?.preferredFilenameExtension
            )
            guard let normalized = outcome.attachment else {
                model.imageError = outcome.failure
                continue
            }
            model.queueImage(normalized)
        }
    }

    /// Wave D: Files-app picks, ANY type — an image queues as an image, the
    /// rest as file tiles; over a cap = the one rejection copy.
    private func ingestFiles(_ urls: [URL]) async {
        model.imageError = nil
        for url in urls {
            let outcome = await AttachmentPicks.readPickedSteerFile(at: url)
            guard let picked = outcome.attachment else {
                model.imageError = outcome.failure
                continue
            }
            model.queuePicked(picked)
        }
    }

    /// EXP-802: a PASTED image joins the strip like a picked one — it can
    /// never become an image BLOCK: the draft is exactly one text block.
    private func ingestPastedImage(_ image: UIImage) {
        guard !model.imagesFull else { return }
        guard let data = image.jpegData(compressionQuality: 0.85) else { return }
        model.imageError = nil
        let outcome = AttachmentPicks.normalizedPhoto(
            data: data, contentTypeHint: "image/jpeg", filenameExtensionHint: "jpg"
        )
        guard let normalized = outcome.attachment else {
            model.imageError = outcome.failure
            return
        }
        model.queueImage(normalized)
    }
}

// MARK: - Starting (EXP-1271)

/// The composer card's starting row (feedback-states.json `starting`): the
/// mark, "Starting on <device>" and the seconds since the press, ticking in
/// tenths (web `StartingRow`).
struct ComposerStartingRow: View {
    let starting: AgentComposerModel.Starting

    var body: some View {
        HStack(spacing: 8) {
            FeedbackMark(size: CGFloat(FeedbackStates.Mark.inlineSize))
            Text(FeedbackStates.Starting.label(device: starting.deviceLabel))
                .font(.subheadline)
                .foregroundStyle(.white)
                .lineLimit(1)
                .truncationMode(.tail)
            Spacer(minLength: 8)
            TimelineView(.periodic(from: starting.since, by: 0.1)) { context in
                Text(Self.elapsed(context.date.timeIntervalSince(starting.since)))
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier(FeedbackStates.TestId.starting)
    }

    /// "1.4 s" (web `startingElapsed`).
    static func elapsed(_ seconds: TimeInterval) -> String {
        String(format: "%.1f s", max(seconds, 0))
    }
}

// MARK: - The headline

/// EXP-1038: the composer's HEADLINE — the run's subject as the page's main
/// element rather than a chip buried in the card. The contract verb
/// (`AgentComposerPrompt.headline`, ×4: "Implement") reads straight into the
/// chips beside it: the checked issues; a prompt's chip (NEW-2) IS the
/// headline (S-3: an agent pick is the agent row's, never a chip). The HOST
/// mounts this only with a subject: a chat's heading would repeat the field's
/// own "Ask the agent…" placeholder, so there is no row at all (web
/// `LaunchHeadline` returns null). The chips keep their remove affordances and their
/// identifiers (`agent-composer-chip-prompt`,
/// `agent-composer-chip-issue-<IDENT>`), so a pick still proves itself where
/// the screenshot suites look for it; only their place changed.
struct AgentComposerHeadline: View {
    let model: AgentComposerModel

    var body: some View {
        let issues = model.checkedOptions
        let subject = model.subject
        HStack(alignment: .center, spacing: 8) {
            if !model.headline.isEmpty {
                Text(model.headline)
                    .font(.title2.weight(.semibold))
                    .foregroundStyle(.white)
                    // The verb never shrinks under the chips: they scroll.
                    .fixedSize(horizontal: true, vertical: false)
                    .accessibilityIdentifier("agent-composer-headline-verb")
            }
            if subject != AgentComposerPrompt.Subject.none, subject != AgentComposerPrompt.Subject.agent {
                // Several issue chips scroll sideways rather than wrapping —
                // several issues must not push the field off the first screen.
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 6) {
                        switch subject {
                        case .prompt: promptChip
                        default: ForEach(issues) { issueChip($0) }
                        }
                    }
                    .padding(.vertical, 2)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-composer-headline")
    }

    /// One checked issue: the SHARED issue badge (EXP-885) — status glyph ·
    /// mono identifier · title · ✕, the same chip a `#EXP-1` ref wears in
    /// prose. EXP-827: only the ✕ removes (web and Android agree); the chip
    /// body is inert.
    private func issueChip(_ option: IssueOption) -> some View {
        let id = "agent-composer-chip-issue-\(option.identifier ?? option.id)"
        return IssueChip(
            identifier: option.identifier,
            title: option.title,
            status: IssueStatus.from(option.status),
            onRemove: { model.toggleIssue(option.id) }
        )
        .accessibilityIdentifier(id)
    }

    /// NEW-2: the one prompt: its curated glyph · name · ✕ (`Prompt…` while
    /// its row is not synced). Same split: the ✕ clears it, the body does
    /// nothing.
    private var promptChip: some View {
        let prompt = model.selectedPrompt
        let name = prompt?.name ?? "Prompt…"
        return GlassPill(
            name,
            mode: .readonly,
            leading: {
                AppIcon(ActionIconDisplay.iconName(for: prompt?.icon), size: 12)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            },
            trailing: { chipClose }
        )
        .overlay(alignment: .trailing) {
            chipRemoveButton(name: prompt?.name ?? "prompt", identifier: "agent-composer-chip-prompt-remove") {
                model.clearPrompt()
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-composer-chip-prompt")
    }

    /// The ✕ glyph a chip wears in its trailing slot. A readonly pill takes no
    /// hits, so the tap target is the overlay button below, sat over it.
    private var chipClose: some View {
        AppIcon(AppIcons.uiClose, size: 10)
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
    }

    /// The chip's ONE control: a clear 28pt button over the trailing ✕.
    private func chipRemoveButton(
        name: String,
        identifier: String,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Color.clear
                .frame(width: 28, height: 28)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Remove \(name)")
        .accessibilityIdentifier(identifier)
    }
}

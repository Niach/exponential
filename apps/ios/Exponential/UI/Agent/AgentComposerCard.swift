import ExpCore
import ExpUI
import PhotosUI
import SwiftUI
import UIKit

/// EXP-825: the ONE launcher's card — the same `GlassComposer` the steer and
/// comment composers wear. Slot order is leading · field · strip · tools:
///
/// - leading: under an action that declares inputs, its typed pick fields
///   (`ActionInputFieldsView`). EXP-1038: the subject chips are NOT here any
///   more — they head the page, beside the contract verb
///   (`AgentComposerHeadline`, mounted by the host above this card);
/// - field: the one-block markdown field with the `@` / `#` / `:` typeahead
///   (the host mounts `EditorAutocompleteMenu` under the card);
/// - strip: the pending images (the steer composer's tiles + markers) and,
///   since wave D, the pending files (a file tile each);
/// - tools: EXP-1249 — the ONE "+" (`ComposerPlusMenu`, a bottom sheet of the
///   `composer-menu.json` rows: Implement issue › · Run action › · Add file or
///   image | Effort › · Subagents › · Ultracode | MCP servers › · Computer
///   use); the submit is the round send GLYPH (EXP-827: icon-only
///   on every client), whose contract title ("Start chat" / "Start coding" /
///   "Start batch · N" / "Run action") is its accessibility name.
struct AgentComposerCard: View {
    let model: AgentComposerModel
    /// Where the composer routes a tapped `#EXP-1` chip.
    let onIssueRefTap: (String) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var showIssuePicker = false
    @State private var showActionPicker = false
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
    @State private var showSubagentPicker = false
    @State private var showMcpPicker = false
    /// The team's MCP servers (`mcpServers.list`), read once per team.
    @State private var mcpServers: [McpServerRow] = []

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
                maxHeight: 160
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
            if let imageError = model.imageError {
                Text(imageError)
                    .font(.caption2)
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
                ComposerPlusMenu(model: model, mcpServers: mcpServers) { id in
                    menuPick = id
                    showMenu = false
                }
            }
            .background {
                AgentIssuePickerSheet(model: model, isPresented: $showIssuePicker)
            }
            .background {
                AgentActionPickerSheet(model: model, isPresented: $showActionPicker)
            }
            .background { effortPicker }
            .background { subagentPicker }
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
            // already say what a send starts, and the contract's per-subject
            // label (`Start coding` / `Start batch · 3` / `Run action`) stays
            // the button's NAME for VoiceOver. Same
            // `GlassComposerSubmitButton` the comment and steer composers wear.
            GlassComposerSubmitButton(
                AppIcons.uiSubmit,
                accessibilityLabel: model.submitTitle,
                enabled: model.canSubmit
            ) {
                model.submit()
            }
            .accessibilityIdentifier("agent-composer-submit")
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-composer")
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
        case .implementIssue: showIssuePicker = true
        case .runAction: showActionPicker = true
        case .addFile:
            var transaction = Transaction()
            transaction.disablesAnimations = true
            withTransaction(transaction) { showAttachMenu = true }
        case .effort: showEffortPicker = true
        case .subagents: showSubagentPicker = true
        case .mcpServers: showMcpPicker = true
        case .ultracode, .computerUse: break
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

    private var subagentPicker: some View {
        let launch = model.launch
        return GlassPicker(
            items: LaunchVocabulary.subagentModelValues().map {
                PickerItem(value: $0, label: LaunchVocabulary.subagentModelLabel($0))
            },
            mode: .single,
            value: [launch.subagentModel],
            onChange: { picked in if let value = picked.first { launch.subagentModel = value } },
            title: "Subagents",
            open: $showSubagentPicker,
            hideTrigger: true,
            sheetIdentifier: "agent-subagent-picker",
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

    // MARK: - Action inputs

    /// EXP-1038: the subject chips left the card — they are the run's SUBJECT
    /// and now head the page (`AgentComposerHeadline`). What stays in the
    /// leading slot is the picked action's typed pick fields, which belong
    /// with the field they are filled next to.
    @ViewBuilder
    private var leading: some View {
        if let action = model.selectedAction, let inputs = action.inputs, !inputs.isEmpty {
            ActionInputFieldsView(model: model, inputs: inputs)
                .padding(.horizontal, 12)
                .padding(.top, 12)
        }
    }

    // MARK: - Return

    /// The return key ACCEPTS an open `@`/`#`/`:` menu's top row (the row
    /// every other client's Enter takes) and otherwise submits.
    private func handleReturn() {
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
    /// rest as file tiles; over a cap = the one rejection copy. F6: a device
    /// without `steer-files` takes images only; a file pick says why
    /// (`SteerFiles.needsNewerDeviceNotice`) and is dropped.
    private func ingestFiles(_ urls: [URL]) async {
        model.imageError = nil
        let caps = model.device?.caps
        for url in urls {
            let outcome = await AttachmentPicks.readPickedSteerFile(at: url)
            guard let picked = outcome.attachment else {
                model.imageError = outcome.failure
                continue
            }
            guard SteerFiles.accepts(contentType: picked.contentType, caps: caps) else {
                model.imageError = SteerFiles.needsNewerDeviceNotice
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

// MARK: - The headline

/// EXP-1038: the composer's HEADLINE — the run's subject as the page's main
/// element rather than a chip buried in the card. The contract verb
/// (`AgentComposerPrompt.headline`, ×4: "Run" · "Implement") reads straight
/// into the chips beside it: one action chip, or the checked issues. The HOST
/// mounts this only with a subject: a chat's heading would repeat the field's
/// own "Ask the agent…" placeholder, so there is no row at all (web
/// `LaunchHeadline` returns null). The chips keep their remove affordances and their
/// identifiers (`agent-composer-chip-action`,
/// `agent-composer-chip-issue-<IDENT>`), so a pick still proves itself where
/// the screenshot suites look for it; only their place changed.
///
/// EXP-1233: the Fix merge conflicts builtin with a PICKED pull request is
/// the third verb — "Fix merge conflicts" leads and the chips are the PR's
/// OWN issue chips (the ones "Implement" draws, a batch PR's several), so a
/// refused merge lands on a headline that names the work. Their ✕ clears the
/// PICK (back to "Run" + the action chip, the card's picker), never the action.
struct AgentComposerHeadline: View {
    let model: AgentComposerModel

    var body: some View {
        let issues = model.checkedOptions
        let action = model.selectedAction
        let fixPr = model.fixConflictsPr
        HStack(alignment: .center, spacing: 8) {
            Text(model.headline)
                .font(.title2.weight(.semibold))
                .foregroundStyle(.white)
                // The verb never shrinks under the chips: they scroll.
                .fixedSize(horizontal: true, vertical: false)
                .accessibilityIdentifier("agent-composer-headline-verb")
            if action != nil || !issues.isEmpty {
                // Several issue chips scroll sideways rather than wrapping —
                // a batch must not push the field off the first screen.
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 6) {
                        if let fixPr {
                            ForEach(fixPr.issues) { prIssueChip($0) }
                        } else if let action {
                            actionChip(action)
                        } else {
                            ForEach(issues) { issueChip($0) }
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

    /// EXP-1233: one of the picked pull request's issues — the SAME chip and
    /// identifiers as a checked issue's; the ✕ clears the pull request.
    private func prIssueChip(_ issue: FixConflictsPr.Issue) -> some View {
        let id = "agent-composer-chip-issue-\(issue.identifier ?? issue.id)"
        return IssueChip(
            identifier: issue.identifier,
            title: issue.title,
            status: IssueStatus.from(issue.status),
            onRemove: { model.clearPullRequest() },
            removeLabel: "Clear the pull request",
            removeIdentifier: "\(id)-remove"
        )
        .accessibilityIdentifier(id)
    }

    /// The one action: its curated glyph · name · ✕. Same split: the ✕ clears
    /// the action, the body does nothing.
    private func actionChip(_ action: ActionDto) -> some View {
        GlassPill(
            action.name,
            mode: .readonly,
            leading: {
                AppIcon(action.icon ?? AppIcons.actionDefault, size: 12)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            },
            trailing: { chipClose }
        )
        .overlay(alignment: .trailing) {
            chipRemoveButton(name: action.name, identifier: "agent-composer-chip-action-remove") {
                model.clearAction()
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-composer-chip-action")
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

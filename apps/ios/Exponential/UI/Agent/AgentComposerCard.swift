import ExpCore
import ExpUI
import PhotosUI
import SwiftUI
import UIKit

/// EXP-825: the ONE launcher's card — the same `GlassComposer` the steer and
/// comment composers wear. Slot order is leading · field · strip · tools:
///
/// - leading: the subject chips (issue chips OR one action chip — tapping a
///   chip removes it) and, under an action that declares inputs, its typed
///   pick fields (`ActionInputFieldsView`);
/// - field: the one-block markdown field with the `@` / `#` / `:` typeahead
///   (the host mounts `EditorAutocompleteMenu` under the card);
/// - strip: the pending images (the steer composer's tiles + markers);
/// - tools: `#` opens the issue picker, ▶ the action picker, the image glyph
///   the photo picker; the submit is the round send GLYPH (EXP-827: icon-only
///   on every client), whose contract title ("Start chat" / "Start coding" /
///   "Start batch · N" / "Run action") is its accessibility name.
struct AgentComposerCard: View {
    let model: AgentComposerModel
    /// Where the composer routes a tapped `#EXP-1` chip.
    let onIssueRefTap: (String) -> Void

    @State private var showIssuePicker = false
    @State private var showActionPicker = false
    @State private var showPhotoPicker = false
    @State private var photoItems: [PhotosPickerItem] = []

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
            if !model.pendingImages.isEmpty {
                PendingAttachmentStrip(items: model.pendingImages) { id in
                    model.removeImage(id: id)
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
            // EXP-825 ×4: `#` for issues, ▶ for actions, the image glyph
            // every other composer wears. Each sheet hangs off ITS button —
            // stacking presentations on one node is where SwiftUI starts
            // dropping them.
            GlassComposerToolButton(AppIcons.editorIssueRef, accessibilityLabel: "Pick issues") {
                showIssuePicker = true
            }
            .accessibilityIdentifier("agent-composer-issues-button")
            .sheet(isPresented: $showIssuePicker) {
                AgentIssuePickerSheet(model: model)
            }

            GlassComposerToolButton(AppIcons.actionRun, accessibilityLabel: "Pick an action") {
                showActionPicker = true
            }
            .accessibilityIdentifier("agent-composer-actions-button")
            .sheet(isPresented: $showActionPicker) {
                AgentActionPickerSheet(model: model)
            }

            // EXP-850 §13: the steer composers attach with the `ui-add` (plus)
            // concept ×4; `editor-image` stays the comment/description glyph.
            GlassComposerToolButton(
                AppIcons.uiAdd,
                accessibilityLabel: "Attach image",
                enabled: !model.attachFull && !model.sending
            ) {
                showPhotoPicker = true
            }
            .accessibilityIdentifier("agent-composer-image-button")
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
    }

    // MARK: - Subject chips

    @ViewBuilder
    private var leading: some View {
        let issues = model.checkedOptions
        let action = model.selectedAction
        if !issues.isEmpty || action != nil {
            VStack(alignment: .leading, spacing: 8) {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 6) {
                        if let action {
                            actionChip(action)
                        } else {
                            ForEach(issues) { issueChip($0) }
                        }
                    }
                }
                if let action, let inputs = action.inputs, !inputs.isEmpty {
                    ActionInputFieldsView(model: model, inputs: inputs)
                }
            }
            .padding(.horizontal, 12)
            .padding(.top, 12)
        }
    }

    /// One checked issue: status glyph · mono identifier · ✕. EXP-827: only
    /// the ✕ removes (web and Android agree); the chip body is inert.
    private func issueChip(_ option: IssueOption) -> some View {
        let name = option.identifier ?? option.title
        let id = "agent-composer-chip-issue-\(option.identifier ?? option.id)"
        return GlassPill(
            name,
            mode: .readonly,
            leading: {
                AppIcon(IssueStatus.from(option.status).iconName, size: 12)
                    .foregroundStyle(IssueStatus.from(option.status).color)
            },
            trailing: { chipClose }
        )
        .overlay(alignment: .trailing) {
            chipRemoveButton(name: name, identifier: "\(id)-remove") {
                model.toggleIssue(option.id)
            }
        }
        .accessibilityElement(children: .contain)
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
            guard !model.attachFull else { break }
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

    /// EXP-802: a PASTED image joins the strip like a picked one — it can
    /// never become an image BLOCK: the draft is exactly one text block.
    private func ingestPastedImage(_ image: UIImage) {
        guard !model.attachFull else { return }
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

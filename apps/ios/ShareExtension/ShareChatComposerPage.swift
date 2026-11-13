#if !SHARE_EXTENSION
import ExpCore
import ExpUI
#endif
import SwiftUI
import UIKit

/// What the composer reads off the mirror.
struct ShareComposerContext {
    /// The team a Temporary session starts in.
    let teamId: String?
    let agents: [MirroredAgent]
    let me: String?
    /// The team's machines, the default first (`startDevices`).
    let devices: [MirroredDevice]
    /// A direct share into an existing chat: no agent row, no device.
    let chat: MirroredChat?
}

/// EXP-1635 (share-targets.json), STEP 2 for `new-chat` or an agent: THE
/// composer (composer-home.json: the agent row over the card) inside the
/// extension, prefilled with `shareChatText`, the share as its pending
/// attachments. Nothing is sent until Send. The card is ExpUI's
/// `GlassComposer`; the field is a plain text field (the markdown field and
/// its typeaheads live in the app) and the `+` holds the one row a share
/// needs: Device, while the pick is Temporary session.
struct ShareChatComposerPage: View {
    let context: ShareComposerContext
    /// Uploads and sends; throws what the page shows inline. Returns once the
    /// message is out.
    let onSend: @MainActor (_ destination: ShareChatDestination, _ text: String, _ attachments: [ShareAttachment]) async throws -> Void
    let onComplete: () -> Void

    @State private var pick: String
    @State private var draft: String
    @State private var attachments: [ShareAttachment]
    @State private var deviceId: String?
    @State private var showPlus = false
    @State private var sending = false
    @State private var error: String?
    /// A share beyond a cap (inline, composer-menu.json `add-file`).
    @State private var capNote: String?

    init(
        context: ShareComposerContext,
        pick: String,
        payload: SharedPayload,
        onSend: @escaping @MainActor (ShareChatDestination, String, [ShareAttachment]) async throws -> Void,
        onComplete: @escaping () -> Void
    ) {
        self.context = context
        self.onSend = onSend
        self.onComplete = onComplete
        _pick = State(initialValue: pick)
        _deviceId = State(initialValue: context.devices.first?.deviceId)
        var text = ShareTargets.shareChatText(
            text: payload.text, subject: payload.subject, url: payload.url, hasImages: !payload.images.isEmpty
        )
        var pending: [ShareAttachment] = []
        // The composer's own caps; each image drops its `[Image #k]` marker
        // at the end of the draft, like a picked one.
        for (index, image) in payload.images.prefix(AgentComposerPrompt.maxImages).enumerated() {
            pending.append(ShareAttachment(
                data: image.data, filename: image.filename, contentType: image.contentType, isImage: true
            ))
            text = SteerImageMessage.insertImageMarker(
                text: text, caret: (text as NSString).length, index: index + 1
            ).text
        }
        for file in payload.files.prefix(SteerImageMessage.maxFiles) {
            pending.append(ShareAttachment(
                data: file.data, filename: file.filename, contentType: file.contentType, isImage: false
            ))
        }
        _draft = State(initialValue: text)
        _attachments = State(initialValue: pending)
        _capNote = State(initialValue: SteerImageMessage.capNote(
            images: payload.images.count, files: payload.files.count, moreFiles: payload.moreFiles
        ))
    }

    private var rowState: ComposerAgentRow.State<MirroredAgent> {
        ComposerAgentRow.composerAgentRow(agents: context.agents, me: context.me, picked: pick)
    }

    private var device: MirroredDevice? {
        context.devices.first { $0.deviceId == deviceId } ?? context.devices.first
    }

    private var isTemporary: Bool { context.chat == nil && rowState.isTemporary }

    private var destination: ShareChatDestination? {
        if let chat = context.chat { return .chat(chat.root) }
        if let agent = rowState.agent { return .agent(id: agent.id, teamId: agent.teamId, conversation: agent.conversation) }
        guard let device, let teamId = context.teamId else { return nil }
        return .newChat(teamId: teamId, deviceId: device.deviceId, rowIds: device.rowIds)
    }

    private var hasContent: Bool {
        !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !attachments.isEmpty
    }

    private var canSend: Bool {
        destination != nil && hasContent && !sending && AgentComposerPrompt.withinLimit(draft)
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: CGFloat(ComposerAgentRow.rowToCard)) {
                if context.chat == nil {
                    ComposerAgentRowView(
                        label: rowState.label,
                        items: rowState.agents.map { agent in
                            ComposerAgentRowView.Item(id: agent.id, name: agent.name, glyph: agent.bubbleGlyph)
                        } + [.temporary],
                        selected: rowState.selected,
                        onSelect: { pick = $0 }
                    )
                }
                card
                ForEach(notes, id: \.self) { note in
                    Text(note)
                        .font(.system(size: CGFloat(FeedbackStates.Errors.inlineSize)))
                        .foregroundStyle(DesignTokens.Semantic.red)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 4)
                }
            }
            .padding(16)
        }
        .background(DesignTokens.Palette.background)
        .navigationTitle(context.chat?.title ?? ShareTargets.Copy.newChat)
        .navigationBarTitleDisplayMode(.inline)
        .sheet(isPresented: $showPlus) {
            ShareComposerPlusSheet(devices: context.devices, deviceId: $deviceId)
        }
    }

    /// What stands under the card: the send's failure, the composer's "no
    /// machine" note (the app's copy), a cap.
    private var notes: [String] {
        var out = [capNote, error].compactMap(\.self)
        if isTemporary, context.devices.isEmpty {
            out.append("No desktop online. Open the Exponential desktop app to start a session.")
        }
        return out
    }

    private var card: some View {
        GlassComposer {
            TextField(HomeGreeting.placeholder, text: $draft, axis: .vertical)
                .lineLimit(3...8)
                .foregroundStyle(.white)
                .padding(.horizontal, 12)
                .padding(.top, 12)
                .padding(.bottom, 6)
                .accessibilityIdentifier("agent-composer-field")
        } strip: {
            if !attachments.isEmpty {
                strip
                    .padding(.horizontal, 12)
                    .padding(.bottom, 8)
            }
        } tools: {
            if isTemporary {
                GlassComposerToolButton(AppIcons.uiAdd, accessibilityLabel: ComposerMenu.plusLabel) {
                    showPlus = true
                }
                .accessibilityIdentifier(ComposerMenu.plusTestId)
            }
        } submit: {
            GlassComposerSubmitButton(
                AppIcons.uiSubmit,
                accessibilityLabel: AgentComposerPrompt.submitTitle(for: isTemporary ? .none : .agent),
                enabled: canSend,
                pending: sending,
                action: send
            )
            .accessibilityIdentifier("agent-composer-submit")
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-composer")
    }

    private var strip: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(attachments) { attachment in
                    tile(attachment)
                }
            }
        }
    }

    private func tile(_ attachment: ShareAttachment) -> some View {
        Group {
            if attachment.isImage, let image = UIImage(data: attachment.data) {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFill()
            } else {
                AppIcon(AppIcons.uiFile, size: 20)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(GlassTokens.fillCard)
            }
        }
        .frame(width: 56, height: 56)
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .overlay(alignment: .topTrailing) {
            Button {
                remove(attachment)
            } label: {
                AppIcon(AppIcons.uiClose, size: 10)
                    .foregroundStyle(.white)
                    .frame(width: 20, height: 20)
                    .background(.black.opacity(0.6), in: Circle())
                    .padding(2)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Remove \(attachment.filename)")
        }
    }

    /// Dropping an image renumbers the markers, like the app's strip.
    private func remove(_ attachment: ShareAttachment) {
        let images = attachments.filter(\.isImage)
        if let position = images.firstIndex(where: { $0.id == attachment.id }) {
            draft = SteerImageMessage.renumberImageMarkers(draft, removedIndex: position + 1)
        }
        attachments.removeAll { $0.id == attachment.id }
    }

    private func send() {
        guard canSend, let destination else { return }
        sending = true
        error = nil
        Task {
            do {
                try await onSend(destination, draft, attachments)
                onComplete()
            } catch {
                self.error = error.localizedDescription
                sending = false
            }
        }
    }
}

/// The extension's `+`: THE Device row (composer-menu.json `device`) over the
/// picker rows, the bottom sheet of a phone (the selected row's check
/// trailing).
private struct ShareComposerPlusSheet: View {
    let devices: [MirroredDevice]
    @Binding var deviceId: String?

    @Environment(\.dismiss) private var dismiss

    private var current: MirroredDevice? {
        devices.first { $0.deviceId == deviceId } ?? devices.first
    }

    var body: some View {
        NavigationStack {
            List {
                NavigationLink {
                    picker
                } label: {
                    HStack(spacing: 12) {
                        AppIcon(AppIcons.conceptOrName(ComposerMenu.row(.device).icon) ?? AppIcons.uiDevice, size: 17)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        Text(ComposerMenu.row(.device).label)
                            .foregroundStyle(.white)
                        Spacer(minLength: 8)
                        Text(current?.label ?? "")
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .lineLimit(1)
                    }
                }
                .accessibilityIdentifier(ComposerMenu.rowTestId(.device))
                .listRowBackground(GlassTokens.opaqueCardFill)
            }
            .scrollContentBackground(.hidden)
            .background(DesignTokens.Palette.background)
            .navigationTitle(ComposerMenu.plusLabel)
            .navigationBarTitleDisplayMode(.inline)
        }
        .presentationDetents([.medium])
        .preferredColorScheme(.dark)
    }

    private var picker: some View {
        List(devices) { device in
            Button {
                deviceId = device.deviceId
                dismiss()
            } label: {
                HStack(spacing: 12) {
                    AppIcon(device.glyph, size: 17)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    Text(device.label)
                        .foregroundStyle(.white)
                    Spacer(minLength: 8)
                    if device.deviceId == current?.deviceId {
                        AppIcon(AppIcons.uiCheck, size: 15)
                            .foregroundStyle(.white)
                    }
                }
            }
            .listRowBackground(GlassTokens.opaqueCardFill)
        }
        .scrollContentBackground(.hidden)
        .background(DesignTokens.Palette.background)
        .navigationTitle(ComposerMenu.row(.device).label)
        .navigationBarTitleDisplayMode(.inline)
    }
}

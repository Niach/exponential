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
    @State private var strip: ShareComposerStrip
    @State private var deviceId: String?
    @State private var showPlus = false
    @State private var sending = false
    @State private var error: String?

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
        _strip = State(initialValue: ShareComposerStrip(payload: payload))
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

    /// #EXP-2031: an agent pick sends into that agent's own chat, so the
    /// page carries its name; only Temporary session is a new chat.
    private var title: String {
        context.chat?.title ?? rowState.agent?.name ?? ShareTargets.Copy.newChat
    }

    private var hasContent: Bool {
        !strip.draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !strip.attachments.isEmpty
    }

    private var canSend: Bool {
        destination != nil && hasContent && !sending && AgentComposerPrompt.withinLimit(strip.draft)
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
        .navigationTitle(title)
        .navigationBarTitleDisplayMode(.inline)
        .sheet(isPresented: $showPlus) {
            ShareComposerPlusSheet(devices: context.devices, deviceId: $deviceId)
        }
        .onAppear { strip.releaseToasts() }
    }

    /// What stands under the card: the send's failure and the composer's "no
    /// machine" note (the app's copy).
    private var notes: [String] {
        var out = [error].compactMap(\.self)
        if isTemporary, context.devices.isEmpty {
            out.append("No desktop online. Open the Exponential desktop app to start a session.")
        }
        return out
    }

    private var card: some View {
        GlassComposer {
            TextField(HomeGreeting.placeholder, text: $strip.draft, axis: .vertical)
                .lineLimit(3...8)
                .foregroundStyle(.white)
                .padding(.horizontal, 12)
                .padding(.top, 12)
                .padding(.bottom, 6)
                .accessibilityIdentifier("agent-composer-field")
        } strip: {
            if !strip.attachments.isEmpty {
                thumbnails
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

    private var thumbnails: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(strip.attachments) { attachment in
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
                strip.removeAttachment(id: attachment.id)
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

    private func send() {
        guard canSend, let destination else { return }
        sending = true
        error = nil
        Task {
            do {
                try await onSend(destination, strip.draft, strip.attachments)
                onComplete()
            } catch {
                self.error = error.localizedDescription
                sending = false
            }
        }
    }
}

/// #EXP-1845: the share as THE composer strip (`ComposerAttachmentStaging`,
/// composer-attachments.json): every shared image drops its `[Image #N]` at
/// the end of the prefilled text, the caps and the rejection toast are the app
/// composer's, and the thumbnail's X renumbers like the app's.
@MainActor
@Observable
final class ShareComposerStrip: ComposerAttachmentStaging {
    var draft: String
    private(set) var images: [ShareAttachment] = []
    private(set) var files: [ShareAttachment] = []
    /// The share's toasts, held until the page is on screen.
    @ObservationIgnored private var heldToasts: [String]? = []

    var attachments: [ShareAttachment] { images + files }

    init(payload: SharedPayload) {
        draft = ShareTargets.shareChatText(
            text: payload.text, subject: payload.subject, url: payload.url, hasImages: !payload.images.isEmpty
        )
        // A share is one pick: its images, its files, then what the
        // extractor left out (unread past the file cap, over the size limit).
        var picks: [ComposerPick<ShareAttachment>] = payload.images.map { image in
            image.data.count > AttachmentFiles.maxImageUploadBytes ? .rejected : .image(ShareAttachment(
                data: image.data, filename: image.filename, contentType: image.contentType, isImage: true
            ))
        }
        picks += payload.files.map { file in
            .file(ShareAttachment(data: file.data, filename: file.filename, contentType: file.contentType, isImage: false))
        }
        // Files the extractor left unread past the cap: one more file pick,
        // over the cap by construction, so the file-cap toast fires.
        if payload.moreFiles { picks.append(.file(ShareAttachment(data: Data(), filename: "", contentType: "", isImage: false))) }
        picks += Array(repeating: .rejected, count: payload.rejected)
        stageAttachments(picks)
    }

    func releaseToasts() {
        guard let held = heldToasts else { return }
        heldToasts = nil
        showAttachmentToasts(held)
    }

    // MARK: ComposerAttachmentStaging

    var stagingText: String { draft }
    /// The plain field reports no caret: a share's markers go at the end.
    var stagingCaret: Int? { nil }
    func setStagingText(_ text: String, caret _: Int) { draft = text }
    var stagedImageIds: [UUID] { images.map(\.id) }
    var stagedFileCount: Int { files.count }

    func appendStaged(_ attachment: ShareAttachment, as kind: ComposerAttachments.Kind) {
        switch kind {
        case .image: images.append(attachment)
        case .file: files.append(attachment)
        }
    }

    func removeStaged(id: UUID) {
        images.removeAll { $0.id == id }
        files.removeAll { $0.id == id }
    }

    func showAttachmentToasts(_ messages: [String]) {
        if heldToasts != nil {
            heldToasts?.append(contentsOf: messages)
        } else {
            for message in messages {
                Toaster.shared.show(ToastItem(kind: .error, title: message))
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

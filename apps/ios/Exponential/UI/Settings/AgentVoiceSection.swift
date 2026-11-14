import ExpCore
import ExpUI
import SwiftUI
import UniformTypeIdentifiers

// #EXP-1847 (agent-settings.json `voice`): EVERY agent's Voice group, right
// after Triggers: Language (THE single-select Picker row over voice.json
// `languages`), Voice (the clip: its value `agentVoiceOf(...).clipLabel`, a
// press opens the file picker for a WAV and uploads it through the clip
// route; a set clip offers the trailing Remove), Transcript and Wake phrase
// (THE input rows, saved on leave or submit; the wake phrase's error inline
// under the group). Every value reads `agentVoiceOf(agent, member)`; a write
// is `agents.update {voice}` (a cleared key = null), the synced row answers.
// The Assistant draws its member rows (push to talk, wake word, meetings)
// below it, unchanged.
struct AgentVoiceSection: View {
    let agentId: String
    let agentName: String
    let role: String?
    /// The synced `agents.voice`.
    let voice: AgentVoice
    /// The member's `voice.get` settings; nil = not loaded (the defaults).
    let member: Voice.Settings?
    /// A team agent's owners, the Assistant's member.
    let canEdit: Bool

    private typealias Copy = AgentSettings.VoiceRows.Copy

    private enum Input: Hashable { case transcript, wakePhrase }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster

    @FocusState private var focused: Input?
    @State private var transcript = ""
    @State private var wakePhrase = ""
    @State private var wakePhraseError: String?
    /// The language pick while its write is in flight.
    @State private var languagePending: String?
    @State private var clipBusy = false
    @State private var fileImporter = false

    private var resolved: AgentSettings.ResolvedVoice {
        AgentSettings.agentVoiceOf(role: role, voice: voice, member: member ?? Voice.defaults)
    }

    var body: some View {
        Section {
            languageRow
            clipRow
            inputRow(.transcript)
            inputRow(.wakePhrase)
        } header: {
            GlassSectionHeader(Copy.label)
        } footer: {
            if let wakePhraseError {
                Text(wakePhraseError)
                    .font(.system(size: CGFloat(FeedbackStates.Errors.inlineSize)))
                    .foregroundStyle(DesignTokens.Semantic.red)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("agent-voice-wake-phrase-error")
            }
        }
        .listRowBackground(glassFormRowFill)
        .onAppear { reseed(.transcript); reseed(.wakePhrase) }
        // The synced row is the truth once the field is left.
        .onChange(of: resolved) { _, _ in
            if focused != .transcript { reseed(.transcript) }
            if focused != .wakePhrase { reseed(.wakePhrase) }
            languagePending = nil
        }
    }

    // MARK: - Language

    private var languageRow: some View {
        GlassPickerRow(
            Copy.language,
            selection: Binding(
                get: { languagePending ?? resolved.language },
                set: { saveLanguage($0) }
            ),
            options: Voice.languages.map(\.value),
            label: { value in Voice.languages.first { $0.value == value }?.label ?? value },
            enabled: canEdit
        )
        .accessibilityIdentifier("agent-voice-language")
    }

    private func saveLanguage(_ next: String) {
        guard canEdit, next != (languagePending ?? resolved.language) else { return }
        languagePending = next
        Task {
            if await !write(AgentVoicePatch(language: .some(next)), thing: "the language") {
                languagePending = nil
            }
        }
    }

    // MARK: - Clip

    private var clipRow: some View {
        HStack(spacing: 8) {
            Button {
                fileImporter = true
            } label: {
                GlassPickerRowLabel(Copy.clip, value: resolved.clipLabel, enabled: canEdit)
            }
            .buttonStyle(.plain)
            .disabled(!canEdit || clipBusy)
            .accessibilityLabel(Copy.clipUpload)
            .accessibilityValue(resolved.clipLabel)
            .accessibilityIdentifier("agent-voice-clip")
            if clipBusy {
                ProgressView()
                    .controlSize(.mini)
                    .tint(.white)
            } else if canEdit, voice.clipId?.isEmpty == false {
                Button(action: removeClip) {
                    AppIcon(AppIcons.uiDelete, size: 16)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .frame(width: 28, height: 28)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Copy.clipRemove)
                .accessibilityIdentifier("agent-voice-clip-remove")
            }
        }
        .background {
            Color.clear
                .fileImporter(isPresented: $fileImporter, allowedContentTypes: [.wav]) { result in
                    guard case let .success(url) = result else { return }
                    uploadClip(url)
                }
        }
    }

    /// Size first (never buffer what the cap rejects), then the bytes; the
    /// route checks the WAV header and answers the clip.
    private func uploadClip(_ url: URL) {
        guard canEdit, !clipBusy else { return }
        clipBusy = true
        Task {
            defer { clipBusy = false }
            let read: Data? = await Task.detached {
                let scoped = url.startAccessingSecurityScopedResource()
                defer { if scoped { url.stopAccessingSecurityScopedResource() } }
                let size = (try? url.resourceValues(forKeys: [.fileSizeKey]))?.fileSize ?? 0
                guard size <= AgentSettings.VoiceRows.ClipUpload.maxBytes else { return Data() }
                return try? Data(contentsOf: url)
            }.value
            guard let read else {
                clipFailed(verb: "upload", description: nil)
                return
            }
            guard !read.isEmpty, read.count <= AgentSettings.VoiceRows.ClipUpload.maxBytes else {
                clipFailed(verb: "upload", description: Copy.Errors.clipSize)
                return
            }
            guard url.pathExtension.lowercased() == "wav" || UTType(filenameExtension: url.pathExtension)?.conforms(to: .wav) == true else {
                clipFailed(verb: "upload", description: Copy.Errors.clipType)
                return
            }
            do {
                _ = try await deps.attachmentsApi.uploadAgentVoiceClip(
                    accountId: accountId,
                    agentId: agentId,
                    data: read,
                    filename: AttachmentFiles.sanitizedFilename(url.lastPathComponent)
                )
            } catch {
                clipFailed(verb: "upload", description: error.trpcUserMessage)
            }
        }
    }

    private func removeClip() {
        guard canEdit, !clipBusy else { return }
        clipBusy = true
        Task {
            defer { clipBusy = false }
            do {
                try await deps.attachmentsApi.removeAgentVoiceClip(accountId: accountId, agentId: agentId)
            } catch {
                clipFailed(verb: "remove", description: error.trpcUserMessage)
            }
        }
    }

    private func clipFailed(verb: String, description: String?) {
        toaster.error(FeedbackStates.Errors.title(verb: verb, thing: "the voice clip"), description: description)
    }

    // MARK: - Transcript + wake phrase

    private func title(_ input: Input) -> String {
        input == .transcript ? Copy.transcript : Copy.wakePhrase
    }

    private func placeholder(_ input: Input) -> String {
        input == .transcript ? Copy.transcriptPlaceholder : AgentSettings.VoiceRows.wakePhrasePlaceholder(agentName)
    }

    private func limit(_ input: Input) -> Int {
        input == .transcript ? AgentSettings.VoiceRows.Limits.transcript : AgentSettings.VoiceRows.Limits.wakePhrase
    }

    private func stored(_ input: Input) -> String {
        input == .transcript ? resolved.transcript : resolved.wakePhrase
    }

    private func text(_ input: Input) -> Binding<String> {
        Binding(
            get: { input == .transcript ? transcript : wakePhrase },
            set: { next in
                // #NEW-75: THE input rule ×4 (control characters dropped,
                // cut to the server's UTF-16 limit).
                let clean = Voice.textInput(next, max: limit(input))
                if input == .transcript {
                    transcript = clean
                } else {
                    if clean != wakePhrase { wakePhraseError = nil }
                    wakePhrase = clean
                }
            }
        )
    }

    private func reseed(_ input: Input) {
        if input == .transcript { transcript = stored(.transcript) } else { wakePhrase = stored(.wakePhrase) }
    }

    /// The input row (label + trailing field, the Voice page's rows).
    private func inputRow(_ input: Input) -> some View {
        HStack(spacing: 8) {
            Text(title(input))
                .foregroundStyle(.white.opacity(canEdit ? TextOpacity.primary : TextOpacity.quaternary))
            Spacer(minLength: 8)
            GlassTextField(
                placeholder(input),
                text: text(input),
                bordered: false,
                accessibilityIdentifier: input == .transcript ? "agent-voice-transcript" : "agent-voice-wake-phrase"
            )
            .multilineTextAlignment(.trailing)
            .autocorrectionDisabled()
            .submitLabel(.done)
            .focused($focused, equals: input)
            .disabled(!canEdit)
            .onSubmit { commit(input) }
            .onChange(of: focused) { old, new in
                if old == input, new != input { commit(input) }
            }
        }
    }

    /// Leave or submit: an invalid wake phrase shows its line and is never
    /// sent; an unchanged value sends nothing; an empty one clears the key.
    private func commit(_ input: Input) {
        guard canEdit else { return }
        let trimmed = (input == .transcript ? transcript : wakePhrase)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        if input == .wakePhrase, let error = Voice.wakePhraseError(trimmed) {
            wakePhraseError = error
            return
        }
        guard trimmed != stored(input) else { return }
        let value: String?? = .some(trimmed.isEmpty ? nil : trimmed)
        let patch = input == .transcript ? AgentVoicePatch(transcript: value) : AgentVoicePatch(wakePhrase: value)
        Task {
            if await !write(patch, thing: input == .transcript ? "the transcript" : "the wake phrase") {
                reseed(input)
            }
        }
    }

    /// One `agents.update {voice}`; a refusal toasts (false).
    private func write(_ patch: AgentVoicePatch, thing: String) async -> Bool {
        do {
            try await deps.agentsApi.update(accountId: accountId, id: agentId, patch: AgentPatch(voice: patch))
            return true
        } catch {
            toaster.error(FeedbackStates.Errors.title(verb: "save", thing: thing), description: error.trpcUserMessage)
            return false
        }
    }
}

import ExpCore
import ExpUI
import GRDB
import SwiftUI

// NEW-67 (voice plan v3, V1): Settings → Personal → Voice (`voice.json`, the
// same page on web, the IDE and Android; on iOS it hangs off the account
// page, the phone's Personal settings). ONE group (Voice device, Agent, Speak
// replies, Language read through `languageLabel`, and while a voice device is
// picked Voice file + Transcript, #NEW-73), then while a voice device is
// picked the tray group under `trayHeader` (Push to talk, Shortcut, Wake
// word, Wake phrase). Every switch reads off and is disabled while the device
// or the agent is None. Each change saves at once with `voice.update`
// (optimistic; a refusal reverts and toasts, feedback-states.json).
struct VoiceSettingsPage: View {
    let accountId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.toaster) private var toaster

    /// nil until `voice.get` landed.
    @State private var settings: Voice.Settings?
    @State private var loadFailed = false
    @State private var wakePhrase = ""
    @State private var voiceFile = ""
    @State private var voiceTranscript = ""
    /// The pending debounced save per input row.
    @State private var textSaves: [TextInput: Task<Void, Never>] = [:]
    /// Last write wins: only the newest `voice.update` may settle or revert.
    @State private var saveSeq = 0
    @State private var devices: [DeviceEntity] = []
    @State private var agents: [AgentDefinitionEntity] = []
    @State private var teams: [TeamEntity] = []

    private var userId: String? {
        deps.auth.accounts.first { $0.id == accountId }?.userId
    }

    var body: some View {
        ZStack {
            AppBackground()
            content
        }
        .navigationTitle(Voice.Copy.group)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .task(id: accountId) { await load() }
        .task(id: accountId) { await observe() }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Voice.TestIds.page)
    }

    @ViewBuilder
    private var content: some View {
        if let settings {
            form(settings)
        } else if loadFailed {
            VStack(spacing: 12) {
                Text(FeedbackStates.Errors.title(verb: "load", thing: "the voice settings"))
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                GlassPill(FeedbackStates.Errors.retry, icon: AppIcons.uiRefresh, mode: .action {
                    Task { await load() }
                })
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            SkeletonRows(rows: 3)
                .padding(16)
                .frame(maxHeight: .infinity, alignment: .top)
        }
    }

    // MARK: - Form

    private func form(_ settings: Voice.Settings) -> some View {
        // voice.json: every switch is off and disabled while either is None.
        let armed = settings.voiceDeviceId != nil && settings.agentId != nil
        return Form {
            Section {
                GlassPickerRow(
                    Voice.Copy.voiceDevice,
                    selection: Binding(
                        get: { settings.voiceDeviceId },
                        set: { save(Voice.Change(voiceDeviceId: .some($0))) }
                    ),
                    options: deviceOptions.map(\.id),
                    label: { id in
                        guard let id else { return Voice.Copy.voiceDeviceNone }
                        return deviceOptions.first { $0.id == id }?.label ?? deviceName(id)
                    },
                    // THE device picker: each desktop wears its glyph, None none.
                    icon: { id in
                        guard let id, let device = devices.first(where: { $0.deviceId == id }) else { return nil }
                        return DeviceIconDisplay.iconName(icon: device.icon, kind: device.kind)
                    }
                )
                .accessibilityIdentifier(Voice.TestIds.voiceDevice)
                GlassPickerRow(
                    Voice.Copy.agent,
                    selection: Binding(
                        get: { settings.agentId },
                        set: { save(Voice.Change(agentId: .some($0))) }
                    ),
                    options: agentOptions.map(\.id),
                    label: { id in agentOptions.first { $0.id == id }?.label ?? Voice.Copy.agentNone }
                )
                .accessibilityIdentifier(Voice.TestIds.agent)
                toggle(.speakReplies, on: armed && settings.speakReplies, enabled: armed)
                .accessibilityIdentifier(Voice.TestIds.speakReplies)
                languageRow(settings)
                if settings.voiceDeviceId != nil {
                    textRow(.voiceFile)
                    textRow(.voiceTranscript)
                }
            }
            .listRowBackground(glassFormRowFill)

            if let deviceId = settings.voiceDeviceId {
                Section {
                    toggle(.pushToTalk, on: armed && settings.pushToTalk, enabled: armed)
                    .accessibilityIdentifier(Voice.TestIds.pushToTalk)
                    GlassPickerRow(
                        Voice.Copy.shortcut,
                        selection: Binding(
                            get: { settings.pttShortcut },
                            set: { save(Voice.Change(pttShortcut: $0)) }
                        ),
                        options: Voice.shortcuts.map(\.value),
                        label: { value in Voice.shortcuts.first { $0.value == value }?.label ?? value }
                    )
                    .accessibilityIdentifier(Voice.TestIds.shortcut)
                    toggle(.wakeWord, on: armed && settings.wakeWord, enabled: armed)
                    .accessibilityIdentifier(Voice.TestIds.wakeWord)
                    textRow(.wakePhrase)
                } header: {
                    GlassSectionHeader(Voice.trayHeader(deviceName(deviceId)))
                }
                .listRowBackground(glassFormRowFill)
            }
        }
        // EXP-603: the page's own background shows through the grouped list.
        .scrollContentBackground(.hidden)
        .listSectionSpacing(8)
        .tint(DesignTokens.Palette.primary)
    }

    private enum Switch {
        case speakReplies, pushToTalk, wakeWord

        var title: String {
            switch self {
            case .speakReplies: Voice.Copy.speakReplies
            case .pushToTalk: Voice.Copy.pushToTalk
            case .wakeWord: Voice.Copy.wakeWord
            }
        }

        func change(_ on: Bool) -> Voice.Change {
            switch self {
            case .speakReplies: Voice.Change(speakReplies: on)
            case .pushToTalk: Voice.Change(pushToTalk: on)
            case .wakeWord: Voice.Change(wakeWord: on)
            }
        }
    }

    private func toggle(_ field: Switch, on: Bool, enabled: Bool) -> some View {
        GlassToggleRow(field.title, isOn: Binding(get: { on }, set: { save(field.change($0)) })) { EmptyView() }
            .disabled(!enabled)
    }

    /// #NEW-73: picker over `languages`; every language carries its GPU note
    /// (`languageLabel`), in the row and in the menu.
    private func languageRow(_ settings: Voice.Settings) -> some View {
        let device = settings.voiceDeviceId.map(deviceName)
        return GlassPickerRow(
            Voice.Copy.language,
            selection: Binding(
                get: { settings.language },
                set: { save(Voice.Change(language: $0)) }
            ),
            options: Voice.languages.map(\.value),
            label: { value in Voice.languageLabel(language: value, deviceName: device) }
        )
        .accessibilityIdentifier(Voice.TestIds.language)
    }

    /// The page's one-line inputs: Wake phrase, Voice file, Transcript.
    private enum TextInput: Hashable {
        case wakePhrase, voiceFile, voiceTranscript

        var title: String {
            switch self {
            case .wakePhrase: Voice.Copy.wakePhrase
            case .voiceFile: Voice.Copy.voiceFile
            case .voiceTranscript: Voice.Copy.voiceTranscript
            }
        }

        var testId: String {
            switch self {
            case .wakePhrase: Voice.TestIds.wakePhrase
            case .voiceFile: Voice.TestIds.voiceFile
            case .voiceTranscript: Voice.TestIds.voiceTranscript
            }
        }

        /// voice.json `limits`; the wake phrase takes the server's 64.
        var limit: Int {
            switch self {
            case .wakePhrase: 64
            case .voiceFile: Voice.Limits.voiceFile
            case .voiceTranscript: Voice.Limits.voiceTranscript
            }
        }

        /// What the failed-action toast names.
        var thing: String {
            switch self {
            case .wakePhrase: "the wake phrase"
            case .voiceFile: "the voice file"
            case .voiceTranscript: "the transcript"
            }
        }

        func stored(_ settings: Voice.Settings) -> String {
            switch self {
            case .wakePhrase: settings.wakePhrase
            case .voiceFile: settings.voiceFile
            case .voiceTranscript: settings.voiceTranscript
            }
        }

        func change(_ value: String) -> Voice.Change {
            switch self {
            case .wakePhrase: Voice.Change(wakePhrase: value)
            case .voiceFile: Voice.Change(voiceFile: value)
            case .voiceTranscript: Voice.Change(voiceTranscript: value)
            }
        }
    }

    private func placeholder(_ input: TextInput) -> String {
        switch input {
        case .wakePhrase: Voice.wakePhrasePlaceholder(agentName)
        case .voiceFile: Voice.Copy.voiceFilePlaceholder
        case .voiceTranscript: Voice.Copy.voiceTranscriptPlaceholder
        }
    }

    private func draft(_ input: TextInput) -> String {
        switch input {
        case .wakePhrase: wakePhrase
        case .voiceFile: voiceFile
        case .voiceTranscript: voiceTranscript
        }
    }

    private func setDraft(_ input: TextInput, _ value: String) {
        switch input {
        case .wakePhrase: wakePhrase = value
        case .voiceFile: voiceFile = value
        case .voiceTranscript: voiceTranscript = value
        }
    }

    /// Every draft back to a stored row (load, revert).
    private func resetDrafts(_ settings: Voice.Settings) {
        wakePhrase = settings.wakePhrase
        voiceFile = settings.voiceFile
        voiceTranscript = settings.voiceTranscript
    }

    /// The input row (label + trailing field, the board form's Prefix row):
    /// saved a beat after typing stops, and at once on submit.
    private func textRow(_ input: TextInput) -> some View {
        HStack(spacing: 8) {
            Text(input.title)
                .foregroundStyle(.white.opacity(TextOpacity.primary))
            Spacer(minLength: 8)
            GlassTextField(
                placeholder(input),
                text: Binding(
                    get: { draft(input) },
                    set: { next in
                        // One line (a pasted line break or tab reads as a
                        // space), no control characters (Cc only: a format
                        // character such as an emoji's ZWJ stays), at most
                        // `limits` UTF-16 units, the server's count.
                        let oneLine = next.replacingOccurrences(of: "\r\n", with: " ")
                        let clean = Self.clipped(
                            String(String.UnicodeScalarView(oneLine.unicodeScalars.compactMap { scalar in
                                if scalar == "\n" || scalar == "\r" || scalar == "\t" { return " " }
                                return scalar.properties.generalCategory == .control ? nil : scalar
                            })),
                            utf16Limit: input.limit
                        )
                        setDraft(input, clean)
                        scheduleTextSave(input)
                    }
                ),
                bordered: false,
                accessibilityIdentifier: input.testId
            )
            .multilineTextAlignment(.trailing)
            .autocorrectionDisabled()
            .submitLabel(.done)
            .onSubmit { saveText(input) }
        }
    }

    /// `text` cut to at most `limit` UTF-16 units (zod's `.max` counts those),
    /// never inside a character.
    private static func clipped(_ text: String, utf16Limit limit: Int) -> String {
        guard text.utf16.count > limit else { return text }
        var end = text.startIndex
        var used = 0
        for index in text.indices {
            let next = used + text[index].utf16.count
            if next > limit { break }
            used = next
            end = text.index(after: index)
        }
        return String(text[..<end])
    }

    // MARK: - Vocabulary

    /// The member's OWN devices; `voiceDeviceOptions` keeps the desktops.
    private var deviceOptions: [Voice.Option] {
        Voice.voiceDeviceOptions(devices.filter { $0.userId == userId }.map {
            Voice.DeviceLike(deviceId: $0.deviceId, name: $0.label, kind: $0.kind ?? "")
        })
    }

    private var agentOptions: [Voice.Option] {
        let teamNames = Dictionary(teams.map { ($0.id, $0.name) }, uniquingKeysWith: { first, _ in first })
        return Voice.agentOptions(agents.map {
            Voice.AgentLike(id: $0.id, name: $0.name, team: teamNames[$0.teamId] ?? "")
        })
    }

    private func deviceName(_ deviceId: String) -> String {
        devices.first { $0.deviceId == deviceId }?.label ?? deviceId
    }

    private var agentName: String {
        agents.first { $0.id == settings?.agentId }?.name ?? Voice.Copy.agent
    }

    // MARK: - Load / save

    private func load() async {
        loadFailed = false
        do {
            let loaded = try await deps.voiceApi.get(accountId: accountId)
            settings = loaded
            resetDrafts(loaded)
        } catch {
            if settings == nil { loadFailed = true }
        }
    }

    private func observe() async {
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in
            (
                try DeviceEntity.fetchAll(db),
                try AgentDefinitionEntity.fetchAll(db),
                try TeamEntity.fetchAll(db)
            )
        }
        do {
            for try await (nextDevices, nextAgents, nextTeams) in observation.values(in: pool) {
                devices = nextDevices
                agents = nextAgents
                teams = nextTeams
            }
        } catch {}
    }

    /// Optimistic: the page reads `clearOnNone(change)` at once, takes the
    /// server's row on success and reverts + toasts on a refusal. Last write
    /// wins: only the newest request settles or reverts, so an older refusal
    /// never undoes a newer change. A text row's refusal resets ITS draft only.
    private func save(_ change: Voice.Change, input: TextInput? = nil) {
        guard let previous = settings else { return }
        let next = Voice.clearOnNone(previous, change)
        guard next != previous else { return }
        settings = next
        saveSeq += 1
        let seq = saveSeq
        Task {
            do {
                let stored = try await deps.voiceApi.update(accountId: accountId, change: change)
                if seq == saveSeq { settings = stored }
            } catch {
                if seq == saveSeq {
                    settings = previous
                    if let input { setDraft(input, input.stored(previous)) }
                }
                toaster.error(
                    FeedbackStates.Errors.title(verb: "save", thing: input?.thing ?? "the voice settings"),
                    description: error.trpcUserMessage
                )
            }
        }
    }

    private func scheduleTextSave(_ input: TextInput) {
        textSaves[input]?.cancel()
        textSaves[input] = Task {
            try? await Task.sleep(for: .milliseconds(600))
            guard !Task.isCancelled else { return }
            saveText(input)
        }
    }

    private func saveText(_ input: TextInput) {
        textSaves[input]?.cancel()
        guard let settings else { return }
        let trimmed = draft(input).trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed != input.stored(settings) else { return }
        save(input.change(trimmed), input: input)
    }
}

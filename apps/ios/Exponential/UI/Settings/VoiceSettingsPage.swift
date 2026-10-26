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
                    label: { id in deviceOptions.first { $0.id == id }?.label ?? Voice.Copy.voiceDeviceNone }
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

    /// #NEW-73: picker over `languages`; German carries its GPU note
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

        /// voice.json `limits`; the wake phrase has none on the page.
        var limit: Int? {
            switch self {
            case .wakePhrase: nil
            case .voiceFile: Voice.Limits.voiceFile
            case .voiceTranscript: Voice.Limits.voiceTranscript
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
                        // One line, no control characters, at most `limits`.
                        var clean = String(String.UnicodeScalarView(
                            next.unicodeScalars.filter { !CharacterSet.controlCharacters.contains($0) }
                        ))
                        if let limit = input.limit, clean.count > limit { clean = String(clean.prefix(limit)) }
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
        agents.first { $0.id == settings?.agentId }?.name ?? ""
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
    /// server's row on success and reverts + toasts on a refusal.
    private func save(_ change: Voice.Change) {
        guard let previous = settings else { return }
        let next = Voice.clearOnNone(previous, change)
        guard next != previous else { return }
        settings = next
        Task {
            do {
                let stored = try await deps.voiceApi.update(accountId: accountId, change: change)
                if settings == next { settings = stored }
            } catch {
                settings = previous
                resetDrafts(previous)
                toaster.error(
                    FeedbackStates.Errors.title(verb: "save", thing: "the voice settings"),
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
        save(input.change(trimmed))
    }
}

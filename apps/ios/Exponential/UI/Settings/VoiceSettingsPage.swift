import ExpCore
import ExpUI
import GRDB
import SwiftUI

// NEW-67 (voice plan v3, V1): Settings → Personal → Voice (`voice.json`, the
// same page on web, the IDE and Android; on iOS it hangs off the account
// page, the phone's Personal settings). ONE group (Voice device, Agent, Speak replies), then while a voice device is
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
    @State private var wakePhraseSave: Task<Void, Never>?
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
                    wakePhraseRow
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

    /// The input row (label + trailing field, the board form's Prefix row):
    /// saved a beat after typing stops, and at once on submit.
    private var wakePhraseRow: some View {
        HStack(spacing: 8) {
            Text(Voice.Copy.wakePhrase)
                .foregroundStyle(.white.opacity(TextOpacity.primary))
            Spacer(minLength: 8)
            GlassTextField(
                Voice.wakePhrasePlaceholder(agentName),
                text: Binding(
                    get: { wakePhrase },
                    set: { next in
                        wakePhrase = next
                        scheduleWakePhraseSave()
                    }
                ),
                bordered: false,
                accessibilityIdentifier: Voice.TestIds.wakePhrase
            )
            .multilineTextAlignment(.trailing)
            .autocorrectionDisabled()
            .submitLabel(.done)
            .onSubmit { saveWakePhrase() }
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
            wakePhrase = loaded.wakePhrase
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
                wakePhrase = previous.wakePhrase
                toaster.error(
                    FeedbackStates.Errors.title(verb: "save", thing: "the voice settings"),
                    description: error.trpcUserMessage
                )
            }
        }
    }

    private func scheduleWakePhraseSave() {
        wakePhraseSave?.cancel()
        wakePhraseSave = Task {
            try? await Task.sleep(for: .milliseconds(600))
            guard !Task.isCancelled else { return }
            saveWakePhrase()
        }
    }

    private func saveWakePhrase() {
        wakePhraseSave?.cancel()
        let trimmed = wakePhrase.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed != settings?.wakePhrase else { return }
        save(Voice.Change(wakePhrase: trimmed))
    }
}

import ExpCore
import ExpUI
import GRDB
import SwiftUI

// NEW-67 (voice plan v3, V1): Settings → Personal → Voice (`voice.json`, the
// same page on web, the IDE and Android; on iOS it hangs off the account
// page, the phone's Personal settings). ONE group (Voice device, Agent,
// Language read through `languageLabel`, and while a voice device is picked
// Voice file + Transcript, #NEW-73), then while a voice device is picked the
// tray group under `trayHeader` (Push to talk, Wake word, Wake phrase). S-3:
// Push to talk is READ-ONLY here (the binding's `shortcutLabel`, None while
// empty: keys are recorded on the voice device's IDE); the Wake word row's
// detail says why it is or is not listening (the voice device's doctor row
// `wake`, else `wakeDetail`). Every switch reads off and is disabled while
// the device or the agent is None. Each change saves at once with `voice.update`
// (optimistic; a refusal reverts and toasts, feedback-states.json). #NEW-56:
// a wake phrase failing `wakePhraseError` is never sent; leaving or
// submitting the field shows the line inline under it until the next edit.
// #NEW-57 (meeting.json): after Wake phrase, Meeting mode (ON opens THE
// consent prompt, Start = `voice.startMeeting`; OFF = `voice.stopMeeting` at
// once; while on its detail reads `who`), Desktop audio over the voice
// device's capability, and while it picks or leaves out apps on a per-app
// device the Apps multi-pick over `appOptions(live, stored)`: their own
// headerless group under the tray group.
struct VoiceSettingsPage: View {
    let accountId: String
    /// Styleguide capture only (`sg_settings-voice`): the page over fixture
    /// rows instead of `voice.get` + the account's database; a change stays local.
    var specimen: Specimen? = nil

    /// The rows the page otherwise loads and observes; `consentOpen` opens
    /// THE consent prompt (`ThreadCardsScreen`).
    struct Specimen {
        var userId: String? = nil
        let settings: Voice.Settings
        let devices: [DeviceEntity]
        let agents: [AgentDefinitionEntity]
        let teams: [TeamEntity]
        var consentOpen = false
    }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.toaster) private var toaster

    /// nil until `voice.get` landed.
    @State private var settings: Voice.Settings?
    @State private var loadFailed = false
    @State private var wakePhrase = ""
    @State private var voiceFile = ""
    @State private var voiceTranscript = ""
    /// #NEW-56: `wakePhraseError` of the phrase left or submitted, until the next edit.
    @State private var wakePhraseError: String?
    @FocusState private var focusedInput: TextInput?
    /// The pending debounced save per input row.
    @State private var textSaves: [TextInput: Task<Void, Never>] = [:]
    /// Last write wins: only the newest `voice.update` may settle or revert.
    @State private var saveSeq = 0
    @State private var devices: [DeviceEntity] = []
    @State private var agents: [AgentDefinitionEntity] = []
    @State private var teams: [TeamEntity] = []
    /// #NEW-57: THE consent prompt is open (Meeting mode turned on).
    @State private var meetingConsent = false
    init(accountId: String, specimen: Specimen? = nil) {
        self.accountId = accountId
        self.specimen = specimen
        if let specimen {
            _settings = State(initialValue: specimen.settings)
            _devices = State(initialValue: specimen.devices)
            _agents = State(initialValue: specimen.agents)
            _teams = State(initialValue: specimen.teams)
            _meetingConsent = State(initialValue: specimen.consentOpen)
        }
    }

    private var userId: String? {
        if let specimen { return specimen.userId }
        return deps.auth.accounts.first { $0.id == accountId }?.userId
    }

    var body: some View {
        ZStack {
            AppBackground()
            content
        }
        .navigationTitle(Voice.Copy.group)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .task(id: accountId) { if specimen == nil { await load() } }
        .task(id: accountId) { if specimen == nil { await observe() } }
        // #NEW-57: THE consent prompt (the doctor Import's GlassAlert);
        // Cancel leaves Meeting mode off.
        .glassAlert(
            isPresented: $meetingConsent,
            title: Meeting.Copy.consentTitle,
            message: Meeting.consentBody(
                who: meetingWho, agent: agentName, days: Meeting.retentionDays
            ),
            actions: [
                GlassAlertAction(Meeting.Copy.cancel, role: .outline, isCancel: true, id: "cancel") {},
                GlassAlertAction(Meeting.Copy.start, role: .primary, id: Meeting.TestIds.consentStart) {
                    startMeeting()
                },
            ]
        )
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
                    // THE device picker: each desktop wears its glyph in the sheet
                    // (None none); the row's value is the name alone (web, Android).
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
                languageRow(settings)
                if settings.voiceDeviceId != nil {
                    textRow(.voiceFile)
                    textRow(.voiceTranscript)
                }
            }
            .listRowBackground(glassFormRowFill)

            if let deviceId = settings.voiceDeviceId {
                Section {
                    // S-3: read-only; the voice device's IDE records the key.
                    GlassPickerRowLabel(Voice.Copy.pushToTalk, value: Voice.shortcutLabel(settings.pttShortcut), chevron: false)
                        .accessibilityElement(children: .combine)
                        .accessibilityIdentifier(Voice.TestIds.shortcut)
                    GlassToggleRow(
                        Voice.Copy.wakeWord,
                        description: wakeDetail(settings, armed: armed),
                        isOn: Binding(get: { armed && settings.wakeWord }, set: { save(Voice.Change(wakeWord: $0)) })
                    )
                    .disabled(!armed)
                    .accessibilityIdentifier(Voice.TestIds.wakeWord)
                    textRow(.wakePhrase)
                } header: {
                    GlassSectionHeader(Voice.trayHeader(deviceName(deviceId)))
                } footer: {
                    // #NEW-56: the wake phrase's invalid-input line (FEED-79,
                    // 13pt rose) under the group, like web's and Android's
                    // FieldError; the bare field itself draws no border.
                    if let wakePhraseError {
                        Text(wakePhraseError)
                            .font(.system(size: CGFloat(FeedbackStates.Errors.inlineSize)))
                            .foregroundStyle(DesignTokens.Semantic.red)
                            .fixedSize(horizontal: false, vertical: true)
                            .accessibilityIdentifier("\(Voice.TestIds.wakePhrase)-error")
                    }
                }
                .listRowBackground(glassFormRowFill)
                // #NEW-57: the meeting rows, their OWN group without a header,
                // right under the tray group (Wake phrase stays its last row).
                Section {
                    meetingRows(settings, armed: armed)
                }
                .listRowBackground(glassFormRowFill)
            }
        }
        // EXP-603: the page's own background shows through the grouped list.
        .scrollContentBackground(.hidden)
        .listSectionSpacing(8)
        .tint(DesignTokens.Palette.primary)
    }

    /// S-3: the Wake word row's detail: the voice device's doctor row
    /// `wake` (its own `wakeDetail` text) verbatim, else `wakeDetail` with
    /// no worker state (noAgent / offline / starting).
    private func wakeDetail(_ settings: Voice.Settings, armed: Bool) -> String? {
        let device = settings.voiceDeviceId.flatMap { id in devices.first { $0.deviceId == id } }
        let context = Voice.WakeDetailContext(
            wakeWord: armed && settings.wakeWord,
            agent: settings.agentId != nil,
            online: DeviceLiveness.isOnline(lastSeenAt: device?.lastSeenAt, now: Date()),
            device: device?.label ?? "",
            state: nil
        )
        guard context.wakeWord, context.agent, context.online else { return Voice.wakeDetail(context) }
        return Voice.wakeDoctorDetail(ofJSON: device?.doctor) ?? Voice.wakeDetail(context)
    }

    // MARK: - Meeting mode (#NEW-57)

    /// The voice device's `doctor.desktopAudio`; absent = none and no apps.
    private var doctorAudio: Meeting.DoctorAudio {
        guard let id = settings?.voiceDeviceId,
              let device = devices.first(where: { $0.deviceId == id })
        else { return .absent }
        return DeviceDoctor.decode(json: device.doctor)?.desktopAudio ?? .absent
    }

    private var meetingWho: String {
        guard let settings else { return Meeting.Copy.you }
        return Meeting.who(settings.desktopAudio, apps: settings.desktopAudioApps, capability: doctorAudio.capability)
    }

    @ViewBuilder
    private func meetingRows(_ settings: Voice.Settings, armed: Bool) -> some View {
        let on = armed && settings.meetingOn
        GlassToggleRow(
            Meeting.Copy.meeting,
            description: on ? meetingWho : nil,
            isOn: Binding(
                get: { on },
                set: { next in
                    // ON never writes by itself: THE consent prompt does.
                    if next { meetingConsent = true } else { stopMeeting() }
                }
            )
        ) { EmptyView() }
        .disabled(!armed)
        .accessibilityIdentifier(Meeting.TestIds.meeting)
        let capability = doctorAudio.capability
        GlassPickerRow(
            Meeting.Copy.desktopAudio,
            selection: Binding(
                get: { settings.desktopAudio },
                set: { save(Voice.Change(desktopAudio: $0)) }
            ),
            options: Meeting.desktopAudioOptions(capability),
            label: \.label
        )
        .accessibilityIdentifier(Meeting.TestIds.desktopAudio)
        // Only while the device picks per app and the stored value is one
        // of its options (include / exclude).
        if capability == .perApp, settings.desktopAudio.picksApps,
           Meeting.desktopAudioOptions(capability).contains(settings.desktopAudio) {
            appsRow(settings)
        }
    }

    /// THE multi-pick over `appOptions(live, stored)`; the picked apps keep
    /// the options' order, at most `limits.apps`.
    private func appsRow(_ settings: Voice.Settings) -> some View {
        let stored = settings.desktopAudioApps
        let options = Meeting.appOptions(live: doctorAudio.apps, stored: stored)
        return GlassPicker(
            items: options.map { PickerItem(value: $0.value, label: $0.label) },
            mode: .multi,
            value: Set(stored.map(\.id)),
            onChange: { picked in
                let apps = options
                    .filter { picked.contains($0.value) }
                    .prefix(Meeting.Limits.apps)
                    .map { Meeting.AudioApp(id: $0.value, name: String($0.label.prefix(Meeting.Limits.appName))) }
                save(Voice.Change(desktopAudioApps: Array(apps)))
            },
            emptyText: Meeting.Copy.appsNone,
            title: Meeting.Copy.apps
        ) {
            GlassPickerRowLabel(Meeting.Copy.apps, value: Meeting.appsValue(stored))
        }
        .accessibilityIdentifier(Meeting.TestIds.apps)
    }

    /// Start: optimistic on, `voice.startMeeting {consent: true}`; a refusal
    /// reverts and toasts.
    private func startMeeting() {
        guard settings != nil, settings?.meetingOn == false else { return }
        let startedAt = ISO8601DateFormatter().string(from: Date())
        settings?.meetingStartedAt = startedAt
        Task {
            do {
                try await deps.voiceApi.startMeeting(accountId: accountId)
            } catch {
                if settings?.meetingStartedAt == startedAt { settings?.meetingStartedAt = nil }
                toaster.error(
                    FeedbackStates.Errors.title(verb: "start", thing: "meeting mode"),
                    description: error.trpcUserMessage
                )
            }
        }
    }

    /// Off: `voice.stopMeeting` at once (no prompt); a refusal reverts and toasts.
    private func stopMeeting() {
        guard let previous = settings?.meetingStartedAt else { return }
        settings?.meetingStartedAt = nil
        Task {
            do {
                try await deps.voiceApi.stopMeeting(accountId: accountId)
            } catch {
                if settings?.meetingStartedAt == nil { settings?.meetingStartedAt = previous }
                toaster.error(
                    FeedbackStates.Errors.title(verb: "stop", thing: "meeting mode"),
                    description: error.trpcUserMessage
                )
            }
        }
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

        /// voice.json `limits` (= `voice.update`'s caps).
        var limit: Int {
            switch self {
            case .wakePhrase: Voice.Limits.wakePhrase
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
    /// saved a beat after typing stops, and at once on submit or leave. The
    /// wake phrase's invalid-input line is the tray section's footer.
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
                        // #NEW-75: THE input rule ×4 (control characters
                        // dropped, cut to the server's UTF-16 limit).
                        let clean = Voice.textInput(next, max: input.limit)
                        if input == .wakePhrase, clean != wakePhrase { wakePhraseError = nil }
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
            .focused($focusedInput, equals: input)
            .onSubmit { saveText(input, committed: true) }
            .onChange(of: focusedInput) { old, new in
                if old == input, new != input { saveText(input, committed: true) }
            }
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

    /// #NEW-71: a device row not (yet) synced reads `…`, never its raw id.
    private func deviceName(_ deviceId: String) -> String {
        devices.first { $0.deviceId == deviceId }?.label ?? Voice.Copy.ellipsis
    }

    private var agentName: String {
        agents.first { $0.id == settings?.agentId }?.name ?? Voice.Copy.agent
    }

    // MARK: - Load / save

    private func load() async {
        loadFailed = false
        if let specimen {
            settings = specimen.settings
            resetDrafts(specimen.settings)
            return
        }
        do {
            let loaded = try await deps.voiceApi.get(accountId: accountId)
            settings = loaded
            resetDrafts(loaded)
        } catch {
            if settings == nil { loadFailed = true }
        }
    }

    private func observe() async {
        if let specimen {
            devices = specimen.devices
            agents = specimen.agents
            teams = specimen.teams
            return
        }
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
        if specimen != nil { return }
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

    /// `committed` = the field was submitted or left (the debounce is not):
    /// only then does an invalid wake phrase show its line. An invalid one is
    /// never sent, so the stored phrase stays.
    private func saveText(_ input: TextInput, committed: Bool = false) {
        textSaves[input]?.cancel()
        guard let settings else { return }
        let trimmed = draft(input).trimmingCharacters(in: .whitespacesAndNewlines)
        if input == .wakePhrase, let error = Voice.wakePhraseError(trimmed) {
            if committed { wakePhraseError = error }
            return
        }
        guard trimmed != input.stored(settings) else { return }
        save(input.change(trimmed), input: input)
    }
}

/// Styleguide capture only (`-uiTestingStyleguide settings-voice`,
/// `sg_settings-voice`): THE Voice page as pushed off the account page, over
/// the seed's rows (seed-screenshots.ts: Alex's MacBook Pro as the voice
/// device, Triage as the agent, Ctrl + Space bound, the wake word on and
/// listening (the device's doctor row `wake`), German).
struct VoiceSettingsSpecimen: View {
    private static let userId = "specimen-user"
    private static let deviceId = "screenshot-demo-desktop"
    private static let teamId = "specimen-team"
    private static let stamp = "2026-10-01T09:00:00Z"

    private static let specimen = VoiceSettingsPage.Specimen(
        userId: userId,
        settings: Voice.Settings(
            voiceDeviceId: deviceId,
            agentId: "specimen-agent-triage",
            pttShortcut: "ctrl+space",
            wakeWord: true,
            wakePhrase: "",
            language: "de"
        ),
        devices: [
            DeviceEntity(
                id: "specimen-device", userId: userId, deviceId: deviceId, label: "Alex's MacBook Pro", kind: "desktop",
                doctor: #"{"items":[{"key":"wake","group":"voice","state":"ok","detail":"Listening"}]}"#,
                // Online at capture time (the Wake word row reads the doctor).
                lastSeenAt: ISO8601DateFormatter().string(from: Date())
            ),
        ],
        agents: [
            AgentDefinitionEntity(id: "specimen-agent-triage", teamId: teamId, name: "Triage", createdAt: stamp, updatedAt: stamp),
            AgentDefinitionEntity(id: "specimen-agent-designer", teamId: teamId, name: "Designer", createdAt: stamp, updatedAt: stamp),
        ],
        teams: [
            TeamEntity(id: teamId, name: "Acme", slug: "acme", iconUrl: nil, createdAt: stamp, updatedAt: stamp),
        ]
    )

    var body: some View {
        // A one-deep path, so the page wears the pushed back chevron.
        NavigationStack(path: .constant(["voice"])) {
            Color.clear
                .navigationDestination(for: String.self) { _ in
                    VoiceSettingsPage(accountId: "", specimen: Self.specimen)
                        // A preset path skips the push, so pin the pushed
                        // page's inline title (else a large one overlaps).
                        .navigationBarTitleDisplayMode(.inline)
                }
        }
    }
}

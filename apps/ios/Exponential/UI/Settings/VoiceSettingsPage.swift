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
// (optimistic; a refusal reverts and toasts, feedback-states.json). #NEW-56:
// a wake phrase failing `wakePhraseError` is never sent; leaving or
// submitting the field shows the line inline under it until the next edit.
// #NEW-57 (meeting.json): after Wake phrase, Meeting mode (ON opens THE
// consent prompt, Start = `voice.startMeeting`; OFF = `voice.stopMeeting` at
// once; while on its detail reads `who`), Desktop audio over the voice
// device's capability, and while it picks or leaves out apps on a per-app
// device the Apps multi-pick over `appOptions(live, stored)`: their own
// headerless group under the tray group.
//
// S-3 (agent-settings.json `assistant`): the Voice page left Settings'
// Personal group; its rows are ONE block (`VoiceSettingsBlock` over
// `VoiceSettingsModel`) that the Assistant page draws as its Voice section
// WITHOUT the Voice device + Agent pickers (the server derives both from the
// member's Assistant). `VoiceSettingsPage` stays a thin host of the same
// block (its route + the styleguide specimens).
/// The voice settings' state + writes (`voice.get` / `voice.update` /
/// meeting start + stop), shared by every host of `VoiceSettingsBlock`.
@MainActor
@Observable
final class VoiceSettingsModel {
    let accountId: String
    /// Styleguide capture only: fixture rows, a change stays local.
    let specimen: VoiceSettingsPage.Specimen?

    /// nil until `voice.get` landed.
    var settings: Voice.Settings?
    var loadFailed = false
    var wakePhrase = ""
    var voiceFile = ""
    var voiceTranscript = ""
    /// #NEW-56: `wakePhraseError` of the phrase left or submitted, until the next edit.
    var wakePhraseError: String?
    /// The pending debounced save per input row.
    @ObservationIgnored private var textSaves: [VoiceTextInput: Task<Void, Never>] = [:]
    /// Last write wins: only the newest `voice.update` may settle or revert.
    @ObservationIgnored private var saveSeq = 0
    var devices: [DeviceEntity] = []
    var agents: [AgentDefinitionEntity] = []
    var teams: [TeamEntity] = []
    /// #NEW-57: THE consent prompt is open (Meeting mode turned on).
    var meetingConsent = false

    @ObservationIgnored private var deps: AppDependencies?
    @ObservationIgnored private var toaster: Toaster = .shared

    init(accountId: String, specimen: VoiceSettingsPage.Specimen? = nil) {
        self.accountId = accountId
        self.specimen = specimen
        if let specimen {
            settings = specimen.settings
            devices = specimen.devices
            agents = specimen.agents
            teams = specimen.teams
            meetingConsent = specimen.consentOpen
            resetDrafts(specimen.settings)
        }
    }

    /// The host's environment (once, before `load` / `observe`).
    func attach(deps: AppDependencies, toaster: Toaster) {
        self.deps = deps
        self.toaster = toaster
    }

    var userId: String? {
        if let specimen { return specimen.userId }
        return deps?.auth.accounts.first { $0.id == accountId }?.userId
    }

    // MARK: - Vocabulary

    /// The member's OWN devices; `voiceDeviceOptions` keeps the desktops.
    var deviceOptions: [Voice.Option] {
        Voice.voiceDeviceOptions(devices.filter { $0.userId == userId }.map {
            Voice.DeviceLike(deviceId: $0.deviceId, name: $0.label, kind: $0.kind ?? "")
        })
    }

    var agentOptions: [Voice.Option] {
        let teamNames = Dictionary(teams.map { ($0.id, $0.name) }, uniquingKeysWith: { first, _ in first })
        return Voice.agentOptions(agents.map {
            Voice.AgentLike(id: $0.id, name: $0.name, team: teamNames[$0.teamId] ?? "")
        })
    }

    /// #NEW-71: a device row not (yet) synced reads `…`, never its raw id.
    func deviceName(_ deviceId: String) -> String {
        devices.first { $0.deviceId == deviceId }?.label ?? Voice.Copy.ellipsis
    }

    var agentName: String {
        agents.first { $0.id == settings?.agentId }?.name ?? Voice.Copy.agent
    }

    /// The voice device's `doctor.desktopAudio`; absent = none and no apps.
    var doctorAudio: Meeting.DoctorAudio {
        guard let id = settings?.voiceDeviceId,
              let device = devices.first(where: { $0.deviceId == id })
        else { return .absent }
        return DeviceDoctor.decode(json: device.doctor)?.desktopAudio ?? .absent
    }

    var meetingWho: String {
        guard let settings else { return Meeting.Copy.you }
        return Meeting.who(settings.desktopAudio, apps: settings.desktopAudioApps, capability: doctorAudio.capability)
    }

    // MARK: - Drafts

    func draft(_ input: VoiceTextInput) -> String {
        switch input {
        case .wakePhrase: wakePhrase
        case .voiceFile: voiceFile
        case .voiceTranscript: voiceTranscript
        }
    }

    func setDraft(_ input: VoiceTextInput, _ value: String) {
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

    // MARK: - Load / save

    func load() async {
        loadFailed = false
        if let specimen {
            settings = specimen.settings
            resetDrafts(specimen.settings)
            return
        }
        guard let deps else { return }
        do {
            let loaded = try await deps.voiceApi.get(accountId: accountId)
            settings = loaded
            resetDrafts(loaded)
        } catch {
            if settings == nil { loadFailed = true }
        }
    }

    func observe() async {
        if specimen != nil { return }
        guard let deps, let pool = try? deps.db.pool(forAccountId: accountId) else { return }
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
    func save(_ change: Voice.Change, input: VoiceTextInput? = nil) {
        guard let previous = settings else { return }
        let next = Voice.clearOnNone(previous, change)
        guard next != previous else { return }
        settings = next
        guard specimen == nil, let deps else { return }
        saveSeq += 1
        let seq = saveSeq
        let accountId = accountId
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

    func scheduleTextSave(_ input: VoiceTextInput) {
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
    func saveText(_ input: VoiceTextInput, committed: Bool = false) {
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

    // MARK: - Meeting mode (#NEW-57)

    /// Start: optimistic on, `voice.startMeeting {consent: true}`; a refusal
    /// reverts and toasts.
    func startMeeting() {
        guard settings != nil, settings?.meetingOn == false else { return }
        let startedAt = ISO8601DateFormatter().string(from: Date())
        settings?.meetingStartedAt = startedAt
        guard specimen == nil, let deps else { return }
        let accountId = accountId
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
    func stopMeeting() {
        guard let previous = settings?.meetingStartedAt else { return }
        settings?.meetingStartedAt = nil
        guard specimen == nil, let deps else { return }
        let accountId = accountId
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
}

/// The page's one-line inputs: Wake phrase, Voice file, Transcript.
enum VoiceTextInput: Hashable {
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

/// THE voice settings rows as ONE block of `Form` sections (voice.json's
/// group, then the tray group and the meeting rows while a voice device is
/// picked). `pickers` = the Voice device + Agent rows (the old standalone
/// page); the Assistant page draws the block without them. `header` titles
/// the first section (the Assistant page's `Voice`). Its host attaches
/// `voiceSettingsHost(_:)` (load, observe, THE consent prompt).
struct VoiceSettingsBlock: View {
    @Bindable var model: VoiceSettingsModel
    var pickers = false
    var header: String? = nil

    @FocusState private var focusedInput: VoiceTextInput?

    var body: some View {
        if let settings = model.settings {
            sections(settings)
        } else if model.loadFailed {
            Section {
                VStack(spacing: 12) {
                    Text(FeedbackStates.Errors.title(verb: "load", thing: "the voice settings"))
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    GlassPill(FeedbackStates.Errors.retry, icon: AppIcons.uiRefresh, mode: .action {
                        Task { await model.load() }
                    })
                }
                .frame(maxWidth: .infinity)
                .padding(.vertical, 12)
            } header: {
                headerView
            }
            .listRowBackground(glassFormRowFill)
        } else {
            Section {
                SkeletonRows(rows: 3, rowHeight: 22, dot: false, dividers: false)
            } header: {
                headerView
            }
            .listRowBackground(glassFormRowFill)
        }
    }

    @ViewBuilder
    private var headerView: some View {
        if let header { GlassSectionHeader(header) }
    }

    @ViewBuilder
    private func sections(_ settings: Voice.Settings) -> some View {
        // voice.json: every switch is off and disabled while either is None.
        let armed = settings.voiceDeviceId != nil && settings.agentId != nil
        Section {
            if pickers {
                GlassPickerRow(
                    Voice.Copy.voiceDevice,
                    selection: Binding(
                        get: { settings.voiceDeviceId },
                        set: { model.save(Voice.Change(voiceDeviceId: .some($0))) }
                    ),
                    options: model.deviceOptions.map(\.id),
                    label: { id in
                        guard let id else { return Voice.Copy.voiceDeviceNone }
                        return model.deviceOptions.first { $0.id == id }?.label ?? model.deviceName(id)
                    },
                    // THE device picker: each desktop wears its glyph in the sheet
                    // (None none); the row's value is the name alone (web, Android).
                    icon: { id in
                        guard let id, let device = model.devices.first(where: { $0.deviceId == id }) else { return nil }
                        return DeviceIconDisplay.iconName(icon: device.icon, kind: device.kind)
                    }
                )
                .accessibilityIdentifier(Voice.TestIds.voiceDevice)
                GlassPickerRow(
                    Voice.Copy.agent,
                    selection: Binding(
                        get: { settings.agentId },
                        set: { model.save(Voice.Change(agentId: .some($0))) }
                    ),
                    options: model.agentOptions.map(\.id),
                    label: { id in model.agentOptions.first { $0.id == id }?.label ?? Voice.Copy.agentNone }
                )
                .accessibilityIdentifier(Voice.TestIds.agent)
            }
            toggle(.speakReplies, on: armed && settings.speakReplies, enabled: armed)
            .accessibilityIdentifier(Voice.TestIds.speakReplies)
            languageRow(settings)
            if settings.voiceDeviceId != nil {
                textRow(.voiceFile)
                textRow(.voiceTranscript)
            }
        } header: {
            headerView
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
                        set: { model.save(Voice.Change(pttShortcut: $0)) }
                    ),
                    options: Voice.shortcuts.map(\.value),
                    label: { value in Voice.shortcuts.first { $0.value == value }?.label ?? value }
                )
                .accessibilityIdentifier(Voice.TestIds.shortcut)
                toggle(.wakeWord, on: armed && settings.wakeWord, enabled: armed)
                .accessibilityIdentifier(Voice.TestIds.wakeWord)
                textRow(.wakePhrase)
            } header: {
                GlassSectionHeader(Voice.trayHeader(model.deviceName(deviceId)))
            } footer: {
                // #NEW-56: the wake phrase's invalid-input line (FEED-79,
                // 13pt rose) under the group, like web's and Android's
                // FieldError; the bare field itself draws no border.
                if let wakePhraseError = model.wakePhraseError {
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
        GlassToggleRow(field.title, isOn: Binding(get: { on }, set: { model.save(field.change($0)) })) { EmptyView() }
            .disabled(!enabled)
    }

    // MARK: - Meeting mode (#NEW-57)

    @ViewBuilder
    private func meetingRows(_ settings: Voice.Settings, armed: Bool) -> some View {
        let on = armed && settings.meetingOn
        GlassToggleRow(
            Meeting.Copy.meeting,
            description: on ? model.meetingWho : nil,
            isOn: Binding(
                get: { on },
                set: { next in
                    // ON never writes by itself: THE consent prompt does.
                    if next { model.meetingConsent = true } else { model.stopMeeting() }
                }
            )
        ) { EmptyView() }
        .disabled(!armed)
        .accessibilityIdentifier(Meeting.TestIds.meeting)
        let capability = model.doctorAudio.capability
        GlassPickerRow(
            Meeting.Copy.desktopAudio,
            selection: Binding(
                get: { settings.desktopAudio },
                set: { model.save(Voice.Change(desktopAudio: $0)) }
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
        let options = Meeting.appOptions(live: model.doctorAudio.apps, stored: stored)
        return GlassPicker(
            items: options.map { PickerItem(value: $0.value, label: $0.label) },
            mode: .multi,
            value: Set(stored.map(\.id)),
            onChange: { picked in
                let apps = options
                    .filter { picked.contains($0.value) }
                    .prefix(Meeting.Limits.apps)
                    .map { Meeting.AudioApp(id: $0.value, name: String($0.label.prefix(Meeting.Limits.appName))) }
                model.save(Voice.Change(desktopAudioApps: Array(apps)))
            },
            emptyText: Meeting.Copy.appsNone,
            title: Meeting.Copy.apps
        ) {
            GlassPickerRowLabel(Meeting.Copy.apps, value: Meeting.appsValue(stored))
        }
        .accessibilityIdentifier(Meeting.TestIds.apps)
    }

    /// #NEW-73: picker over `languages`; every language carries its GPU note
    /// (`languageLabel`), in the row and in the menu.
    private func languageRow(_ settings: Voice.Settings) -> some View {
        let device = settings.voiceDeviceId.map(model.deviceName)
        return GlassPickerRow(
            Voice.Copy.language,
            selection: Binding(
                get: { settings.language },
                set: { model.save(Voice.Change(language: $0)) }
            ),
            options: Voice.languages.map(\.value),
            label: { value in Voice.languageLabel(language: value, deviceName: device) }
        )
        .accessibilityIdentifier(Voice.TestIds.language)
    }

    private func placeholder(_ input: VoiceTextInput) -> String {
        switch input {
        case .wakePhrase: Voice.wakePhrasePlaceholder(model.agentName)
        case .voiceFile: Voice.Copy.voiceFilePlaceholder
        case .voiceTranscript: Voice.Copy.voiceTranscriptPlaceholder
        }
    }

    /// The input row (label + trailing field, the board form's Prefix row):
    /// saved a beat after typing stops, and at once on submit or leave. The
    /// wake phrase's invalid-input line is the tray section's footer.
    private func textRow(_ input: VoiceTextInput) -> some View {
        HStack(spacing: 8) {
            Text(input.title)
                .foregroundStyle(.white.opacity(TextOpacity.primary))
            Spacer(minLength: 8)
            GlassTextField(
                placeholder(input),
                text: Binding(
                    get: { model.draft(input) },
                    set: { next in
                        // #NEW-75: THE input rule ×4 (control characters
                        // dropped, cut to the server's UTF-16 limit).
                        let clean = Voice.textInput(next, max: input.limit)
                        if input == .wakePhrase, clean != model.wakePhrase { model.wakePhraseError = nil }
                        model.setDraft(input, clean)
                        model.scheduleTextSave(input)
                    }
                ),
                bordered: false,
                accessibilityIdentifier: input.testId
            )
            .multilineTextAlignment(.trailing)
            .autocorrectionDisabled()
            .submitLabel(.done)
            .focused($focusedInput, equals: input)
            .onSubmit { model.saveText(input, committed: true) }
            .onChange(of: focusedInput) { old, new in
                if old == input, new != input { model.saveText(input, committed: true) }
            }
        }
    }
}

extension View {
    /// The host side of `VoiceSettingsBlock`: hands the model the
    /// environment, loads + observes, and THE consent prompt (#NEW-57, the
    /// doctor Import's GlassAlert; Cancel leaves Meeting mode off) on a
    /// zero-size node of its own (EXP-1215).
    func voiceSettingsHost(_ model: VoiceSettingsModel) -> some View {
        modifier(VoiceSettingsHost(model: model))
    }
}

private struct VoiceSettingsHost: ViewModifier {
    @Bindable var model: VoiceSettingsModel

    @Environment(AppDependencies.self) private var deps
    @Environment(\.toaster) private var toaster

    func body(content: Content) -> some View {
        content
            .task(id: model.accountId) {
                guard model.specimen == nil else { return }
                model.attach(deps: deps, toaster: toaster)
                await model.load()
            }
            .task(id: model.accountId) {
                guard model.specimen == nil else { return }
                model.attach(deps: deps, toaster: toaster)
                await model.observe()
            }
            .background {
                Color.clear
                    .frame(width: 0, height: 0)
                    .allowsHitTesting(false)
                    .glassAlert(
                        isPresented: $model.meetingConsent,
                        title: Meeting.Copy.consentTitle,
                        message: Meeting.consentBody(
                            who: model.meetingWho, agent: model.agentName, days: Meeting.retentionDays
                        ),
                        actions: [
                            GlassAlertAction(Meeting.Copy.cancel, role: .outline, isCancel: true, id: "cancel") {},
                            GlassAlertAction(Meeting.Copy.start, role: .primary, id: Meeting.TestIds.consentStart) {
                                model.startMeeting()
                            },
                        ]
                    )
            }
    }
}

/// The standalone Voice page (`AppRoute.voiceSettings`, the styleguide
/// specimens): THE block with its Voice device + Agent pickers. S-3: no
/// settings nav entry opens it any more (the Assistant page's Voice section
/// is the block).
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

    @State private var model: VoiceSettingsModel

    init(accountId: String, specimen: Specimen? = nil) {
        self.accountId = accountId
        self.specimen = specimen
        _model = State(initialValue: VoiceSettingsModel(accountId: accountId, specimen: specimen))
    }

    var body: some View {
        ZStack {
            AppBackground()
            Form {
                VoiceSettingsBlock(model: model, pickers: true)
            }
            // EXP-603: the page's own background shows through the grouped list.
            .scrollContentBackground(.hidden)
            .listSectionSpacing(8)
            .tint(DesignTokens.Palette.primary)
        }
        .navigationTitle(Voice.Copy.group)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .voiceSettingsHost(model)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(Voice.TestIds.page)
    }
}

/// Styleguide capture only (`-uiTestingStyleguide settings-voice`,
/// `sg_settings-voice`): THE Voice page as pushed off the account page, over
/// the seed's rows (seed-screenshots.ts: Alex's MacBook Pro as the voice
/// device, Triage as the agent, Speak replies + Push to talk on, German).
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
            speakReplies: true,
            pushToTalk: true,
            pttShortcut: "ctrl+space",
            wakeWord: false,
            wakePhrase: "",
            language: "de"
        ),
        devices: [
            DeviceEntity(id: "specimen-device", userId: userId, deviceId: deviceId, label: "Alex's MacBook Pro", kind: "desktop"),
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

import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// NEW-69 (voice-call.json ENTRY): THE telephone in the agent chat's header,
/// left of `…`, the header's own icon button (the bare toolbar glyph the `…`
/// wears). Disabled = visible and dimmed; #EXP-1878: a tap toasts the
/// reason (`callTap`, voice-call.json `tapCases`).
/// Never in the Agents list.
struct AgentCallButton: View {
    let agentId: String
    let agentName: String
    /// The agent's own icon (nil = the Exponential logo) and colour.
    var agentIcon: String? = nil
    var agentColor: String? = nil
    /// EXP-1857: the agent's Runs on device (the call rings it) and its own
    /// voice language (nil = the member's).
    var agentDevice: String? = nil
    var agentLanguage: String? = nil

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @State private var model = AgentCallAvailability()

    var body: some View {
        let target = model.target(agentDevice: agentDevice)
        let availability = target.availability
        Button {
            let tap = VoiceCall.callTap(.phone, availability)
            if let reason = tap.toast { toaster.info(reason) }
            guard tap.call else { return }
            VoiceCallClient.shared.start(
                accountId: accountId,
                agentId: agentId,
                callee: VoiceCallCallee(name: agentName, icon: agentIcon, color: agentColor),
                device: target.name ?? "",
                language: agentLanguage.flatMap { $0.isEmpty ? nil : $0 } ?? model.settings?.language,
                fallback: target.fallback,
                voiceApi: deps.voiceApi
            )
        } label: {
            ChatHeaderGhostGlyph(
                icon: AppIcons.voiceCall,
                opacity: availability.enabled ? TextOpacity.secondary : TextOpacity.quaternary
            )
        }
        .accessibilityLabel(VoiceCall.Copy.call(agent: agentName))
        .accessibilityHint(availability.reason ?? "")
        .accessibilityIdentifier(VoiceCall.TestIds.call)
        .task(id: accountId) { await model.load(accountId: accountId, voiceApi: deps.voiceApi) }
        .task(id: accountId) { await model.observe(accountId: accountId, userId: deps.auth.userId, db: deps.db) }
        .task { await model.tick() }
    }
}

/// The telephone's inputs (EXP-1857 `callTarget` on a phone: never `here`,
/// never `busy`): the member's voice settings (`voice.get`, fetched when the
/// chat appears; a meeting runs on its voice device), the member's OWN
/// synced desktop rows (newest `lastSeenAt` first: name, liveness and doctor
/// `speech`), and a clock so a device that stops beating reads offline
/// without a sync. Settings not loaded yet read as no desktop (like web).
@MainActor
@Observable
final class AgentCallAvailability {
    private(set) var settings: Voice.Settings?
    private(set) var devices: [DeviceEntity] = []
    private(set) var userId: String?
    private(set) var now = Date()

    nonisolated init() {}

    /// The member's own desktops as `callTarget` reads them, newest first.
    var desktops: [VoiceCall.TargetDevice] { own(kind: "desktop") }

    /// EXP-1858: the member's own servers (only the agent's counts).
    var servers: [VoiceCall.TargetDevice] { own(kind: "server") }

    private func own(kind: String) -> [VoiceCall.TargetDevice] {
        guard let settings else { return [] }
        func seen(_ row: DeviceEntity) -> Date {
            row.lastSeenAt.flatMap(WireTimestamps.parse) ?? .distantPast
        }
        return devices
            .filter { $0.kind == kind && (userId == nil || $0.userId == userId) }
            .sorted { seen($0) > seen($1) }
            .map { row in
                VoiceCall.TargetDevice(
                    id: row.deviceId,
                    name: row.label,
                    online: DeviceLiveness.isOnline(lastSeenAt: row.lastSeenAt, now: now),
                    speech: VoiceCall.callSpeech(ofJSON: row.doctor),
                    // #NEW-57: a meeting runs on the voice device.
                    meeting: settings.meetingOn && row.deviceId == settings.voiceDeviceId
                )
            }
    }

    /// Where a call to the agent running on `agentDevice` speaks.
    func target(agentDevice: String?) -> VoiceCall.Target {
        VoiceCall.callTarget(VoiceCall.TargetInput(
            platform: .phone, here: nil, agentDevice: agentDevice, desktops: desktops, servers: servers, busy: false
        ))
    }

    func load(accountId: String, voiceApi: VoiceApi) async {
        // A failed read keeps the last answer (none = no voice device).
        if let loaded = try? await voiceApi.get(accountId: accountId) { settings = loaded }
    }

    func observe(accountId: String, userId: String?, db: DatabaseManager) async {
        self.userId = userId
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in try DeviceEntity.fetchAll(db) }
        do {
            for try await rows in observation.values(in: pool) { devices = rows }
        } catch {}
    }

    func tick() async {
        while !Task.isCancelled {
            try? await Task.sleep(for: .seconds(15))
            now = Date()
        }
    }
}

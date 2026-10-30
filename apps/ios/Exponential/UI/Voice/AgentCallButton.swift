import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// NEW-69 (voice-call.json ENTRY): THE telephone in the agent chat's header,
/// left of `…`, the header's own icon button (the bare toolbar glyph the `…`
/// wears). Disabled = visible and dimmed; a long press toasts the reason.
/// Never in the Agents list.
struct AgentCallButton: View {
    let agentId: String
    let agentName: String
    /// The agent's own icon (nil = the Exponential logo) and colour.
    var agentIcon: String? = nil
    var agentColor: String? = nil

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @State private var model = AgentCallAvailability()

    var body: some View {
        let availability = model.availability
        Button {
            guard availability.enabled else { return }
            VoiceCallClient.shared.start(
                accountId: accountId,
                agentId: agentId,
                callee: VoiceCallCallee(name: agentName, icon: agentIcon, color: agentColor),
                device: model.device?.label ?? "",
                language: model.settings?.language,
                voiceApi: deps.voiceApi
            )
        } label: {
            AppIcon(AppIcons.voiceCall, size: AppIcon.Size.medium, weight: .medium)
                .foregroundStyle(.white.opacity(availability.enabled ? TextOpacity.secondary : TextOpacity.quaternary))
        }
        .simultaneousGesture(LongPressGesture().onEnded { _ in
            if let reason = availability.reason { toaster.info(reason) }
        })
        .accessibilityLabel(VoiceCall.Copy.call(agent: agentName))
        .accessibilityHint(availability.reason ?? "")
        .accessibilityIdentifier(VoiceCall.TestIds.call)
        .task(id: accountId) { await model.load(accountId: accountId, voiceApi: deps.voiceApi) }
        .task(id: accountId) { await model.observe(accountId: accountId, userId: deps.auth.userId, db: deps.db) }
        .task { await model.tick() }
    }
}

/// The telephone's inputs (`callAvailability` on a phone: never `here`,
/// never `busy`): the member's voice settings (`voice.get`, fetched when the
/// chat appears; its agentId = the member has an Assistant), the voice
/// device's synced row (its name, liveness and doctor `speech`), and a clock so a device that stops beating reads
/// offline without a sync.
@MainActor
@Observable
final class AgentCallAvailability {
    private(set) var settings: Voice.Settings?
    private(set) var devices: [DeviceEntity] = []
    private(set) var userId: String?
    private(set) var now = Date()

    nonisolated init() {}

    /// The voice device's row; nil = none picked, or its row is gone.
    var device: DeviceEntity? {
        guard let id = settings?.voiceDeviceId else { return nil }
        return devices.first { $0.deviceId == id && (userId == nil || $0.userId == userId) }
    }

    var availability: VoiceCall.Availability {
        let device = device
        return VoiceCall.callAvailability(VoiceCall.AvailabilityInput(
            // NEW-99: no voice Assistant yet (`voice.get` agentId) names it first.
            agent: settings?.agentId != nil,
            platform: .phone,
            device: device?.label,
            online: DeviceLiveness.isOnline(lastSeenAt: device?.lastSeenAt, now: now),
            here: false,
            speech: VoiceCall.callSpeech(ofJSON: device?.doctor),
            busy: false,
            // #NEW-57: a meeting on the voice device disables the telephone.
            meeting: settings?.meetingOn ?? false
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

import ExpCore
import ExpUI
import GRDB
import SwiftUI

// #EXP-2161 (member-space.json, UI ×4; web `assistant-device-row.tsx`): the
// Assistant page's Runs on device row: the member's own machines
// (`runsOnRows`), then one section per team with opted-in hosts of others
// (`spaceSections`), headed by the team's name. Picking one of those confirms
// (`copy.pick*`) and is `memberSpaces.request`; a bound space shows as its
// host's row. An Assistant without a device (its space ended) reads
// `copy.noDevice`; one whose space waits for approval, `copy.waiting`.

/// The raw device rows (a space resolves here, never in a list), the member's
/// teams and the users' names the row reads, plus whether the bound space's
/// install waits for this member's approval.
@MainActor
@Observable
final class AssistantSpaceModel {
    private(set) var rows: [DeviceEntity] = []
    private(set) var names: [String: String] = [:]
    /// The space id whose install waits (nil = none or ready).
    private(set) var waitingSpaceId: String?
    private var me: String?

    /// How often a space that is not let in yet is read again.
    private static let waitingInterval: Duration = .seconds(15)

    func observe(deps: AppDependencies, accountId: String) async {
        me = deps.auth.userId
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in
            (try DeviceEntity.fetchAll(db), try UserEntity.fetchAll(db))
        }
        do {
            for try await (devices, users) in observation.values(in: pool) {
                rows = devices
                names = Dictionary(users.map { ($0.id, $0.name ?? $0.email) }, uniquingKeysWith: { a, _ in a })
            }
        } catch {}
    }

    /// Polls `memberSpaces.mine` while the bound space is not `ready`.
    func watchWaiting(deps: AppDependencies, accountId: String, spaceRowId: String?) async {
        guard let spaceRowId else {
            waitingSpaceId = nil
            return
        }
        while !Task.isCancelled {
            let status = try? await deps.devicesApi.myMemberSpace(accountId: accountId).space
            let mine = status?.spaceId == spaceRowId ? status?.status : nil
            waitingSpaceId = mine == "waiting" ? spaceRowId : nil
            if mine == "ready" { return }
            try? await Task.sleep(for: Self.waitingInterval)
        }
    }

    func space(boundId: String?) -> DeviceEntity? {
        guard let boundId, !boundId.isEmpty else { return nil }
        return rows.first { $0.deviceId == boundId && $0.isMemberSpace }
    }

    func picks(teams: [TeamEntity], boundId: String?) -> (picks: [MemberSpace.Pick], value: String?) {
        MemberSpace.picks(rows: rows, teams: teams.map { (id: $0.id, name: $0.name) }, boundId: boundId, me: me ?? "")
    }

    /// What the Account and Model rows read while bound to a space.
    func accountDevice(boundId: String?) -> SteerDevice? {
        guard let space = space(boundId: boundId) else { return nil }
        return MemberSpace.accountDevice(space: space, host: rows.first { $0.id == space.spaceHostId }, me: me)
    }

    func waiting(boundId: String?) -> Bool {
        guard let space = space(boundId: boundId) else { return false }
        return waitingSpaceId == space.id
    }

    func ownerName(_ pick: MemberSpace.Pick) -> String { names[pick.ownerId] ?? "" }
}

struct AssistantDeviceRow: View {
    @Bindable var draft: AgentSettingsDraft
    /// The member's own machines.
    let devices: [SteerDevice]
    let space: AssistantSpaceModel
    let canEdit: Bool
    let title: String
    /// `memberSpaces.request` went through: the space's device id.
    let onPickedSpace: (String) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(TeamState.self) private var teamState
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @State private var confirm: MemberSpace.Pick?

    private var bound: DeviceEntity? { space.space(boundId: draft.deviceId) }

    private var spacePicks: (picks: [MemberSpace.Pick], value: String?) {
        space.picks(teams: teamState.orderedTeams, boundId: draft.deviceId)
    }

    /// The member's own rows (web `runsOnPickerRows`); a bound space is no
    /// own row, never its raw id.
    private var own: [DevicePickerDevice] {
        let byId = Dictionary(devices.map { ($0.deviceId, $0) }, uniquingKeysWith: { a, _ in a })
        return AgentSettings.runsOnRows(
            devices.map { (deviceId: $0.deviceId, kind: $0.kind, online: $0.isOnline) },
            boundId: bound == nil ? draft.deviceId : nil,
            assistant: true
        ).map { row in
            guard let device = byId[row.id] else { return DevicePickerDevice(id: row.id, name: row.id) }
            let base = DevicePickerDevice(device)
            return DevicePickerDevice(
                id: base.id, name: base.name, icon: base.icon,
                description: row.description ?? base.description, disabled: row.disabled
            )
        }
    }

    private var rows: [DevicePickerDevice] {
        let (picks, value) = spacePicks
        // A space whose host is gone from the list names itself.
        let orphan = bound.flatMap { space in
            value == nil
                ? DevicePickerDevice(
                    id: space.deviceId, name: space.label.isEmpty ? space.deviceId : space.label,
                    icon: DeviceIconDisplay.iconName(for: space)
                )
                : nil
        }
        let teamRows = picks.map { pick in
            DevicePickerDevice(
                id: pick.value, name: pick.label, icon: DeviceIconDisplay.iconName(icon: pick.icon, kind: pick.kind),
                description: pick.online ? nil : DevicesList.Copy.offline, disabled: pick.disabled,
                section: pick.title, checked: pick.selected
            )
        }
        return (orphan.map { [$0] } ?? []) + own + teamRows
    }

    private var value: String? {
        guard let bound else { return draft.deviceId }
        return spacePicks.value ?? bound.deviceId
    }

    private var valueLabel: String {
        let id = value
        if let row = rows.first(where: { $0.id == id }) { return row.name }
        return id.flatMap { $0.isEmpty ? nil : $0 } ?? MemberSpace.Copy.noDevice
    }

    var body: some View {
        DevicePicker(
            devices: rows,
            value: value,
            onChange: pick,
            trigger: {
                GlassPickerRowLabel(title, value: valueLabel)
            }
        )
        .disabled(!canEdit)
        .accessibilityIdentifier("agent-device-row")
        .background(
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(item: $confirm) { pick in
                    GlassAlert(
                        prompt: MemberSpace.pickPrompt(device: pick.label, owner: space.ownerName(pick)),
                        handlers: ["pick": { Task { await request(pick) } }]
                    )
                }
        )
        if space.waiting(boundId: draft.deviceId) {
            Text(MemberSpace.Copy.waiting)
                .font(.exp(size: DesignTokens.Typography.Size.sm))
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .frame(maxWidth: .infinity, alignment: .leading)
                .accessibilityIdentifier("assistant-space-waiting")
        }
    }

    private func pick(_ next: String) {
        guard let target = spacePicks.picks.first(where: { $0.value == next }) else {
            draft.selectDevice(next, devices: devices)
            return
        }
        if target.selected { return }
        confirm = target
    }

    private func request(_ pick: MemberSpace.Pick) async {
        do {
            let result = try await deps.devicesApi.requestMemberSpace(
                accountId: accountId, deviceId: pick.hostDeviceId, teamId: pick.teamId
            )
            onPickedSpace(result.deviceId)
        } catch {
            toaster.error(FeedbackStates.Errors.title(verb: "save", thing: "the device"), description: error.userFacingMessage)
        }
    }
}

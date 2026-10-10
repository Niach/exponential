import ExpCore
import ExpUI
import SwiftUI

/// The readiness block's Import pill, as ONE flow every host attaches
/// (device settings' block, the composer's not-ready row — fixture rule "same
/// row, same action"): confirm (`Import {email}?`, the pinned body, Cancel /
/// Import), then queue `agent_login {agent, import: true}` on the machine (cap
/// `agent-import`, which `DeviceReadiness` already gated the pill on).
///
/// A refusal (the device's own sentence) toasts. The MATERIAL outcome lands
/// by sync — the agent row turns ok — and an import that moved into a login
/// the machine already had (same email) raises the duplicate toast, read off
/// the heartbeat (`AgentAccountsRows.loginLanding`, no intended profile).
struct AgentImportTarget: Identifiable, Equatable {
    let deviceId: String
    let agent: String
    let email: String

    var id: String { "\(deviceId):\(agent)" }

    /// The target an Import pill on `row` asks for; nil when the row offers
    /// none.
    init?(row: DeviceReadiness.Row, device: SteerDevice) {
        guard let email = row.importEmail else { return nil }
        deviceId = device.deviceId
        agent = row.key
        self.email = email
    }
}

extension View {
    /// Attach the Import confirm + command to a host. `devices` = the live
    /// machines (the duplicate toast watches their heartbeat).
    func agentImportFlow(
        _ target: Binding<AgentImportTarget?>,
        devices: [SteerDevice]
    ) -> some View {
        modifier(AgentImportFlow(target: target, devices: devices))
    }
}

private struct AgentImportFlow: ViewModifier {
    @Binding var target: AgentImportTarget?
    let devices: [SteerDevice]

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster

    /// A queued Import: every profile's `lastLoginAt` when it was queued.
    private struct Pending: Equatable {
        let deviceId: String
        let agent: String
        let baseline: [String: String]
    }

    /// Keyed by `AgentImportTarget.id`.
    @State private var pending: [String: Pending] = [:]

    func body(content: Content) -> some View {
        content
            // One presentation per node: the confirm hangs off a zero-size
            // node of its own.
            .background(
                Color.clear
                    .frame(width: 0, height: 0)
                    .allowsHitTesting(false)
                    .glassAlert(item: $target) { target in
                        GlassAlert(
                            title: DeviceReadiness.importTitle(email: target.email),
                            message: DeviceReadiness.importBody,
                            actions: [
                                GlassAlertAction("Cancel", role: .outline, isDefault: false, isCancel: true, id: "cancel") {},
                                GlassAlertAction("Import", role: .primary, id: "import") {
                                    queue(target)
                                },
                            ]
                        )
                    }
            )
            .onChange(of: landings) { _, landings in
                for (key, landing) in landings {
                    pending[key] = nil
                    if landing.duplicate {
                        toaster.warning(AgentAccountsRows.alreadyAddedToast(landing))
                    }
                }
            }
    }

    /// Where each queued Import landed, once it has.
    private var landings: [String: AgentAccountsRows.LoginLanding] {
        var out: [String: AgentAccountsRows.LoginLanding] = [:]
        for (key, entry) in pending {
            let account = devices.first { $0.deviceId == entry.deviceId }?.agentAccounts?[entry.agent]
            if let landing = AgentAccountsRows.loginLanding(
                account: account, baseline: entry.baseline, intendedProfileId: nil
            ) {
                out[key] = landing
            }
        }
        return out
    }

    private func queue(_ target: AgentImportTarget) {
        guard pending[target.id] == nil else { return }
        let account = devices.first { $0.deviceId == target.deviceId }?.agentAccounts?[target.agent]
        pending[target.id] = Pending(
            deviceId: target.deviceId,
            agent: target.agent,
            baseline: AgentAccountsRows.loginBaseline(account)
        )
        let api = deps.devicesApi
        let accountId = accountId
        Task {
            do {
                let created = try await api.createCommand(
                    accountId: accountId,
                    deviceId: target.deviceId,
                    kind: "agent_login",
                    agent: target.agent,
                    importLogin: true
                )
                // Bounded 2s polls, like the sign-in sheet: an offline
                // machine keeps the command queued and sync carries the rest.
                for _ in 0..<30 {
                    try? await Task.sleep(for: .seconds(2))
                    guard let command = try? await api.getCommand(
                        accountId: accountId, commandId: created.id
                    ) else { continue }
                    guard !command.isPending else { continue }
                    if command.isFailed {
                        pending[target.id] = nil
                        toaster.error(command.result ?? "The device refused the command.")
                    }
                    break
                }
            } catch {
                pending[target.id] = nil
                toaster.error(error.userFacingMessage)
            }
        }
    }
}

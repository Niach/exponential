import ExpUI
import ExpCore
import SwiftUI

/// Step 4 of the first-run wizard (EXP-725): point the user at the two places
/// a coding session can actually run.
///
/// LAST on purpose — it is the one step whose work happens on ANOTHER machine,
/// so it must not stand between a new user and their first board. Both
/// sub-cards reuse the getting-started checklist's copy (`desktop*` /
/// `server*`), because they are the same two steps said once: download the
/// desktop app, or copy the daemon install one-liner for an always-on box.
///
/// Under the cards, the caller's OWN registered machines off the synced
/// `devices` shape — the same rows (and the same settings sheet) the Devices
/// tab renders, so a machine that signed in while the wizard was open shows up
/// here and the trailing button turns from "Skip for now" into "Continue".
struct OnboardingDevicesStep: View {
    let accountId: String
    /// The wizard's trailing button, rendered by the host below this view —
    /// the step only reports whether at least one own machine exists.
    let onDevicesChanged: (Bool) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.openURL) private var openURL

    @State private var viewModel: AgentsViewModel?
    @State private var settingsTarget: DeviceSettingsTarget?
    /// The pasteboard is silent — the server card's pill says so for 2s.
    @State private var copiedInstall = false
    @State private var copyFlashTask: Task<Void, Never>?

    /// The sheet's target is the ID only: the sheet reads the LIVE row itself
    /// (EXP-490), so a captured value would only go stale under it.
    private struct DeviceSettingsTarget: Identifiable {
        let id: String
    }

    /// Own machines only. The wizard never lists a teammate's shared server:
    /// this step is about the user's OWN setup.
    private var myDevices: [SteerDevice] {
        viewModel?.devices?.filter(\.isMine) ?? []
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            desktopCard
            serverCard

            GlassSectionHeader(OnboardingCopy.devicesYours)

            if myDevices.isEmpty {
                Text(OnboardingCopy.devicesNone)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 12)
                    .glassRow()
            } else {
                ForEach(myDevices) { device in
                    deviceRow(device)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("onboarding-devices-step")
        .onAppear {
            if viewModel == nil {
                viewModel = AgentsViewModel(
                    accountId: accountId, userId: deps.auth.userId, db: deps.db
                )
            }
            viewModel?.startObserving()
        }
        .onDisappear {
            viewModel?.stopObserving()
            copyFlashTask?.cancel()
        }
        .onChange(of: myDevices.isEmpty) { _, empty in
            onDevicesChanged(!empty)
        }
        .sheet(item: $settingsTarget) { target in
            if let viewModel {
                // No team list: sharing a machine with a team is a later
                // decision, and the wizard's user is in exactly one team.
                DeviceSettingsSheet(
                    viewModel: viewModel, deviceId: target.id, teams: []
                )
            }
        }
    }

    // MARK: - The two install cards

    private var desktopCard: some View {
        installCard(
            icon: AppIcons.uiDevice,
            title: GettingStartedCopy.desktopTitle,
            description: GettingStartedCopy.desktopDescription,
            actionLabel: GettingStartedCopy.desktopAction
        ) {
            openURL(AppConstants.desktopReleasesUrl)
        }
    }

    private var serverCard: some View {
        installCard(
            icon: AppIcons.uiServer,
            title: GettingStartedCopy.serverTitle,
            description: GettingStartedCopy.serverDescription,
            actionLabel: copiedInstall
                ? OnboardingCopy.inviteCopied
                : GettingStartedCopy.serverAction
        ) {
            copyInstallSnippet()
        }
    }

    private func installCard(
        icon: String,
        title: String,
        description: String,
        actionLabel: String,
        action: @escaping () -> Void
    ) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 10) {
                AppIcon(icon, size: AppIcon.Size.medium)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text(title)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Spacer(minLength: 8)
            }

            Text(description)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)

            GlassPill(actionLabel, size: .sm, mode: .action(action))
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .glassCard()
    }

    // MARK: - Machine rows

    /// The Devices tab's row, minus the launcher and the overflow menu — there
    /// is nothing to code on yet, and the wizard's only affordance is opening
    /// the settings sheet to sign an agent in.
    private func deviceRow(_ device: SteerDevice) -> some View {
        Button {
            settingsTarget = DeviceSettingsTarget(id: device.deviceId)
        } label: {
            HStack(spacing: 12) {
                AppIcon(
                    device.isServer ? AppIcons.uiServer : AppIcons.uiDevice,
                    size: AppIcon.Size.medium
                )
                .foregroundStyle(.white.opacity(TextOpacity.secondary))

                VStack(alignment: .leading, spacing: 3) {
                    Text(deviceName(device))
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                    statusLine(device)
                }

                Spacer(minLength: 0)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 12)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .glassRow()
        .opacity(device.needsAgentSignIn ? 0.6 : 1)
    }

    /// The Devices tab's live/offline caption, minus the update states the
    /// wizard can never produce (EXP-409: signed-out agents replace "Online"
    /// when nothing is runnable, and annotate it otherwise).
    @ViewBuilder
    private func statusLine(_ device: SteerDevice) -> some View {
        let signedOut = device.unauthedAgentIds.joined(separator: ", ")
        let signInNeeded = device.needsAgentSignIn
        HStack(spacing: 5) {
            if device.isOnline {
                Circle()
                    .fill(signInNeeded ? DesignTokens.Semantic.yellow : DesignTokens.Semantic.green)
                    .frame(width: 6, height: 6)
                if signInNeeded {
                    Text("\(signedOut) not signed in")
                } else {
                    Text("Online")
                    if !signedOut.isEmpty {
                        Text("· \(signedOut) not signed in")
                            .foregroundStyle(.white.opacity(TextOpacity.quaternary))
                            .lineLimit(1)
                    }
                }
            } else {
                Text("Offline")
            }
        }
        .font(.caption)
        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
    }

    private func deviceName(_ device: SteerDevice) -> String {
        device.deviceLabel.isEmpty ? device.deviceId : device.deviceLabel
    }

    private func copyInstallSnippet() {
        guard ServerInstallSnippet.copy(accountId: accountId, auth: deps.auth) else { return }
        copiedInstall = true
        copyFlashTask?.cancel()
        copyFlashTask = Task {
            try? await Task.sleep(for: .seconds(2))
            if !Task.isCancelled { copiedInstall = false }
        }
    }
}

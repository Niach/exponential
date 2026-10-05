import ExpUI
import ExpCore
import SwiftUI

/// The ONE device setup block (EXP-725, EXP-1169): point the user at the two
/// places a coding session can actually run. Four hosts render it unchanged:
///
///   1. the first-run wizard's step 4 (create path), via `DevicesStepView`
///   2. the join step after `teamInvites.accept` when the caller owns no
///      machine, via `DevicesStepView` (wizard, invite deep link, team sheet)
///   3. `AddDeviceSheet`, the Devices tab's "Add device" pill
///   4. the coding readiness sheet's server fix, via `AddDeviceSheet`
///
/// Both sub-cards reuse the getting-started checklist's copy (`desktop*` /
/// `server*`), because they are the same two steps said once: download the
/// desktop app, or copy the daemon install one-liner for an always-on box.
///
/// Under the cards, ONLY for the onboarding hosts (`listsDevices`), the
/// caller's OWN registered machines off the synced `devices` shape: the same
/// rows (and the same settings sheet) the Devices tab renders, so a machine
/// that signed in while the step was open shows up here and the trailing
/// button turns from "Skip for now" into "Continue". The Add device sheet
/// opens over the tab that already lists them, so it shows the cards alone.
struct DeviceSetup: View {
    let accountId: String
    /// The onboarding hosts list the caller's machines under the cards; the
    /// Add device sheet does not.
    var listsDevices = true
    /// A host's trailing button, rendered by the host below this view. The
    /// block only reports whether at least one own machine exists.
    let onDevicesChanged: (Bool) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.openURL) private var openURL

    @State private var viewModel: AgentsViewModel?
    @State private var settingsTarget: DeviceSettingsTarget?
    /// The server card's one-time install token (`devices.createInstallToken`).
    /// Nil until it lands, and after any failed mint: the box then shows the
    /// plain command.
    @State private var installToken: String?
    /// The pasteboard is silent: the copy control shows a check for 1.5s.
    @State private var copyFlash = false
    @State private var copyFlashTask: Task<Void, Never>?
    /// The code field only exists once the command was copied: that is the
    /// moment a CLI code can turn up.
    @State private var commandCopied = false
    @State private var userCode = ""
    @State private var codeBusy = false
    @State private var codeError: DeviceCodeError?
    @State private var codeApproved = false

    /// The sheet's target is the ID only: the sheet reads the LIVE row itself
    /// (EXP-490), so a captured value would only go stale under it.
    private struct DeviceSettingsTarget: Identifiable {
        let id: String
    }

    /// Own machines only. The block never lists a teammate's shared server:
    /// it is about the user's OWN setup.
    private var myDevices: [SteerDevice] {
        viewModel?.devices?.filter(\.isMine) ?? []
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            desktopCard
            serverCard

            if listsDevices {
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
            copyFlash = false
            installToken = nil
            commandCopied = false
            userCode = ""
            codeError = nil
            codeApproved = false
        }
        // Cancelled on disappear, which orphans an in-flight mint.
        .task { await mintInstallTokens() }
        .onChange(of: myDevices.isEmpty) { _, empty in
            onDevicesChanged(!empty)
        }
        .sheet(item: $settingsTarget) { target in
            if let viewModel {
                // No team list: sharing a machine with a team is a later
                // decision, made from the Devices tab's own rows.
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

    /// EXP-1169: the same server card on all four clients (web's
    /// `device-setup.tsx` is the reference): the install command in a box with
    /// a copy control, then, once copied, the field that approves the CLI's
    /// device code in place.
    private var serverCard: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 10) {
                AppIcon(AppIcons.uiServer, size: AppIcon.Size.medium)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text(GettingStartedCopy.serverTitle)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Spacer(minLength: 8)
            }

            Text(GettingStartedCopy.serverDescription)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)

            if let origin = ServerInstallSnippet.origin(accountId: accountId, auth: deps.auth) {
                commandBox(origin: origin)
            }

            if codeApproved {
                Text(DeviceSetupCopy.approved)
                    .font(.caption)
                    .foregroundStyle(.white)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("device-code-approved")
            } else if commandCopied {
                codeEntry
            }
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .glassCard()
    }

    private func commandBox(origin: String) -> some View {
        ZStack(alignment: .topTrailing) {
            Text(Self.breakAnywhere(ServerInstallCommand.displayed(origin: origin, token: installToken)))
                .font(.caption.monospaced())
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(10)
                .padding(.trailing, 28)
                .accessibilityIdentifier("install-snippet")

            Button {
                copyInstallCommand(origin: origin)
            } label: {
                AppIcon(copyFlash ? AppIcons.uiCheck : AppIcons.uiCopy, size: AppIcon.Size.small)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .frame(width: 32, height: 32)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .padding(2)
            .accessibilityLabel(DeviceSetupCopy.copyCommand)
            .accessibilityIdentifier("install-snippet-copy")
        }
        .background(GlassTokens.fillCard, in: RoundedRectangle(cornerRadius: GlassTokens.fieldRadius))
        .overlay(
            RoundedRectangle(cornerRadius: GlassTokens.fieldRadius)
                .stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
        )
    }

    private var codeEntry: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(DeviceSetupCopy.codeLabel)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .fixedSize(horizontal: false, vertical: true)

            HStack(spacing: 8) {
                GlassTextField(
                    DeviceSetupCopy.codePlaceholder,
                    text: Binding(
                        get: { userCode },
                        set: { typed in
                            userCode = DeviceUserCode.normalize(typed)
                            codeError = nil
                        }
                    ),
                    accessibilityIdentifier: "device-code-field"
                )
                .font(.subheadline.monospaced())
                .textInputAutocapitalization(.characters)
                .autocorrectionDisabled()
                .onSubmit { approveCode() }

                GlassPill(
                    DeviceSetupCopy.approve,
                    mode: .action { approveCode() },
                    enabled: DeviceUserCode.isComplete(userCode) && !codeBusy
                )
            }

            if let codeError {
                Text(DeviceSetupCopy.message(codeError))
                    .font(.caption)
                    .foregroundStyle(DesignTokens.Palette.destructive)
                    .fixedSize(horizontal: false, vertical: true)
            }
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

    /// The web box wraps with `break-all`: a long token breaks where the line
    /// ends, not at the nearest word boundary (which strands the indent on an
    /// empty line). Text has no such mode, so the DISPLAYED string gets a
    /// zero-width break opportunity after every character. The pasteboard
    /// never sees this string, which is also why the box is not selectable:
    /// the copy control is the one way out.
    private static func breakAnywhere(_ text: String) -> String {
        text.split(separator: "\n", omittingEmptySubsequences: false)
            .map { line in line.map(String.init).joined(separator: "\u{200B}") }
            .joined(separator: "\n")
    }

    // MARK: - Machine rows

    /// The Devices tab's row, minus the launcher and the overflow menu: the
    /// block is about setting a machine up, and its only affordance is opening
    /// the settings sheet to sign an agent in.
    private func deviceRow(_ device: SteerDevice) -> some View {
        Button {
            settingsTarget = DeviceSettingsTarget(id: device.deviceId)
        } label: {
            HStack(spacing: 12) {
                // EXP-924: the owner's pick, else the kind default.
                AppIcon(
                    DeviceIconDisplay.iconName(for: device),
                    size: AppIcon.Size.medium
                )
                .foregroundStyle(.white.opacity(TextOpacity.secondary))

                VStack(alignment: .leading, spacing: 3) {
                    Text(deviceName(device))
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                    statusLine(device)
                    // EXP-1196/1219: a machine reporting a doctor shows the
                    // rows that need attention, not the signed-out line.
                    if let doctor = device.doctor {
                        let rows = DeviceReadiness.attentionRows(doctor, remote: true)
                        if !rows.isEmpty {
                            DeviceReadinessView(compactRows: rows)
                        }
                    }
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

    /// The Devices tab's live/offline caption, minus the update states a
    /// freshly set up machine can never produce (EXP-409: signed-out agents replace "Online"
    /// when nothing is runnable, and annotate it otherwise).
    @ViewBuilder
    private func statusLine(_ device: SteerDevice) -> some View {
        // EXP-1196: with a doctor report its rows say what is missing.
        let signedOut = device.doctor == nil ? device.unauthedAgentIds.joined(separator: ", ") : ""
        let signInNeeded = device.doctor == nil && device.needsAgentSignIn
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

    private func copyInstallCommand(origin: String) {
        Platform.copyToPasteboard(ServerInstallCommand.copied(origin: origin, token: installToken))
        commandCopied = true
        copyFlash = true
        copyFlashTask?.cancel()
        copyFlashTask = Task {
            try? await Task.sleep(for: .seconds(1.5))
            if !Task.isCancelled { copyFlash = false }
        }
    }

    /// Mint the install token, and remint whenever it lapses while the block
    /// is visible. Any failure (offline, the mint rate limit) is silent: the
    /// box keeps the plain command.
    private func mintInstallTokens() async {
        while !Task.isCancelled {
            guard let minted = try? await deps.devicesApi.createInstallToken(accountId: accountId),
                  !Task.isCancelled else {
                if !Task.isCancelled { installToken = nil }
                return
            }
            installToken = minted.token
            guard let expiresAt = minted.expiresAtDate else { return }
            try? await Task.sleep(for: .seconds(max(0, expiresAt.timeIntervalSinceNow)))
        }
    }

    /// The /auth/device page's claim + approve, in place.
    private func approveCode() {
        guard !codeBusy, DeviceUserCode.isComplete(userCode) else { return }
        let code = userCode
        codeBusy = true
        codeError = nil
        Task {
            let error = await deps.authApi.approveDeviceCode(accountId: accountId, userCode: code)
            codeBusy = false
            if let error {
                codeError = error
            } else {
                codeApproved = true
            }
        }
    }
}

import ExpCore
import ExpUI
import SwiftUI

/// EXP-1126: the account's sign-in methods on the server screen — web
/// `SignInMethodsSection` + `PasskeysSection` parity. Two bands over flat rows:
///
/// - "Sign-in methods": the email-code row (Change → `ChangeEmailSheet`), then
///   one row per provider in the server's render order (Apple, Google, OIDC,
///   the password while one is set). Link = the LINK-mode browser handoff
///   (`SignInLinkSession`); Unlink / Remove confirm first and are disabled
///   while the row is the last way in.
/// - "Passkeys": list + confirmed Remove, and (EXP-1596, web + Android
///   parity) the band's trailing "Add passkey" while the instance offers
///   passkeys: the platform authenticator registers one
///   (`PasskeyRegistrationCeremony`).
///
/// Every call takes the SCREEN's `accountId` — Settings can show a
/// non-active account.
struct SignInMethodsSection: View {
    let accountId: String

    @Environment(AppDependencies.self) private var deps
    @State private var methods: SignInMethods?
    // FEED-79: a failed load = the band with only its Retry (no toast).
    @State private var loadFailed = false
    @Environment(\.toaster) private var toaster
    @State private var confirm: Confirm?
    @State private var busyId: String?
    @State private var pendingLink: String?
    @State private var showChangeEmail = false
    @State private var linkSession = SignInLinkSession()
    // EXP-1596: an Add passkey ceremony in flight; the band's pill spins.
    @State private var addingPasskey = false
    @State private var passkeyCeremony = PasskeyRegistrationCeremony()

    /// Web's name for a passkey made on an iPhone (`suggestedPasskeyName`).
    private static let passkeyName = "iPhone"

    private enum Confirm: Identifiable {
        case unlink(SignInProvider)
        case removePasskey(SignInPasskey)

        var id: String {
            switch self {
            case let .unlink(provider): "provider:\(provider.id)"
            case let .removePasskey(passkey): "passkey:\(passkey.id)"
            }
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            if let methods {
                methodsBand(methods)
                passkeysBand(methods)
            } else {
                VStack(alignment: .leading, spacing: 0) {
                    GlassSectionBand("Sign-in methods")
                    HStack(spacing: 8) {
                        if loadFailed {
                            GlassPill(FeedbackStates.Errors.retry, mode: .action {
                                Task { await load(retried: true) }
                            })
                            Spacer()
                        } else {
                            ProgressView().tint(.white)
                            Spacer()
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 10)
                    .flatRow()
                }
            }
        }
        .task { await load() }
        .onReceive(NotificationCenter.default.publisher(for: .signInMethodsChanged)) { _ in
            Task { await load() }
        }
        .onDisappear { linkSession.cancel() }
        .sheet(isPresented: $showChangeEmail) {
            ChangeEmailSheet(
                accountId: accountId,
                currentEmail: methods?.email ?? ""
            ) { email in
                toaster.success("Your email is now \(email).")
                Task { await load() }
            }
        }
        // EXP-1215: the app's own alert card (`GlassAlert`), ×4.
        .glassAlert(item: $confirm) { target in
            let copy = prompt(target)
            // The ONE answer that is not Cancel (Unlink, Remove) performs it.
            let answer = copy.actions.first { $0.role != .cancel }?.id ?? ""
            return GlassAlert(prompt: copy, handlers: [answer: { Task { await perform(target) } }])
        }
    }

    // MARK: - Bands

    private func methodsBand(_ methods: SignInMethods) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            GlassSectionBand("Sign-in methods")

            emailRow(methods)

            ForEach(methods.providers) { provider in
                providerRow(provider, in: methods)
            }
        }
    }

    private func passkeysBand(_ methods: SignInMethods) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            // EXP-1596: the band's Add passkey action, web's (an https
            // instance only: `passkeyEnabled`).
            GlassSectionBand("Passkeys") {
                if methods.passkeyEnabled {
                    GlassPill(
                        "Add passkey",
                        mode: .action { Task { await addPasskey() } },
                        enabled: !addingPasskey
                    ) {
                        if addingPasskey {
                            ProgressView().controlSize(.mini).tint(.white)
                        }
                    }
                    .accessibilityIdentifier("passkey-add")
                }
            }

            if methods.passkeys.isEmpty {
                EmptyState(.passkeys)
            } else {
                ForEach(methods.passkeys, id: \.id) { passkey in
                    passkeyRow(passkey, in: methods)
                }
            }
        }
    }

    // MARK: - Rows

    private func emailRow(_ methods: SignInMethods) -> some View {
        methodRow(
            title: "Email code",
            // ×4: "{email}", or "{email} · Off on this server" without a
            // mail transport (`SignInMethods.emailCodeSubtitle`).
            subtitle: methods.emailCodeSubtitle
        ) {
            AppIcon(AppIcons.uiMail, size: AppIcon.Size.medium)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        } trailing: {
            if methods.emailOtpEnabled {
                GlassPill("Change", mode: .action { showChangeEmail = true })
                    .accessibilityIdentifier("sign-in-methods-change-email")
            }
        }
    }

    private func providerRow(_ provider: SignInProvider, in methods: SignInMethods) -> some View {
        let blocked = provider.linked && !SignInMethods.canUnlink(provider, in: methods)
        return methodRow(title: provider.name, subtitle: providerSubtitle(provider, blocked: blocked)) {
            providerMark(provider)
        } trailing: {
            if provider.linked {
                GlassPill(
                    busyId == provider.id ? "Removing…" : (provider.isPassword ? "Remove" : "Unlink"),
                    mode: .action {
                        confirm = .unlink(provider)
                    },
                    tint: blocked ? nil : .red,
                    enabled: !blocked && busyId == nil
                )
            } else if provider.available {
                GlassPill(
                    pendingLink == provider.id ? "Redirecting…" : "Link",
                    mode: .action { Task { await link(provider) } },
                    enabled: pendingLink == nil
                )
            }
        }
    }

    private func passkeyRow(_ passkey: SignInPasskey, in methods: SignInMethods) -> some View {
        let blocked = !methods.canRemovePasskey
        return methodRow(title: passkey.displayName, subtitle: passkeySubtitle(passkey, blocked: blocked)) {
            AppIcon(AppIcons.authPasskey, size: AppIcon.Size.medium)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        } trailing: {
            GlassPill(
                busyId == passkey.id ? "Removing…" : "Remove",
                mode: .action {
                    confirm = .removePasskey(passkey)
                },
                tint: blocked ? nil : .red,
                enabled: !blocked && busyId == nil
            )
        }
    }

    private func methodRow<Leading: View, Trailing: View>(
        title: String,
        subtitle: String,
        @ViewBuilder leading: () -> Leading,
        @ViewBuilder trailing: () -> Trailing
    ) -> some View {
        HStack(spacing: 12) {
            leading()
                .frame(width: 22)
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(2)
            }
            Spacer(minLength: 8)
            trailing()
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .flatRow()
    }

    @ViewBuilder
    private func providerMark(_ provider: SignInProvider) -> some View {
        switch provider.kind {
        case "apple":
            // Apple's brand mark, not a registry glyph (login-screen exception:
            // Lucide has no Apple logo, guideline 4.8).
            Image(systemName: "apple.logo")
                .font(.body.weight(.medium))
                .foregroundStyle(.white)
        case "google":
            GoogleLogoMark()
                .frame(width: 17, height: 17)
        case "password":
            AppIcon(AppIcons.uiPrivate, size: AppIcon.Size.medium)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        default:
            EmptyView()
        }
    }

    private func providerSubtitle(_ provider: SignInProvider, blocked: Bool) -> String {
        guard provider.linked else { return "Not linked" }
        if blocked { return "Linked · your only way to sign in" }
        guard provider.available else { return "Linked · no longer offered on this instance" }
        if let date = provider.linkedAt.flatMap(WireTimestamps.parse) {
            return "Linked \(Self.formatDate(date))"
        }
        return "Linked"
    }

    /// EXP-1209: the last way in reads like the web row, `added <date> · your only way to sign in`.
    private func passkeySubtitle(_ passkey: SignInPasskey, blocked: Bool) -> String {
        var parts: [String] = []
        if let date = passkey.createdAt.flatMap(WireTimestamps.parse) {
            parts.append("added \(Self.formatDate(date))")
        }
        if blocked {
            parts.append("your only way to sign in")
        } else if passkey.backedUp {
            parts.append("synced across your devices")
        }
        return parts.isEmpty ? "Passkey" : parts.joined(separator: " · ")
    }

    private static func formatDate(_ date: Date) -> String {
        date.formatted(date: .abbreviated, time: .omitted)
    }

    // MARK: - Confirm copy (EXP-1215: the contract `prompts.json`)

    private func prompt(_ target: Confirm) -> PromptCopy {
        switch target {
        case let .unlink(provider):
            provider.isPassword
                ? Prompts.RemovePassword.copy()
                : Prompts.UnlinkSignInMethod.copy(provider: provider.name)
        case let .removePasskey(passkey):
            Prompts.RemovePasskey.copy(name: passkey.displayName)
        }
    }

    // MARK: - Actions

    /// FEED-79: the list loads on appear and on change notices, so a failure
    /// is quiet (the band's Retry); only a failed Retry tap toasts.
    private func load(retried: Bool = false) async {
        do {
            let next = try await deps.usersApi.signInMethods(accountId: accountId)
            methods = next
            loadFailed = false
        } catch {
            if methods == nil {
                loadFailed = true
                guard retried else { return }
                toaster.error(
                    FeedbackStates.Errors.title(verb: "load", thing: "sign-in methods"),
                    description: error.trpcUserMessage
                )
            }
        }
    }

    private func perform(_ target: Confirm) async {
        confirm = nil
        do {
            switch target {
            case let .unlink(provider):
                busyId = provider.id
                try await deps.usersApi.unlinkSignInMethod(accountId: accountId, providerId: provider.id)
            case let .removePasskey(passkey):
                busyId = passkey.id
                try await deps.usersApi.deletePasskey(accountId: accountId, id: passkey.id)
            }
        } catch {
            // A PRECONDITION_FAILED refusal carries the server's last-way-in
            // sentence — the toast's description (FEED-79).
            let title: String = switch target {
            case .unlink: FeedbackStates.Errors.title(verb: "remove", thing: "the sign-in method")
            case .removePasskey: FeedbackStates.Errors.title(verb: "remove", thing: "the passkey")
            }
            toaster.error(title, description: error.trpcUserMessage)
        }
        busyId = nil
        await load()
    }

    /// EXP-1596 Add passkey (web `PasskeysSection` handleAdd, Android
    /// `addPasskey`): the creation options for this account's session, the
    /// platform authenticator signs them, the attestation goes back, the list
    /// reloads. A dismissed system sheet is silent; every other failure is the
    /// failed-action toast (FEED-79), the server's message (a stale session)
    /// as its description. Options for any rp but this instance are refused,
    /// the login path's guard (`relyingPartyMatchesInstance`).
    private func addPasskey() async {
        guard !addingPasskey,
              let account = deps.auth.accounts.first(where: { $0.id == accountId }),
              let token = account.token else { return }
        let instanceUrl = account.instanceUrl
        addingPasskey = true
        defer { addingPasskey = false }
        let failedTitle = FeedbackStates.Errors.title(verb: "add", thing: "the passkey")

        let options: PasskeyRegistrationOptions
        switch await deps.authApi.passkeyRegistrationOptions(
            instanceUrl: instanceUrl, token: token, name: Self.passkeyName
        ) {
        case let .success(served): options = served
        case let .failure(message):
            toaster.error(failedTitle, description: message.isEmpty ? nil : message)
            return
        }
        guard PasskeyWire.relyingPartyMatchesInstance(rpId: options.rpId, instanceUrl: instanceUrl) else {
            toaster.error(failedTitle)
            return
        }

        let attestation: PasskeyAttestation
        switch await passkeyCeremony.register(options) {
        case let .created(created): attestation = created
        case .cancelled: return
        case .failed:
            // The system error's text is framework jargon; Android's toast
            // carries no description here either.
            toaster.error(failedTitle)
            return
        }

        if let message = await deps.authApi.verifyPasskeyRegistration(
            instanceUrl: instanceUrl,
            token: token,
            cookieHeader: options.cookieHeader,
            attestation: attestation,
            name: Self.passkeyName
        ) {
            toaster.error(failedTitle, description: message.isEmpty ? nil : message)
            return
        }
        await load()
    }

    /// Mint a ticket, then run the link-mode handoff in the in-app browser.
    private func link(_ provider: SignInProvider) async {
        guard pendingLink == nil,
              let instanceUrl = deps.auth.accounts.first(where: { $0.id == accountId })?.instanceUrl
        else { return }
        pendingLink = provider.id
        let ticket: SignInLinkTicket
        do {
            ticket = try await deps.usersApi.mintSignInLinkTicket(accountId: accountId, provider: provider.id)
        } catch {
            pendingLink = nil
            toaster.error(
                FeedbackStates.Errors.title(verb: "link", thing: provider.name),
                description: error.trpcUserMessage
            )
            return
        }
        let pkce = Pkce.generate()
        guard let url = AuthApi.linkStartUrl(
            instanceUrl: instanceUrl, ticket: ticket.ticket, provider: provider.id, codeChallenge: pkce.challenge
        ) else {
            pendingLink = nil
            toaster.error(
                FeedbackStates.Errors.title(verb: "link", thing: provider.name),
                description: OAuthReturn.linkErrorMessage("")
            )
            return
        }
        linkSession.start(url: url, pkce: pkce) { result in
            pendingLink = nil
            // nil = the sheet was closed: a cancel is never an error.
            guard let result else { return }
            switch result {
            case .linked:
                toaster.success("\(provider.name) is linked.")
                Task { await load() }
            case let .error(reason):
                toaster.error(
                    FeedbackStates.Errors.title(verb: "link", thing: provider.name),
                    description: OAuthReturn.linkErrorMessage(reason)
                )
            case .code, .none:
                // A link hop never returns a code; refetch in case it landed.
                Task { await load() }
            }
        }
    }
}

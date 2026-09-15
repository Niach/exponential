import AuthenticationServices
import Combine
import ExpUI
import ExpCore
import SwiftUI

// The Add-repository picker (web github-repo-picker.tsx; FEED-42 canonical
// spec B, copy in ExpCore `GithubCopy`): lists the repos the user's GitHub
// grants cover and ADDS the tapped one (or a successful Add-by-name) through
// the host's `onAdd`, performed INSIDE the sheet — a failure keeps the sheet
// open with the error inline (server message, plan limit, or the grant-model
// FORBIDDEN arm with its Reconnect GitHub pill); success dismisses. Handles
// not-configured / not-installed (in-app connect + auto re-query) / installed
// (independent suspended + re-auth banners, searchable rows, the dashed
// footer).
//
// EXP-8: the connect URL opens in an ASWebAuthenticationSession (mobile-width
// page, in-app) instead of kicking out to system Safari. The server's
// post-install page fires `exponential://github-connected`, which auto-dismisses
// the session; either way the completion re-queries with `refresh: true` so the
// newly connected repos appear without any manual step.
struct GithubRepoPicker: View {
    let accountId: String
    /// Scopes the repo query + connect hop to this team's linked GitHub
    /// accounts (per-team installation claiming).
    let teamId: String
    let integrationsApi: IntegrationsApi
    /// Adds the picked repo. Runs inside the sheet: a throw keeps it open with
    /// the error inline, a return dismisses it.
    var onAdd: @MainActor (GithubPickerRepo) async throws -> Void

    @Environment(\.dismiss) private var dismiss
    @Environment(\.scenePhase) private var scenePhase
    @State private var result: GithubReposResult?
    @State private var loading = true
    @State private var query = ""
    @State private var error: String?
    // A failed connect hop's message (EXP-390) — separate from `error`, which
    // belongs to the repos query and has its own lifecycle.
    @State private var connectError: String?
    @State private var installSession = InstallWebAuthSession()
    // FEED-30: the footer's "Add by name" escape hatch.
    @State private var lookupName = ""
    @State private var lookupBusy = false
    @State private var lookupError: String?
    // FEED-42: the in-sheet add — the row in flight and its failure.
    @State private var adding: String?
    @State private var addError: String?
    @State private var addForbidden = false

    // Bottom-sheet presentation (EXP-390, Android parity): the shared glass
    // sheet chrome, content-fitted (EXP-577).
    var body: some View {
        GlassSheetChrome(title: GithubCopy.addRepository) {
            VStack(alignment: .leading, spacing: 16) {
                content
                if let connectError {
                    Text(connectError).font(.caption).foregroundStyle(.red)
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 16)
        }
        .task { await load() }
        .onChange(of: scenePhase) { _, phase in
            // Self-heal after any trip through another app/browser; bypass
            // the server cache so a just-granted repo shows up.
            if phase == .active { Task { await load(refresh: true) } }
        }
        // The app-level deep-link path for `exponential://github-connected` —
        // covers a connect that finishes outside the in-app auth session. An
        // error slug means the connect FAILED: surface it instead of refreshing.
        .onReceive(NotificationCenter.default.publisher(for: .githubConnected)) { notification in
            if let slug = notification.userInfo?["error"] as? String {
                connectError = GithubConnect.errorMessage(for: slug)
            } else {
                connectError = nil
                Task { await load(refresh: true) }
            }
        }
    }

    @ViewBuilder private var content: some View {
        if loading && result == nil {
            HStack(spacing: 10) {
                ProgressView().controlSize(.small).tint(.white)
                Text(GithubCopy.loadingRepos)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Spacer(minLength: 0)
            }
            .githubBox()
        } else if let data = result, data.configured {
            if data.installed {
                installedList(data)
            } else {
                notInstalled(data)
            }
        } else {
            // Not configured, or the fetch failed with nothing loaded.
            VStack(alignment: .leading, spacing: 8) {
                githubNotice(GithubCopy.pickerNotConfigured)
                if let error {
                    Text(error).font(.caption).foregroundStyle(.red.opacity(0.8))
                }
            }
        }
    }

    /// A boxed [GitHub] + copy notice (loading/not-configured/not-installed).
    private func githubNotice(_ text: String) -> some View {
        HStack(alignment: .top, spacing: 8) {
            AppIcon(AppIcons.uiGithub, size: AppIcon.Size.small)
                .padding(.top, 1)
            Text(text)
                .font(.subheadline)
            Spacer(minLength: 0)
        }
        .foregroundStyle(.white.opacity(TextOpacity.secondary))
        .githubBox(dashed: true)
    }

    @ViewBuilder private func notInstalled(_ data: GithubReposResult) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            githubNotice(GithubCopy.pickerNotInstalled)
            // EXP-687: the app's ONE primary button, never a system tint.
            Button {
                openConnect(data)
            } label: {
                GlassSubmitLabel(GithubCopy.connectGithub) {
                    AppIcon(AppIcons.uiGithub, size: AppIcon.Size.medium)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            GlassPill(GithubCopy.iveConnected, icon: AppIcons.uiRefresh, mode: .action {
                Task { await load(refresh: true) }
            })
        }
    }

    // Grant model: the list shows exactly the repos the user's last OAuth
    // connect proved access to. The suspended and re-auth banners are
    // INDEPENDENT (both may show); the footer renders in every installed state.
    @ViewBuilder private func installedList(_ data: GithubReposResult) -> some View {
        let trimmed = query.trimmingCharacters(in: .whitespaces)
        let repos = data.repos.filter {
            trimmed.isEmpty || $0.fullName.localizedCaseInsensitiveContains(trimmed)
        }
        let suspended = data.installations.filter { $0.isSuspended }
        let needsReauth = data.installations.contains { $0.needsReauth && !$0.isSuspended }
        let empty = data.repos.isEmpty
        VStack(alignment: .leading, spacing: 8) {
            if !suspended.isEmpty {
                suspendedBanner(suspended)
            }
            if needsReauth {
                reauthBanner(data, empty: empty)
            }

            addErrorBox(data)

            if !empty {
                GlassSheetSearchField(placeholder: GithubCopy.searchPlaceholder, text: $query)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()

                if repos.isEmpty {
                    Text(GithubCopy.noneFound)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .padding(.vertical, 8)
                }

                ForEach(repos) { repo in
                    repoRow(repo)
                }
            } else if !needsReauth, suspended.isEmpty {
                Text(GithubCopy.noneGranted)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .frame(maxWidth: .infinity)
                    .githubBox()
            }

            footer(data)
        }
    }

    private func repoRow(_ repo: GithubPickerRepo) -> some View {
        Button {
            Task { await add(repo) }
        } label: {
            HStack(spacing: 10) {
                AppIcon(AppIcons.uiGithub, size: AppIcon.Size.small)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text(repo.fullName)
                    .font(.subheadline.monospaced())
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Spacer()
                if adding == repo.fullName {
                    ProgressView().controlSize(.small).tint(.white)
                } else if repo.`private` {
                    AppIcon(AppIcons.uiPrivate, size: 11)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(adding != nil)
    }

    // GitHub-side App suspension (REV2-29): the installation lists no repos
    // and mints no tokens until it's unsuspended on GitHub. No button.
    private func suspendedBanner(_ suspended: [GithubInstallation]) -> some View {
        HStack(alignment: .top, spacing: 8) {
            AppIcon(AppIcons.uiGithub, size: AppIcon.Size.small)
                .padding(.top, 1)
            Text(GithubCopy.pickerSuspended(suspended.map { $0.accountLogin }))
                .font(.caption)
            Spacer(minLength: 0)
        }
        .foregroundStyle(DesignTokens.Palette.destructive)
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .background(DesignTokens.Palette.destructive.opacity(0.1), in: RoundedRectangle(cornerRadius: GlassTokens.rowRadius))
        .overlay(
            RoundedRectangle(cornerRadius: GlassTokens.rowRadius)
                .stroke(DesignTokens.Palette.destructive.opacity(0.5), lineWidth: GlassTokens.hairline)
        )
    }

    // A linked account whose grants were never captured — its repos are
    // missing until the user reconnects.
    private func reauthBanner(_ data: GithubReposResult, empty: Bool) -> some View {
        let accounts = data.installations
            .filter { $0.needsReauth && !$0.isSuspended }
            .compactMap { $0.accountLogin }
        return VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top, spacing: 8) {
                AppIcon(AppIcons.uiWarning, size: AppIcon.Size.small)
                    .foregroundStyle(DesignTokens.Semantic.yellow)
                    .padding(.top, 1)
                Text(GithubCopy.reauthBanner(accounts: accounts, emptyList: empty))
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Spacer(minLength: 0)
            }
            GlassPill(GithubCopy.reconnectGithub, icon: AppIcons.uiRefresh, mode: .action { openConnect(data) })
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .background(DesignTokens.Semantic.yellow.opacity(0.05), in: RoundedRectangle(cornerRadius: GlassTokens.rowRadius))
        .overlay(
            RoundedRectangle(cornerRadius: GlassTokens.rowRadius)
                .stroke(DesignTokens.Semantic.yellow.opacity(0.4), lineWidth: GlassTokens.hairline)
        )
    }

    // FEED-42: a failed add, inline — the (neutral) server message, or the
    // grant-model FORBIDDEN arm with its reconnect. No web upgrade pointer on
    // a store app (EXP-216).
    @ViewBuilder private func addErrorBox(_ data: GithubReposResult) -> some View {
        if let addError {
            VStack(alignment: .leading, spacing: 8) {
                Text(addError)
                    .font(.caption)
                    .foregroundStyle(.red.opacity(0.8))
                if addForbidden, (data.connectUrl ?? data.installUrl) != nil {
                    GlassPill(GithubCopy.reconnectGithub, icon: AppIcons.uiRefresh, mode: .action { openConnect(data) })
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
        }
    }

    // FEED-30: the list explains itself — the sentence plus a Configure link
    // per account, the cap note, the two fixes, and the by-name escape hatch.
    @ViewBuilder private func footer(_ data: GithubReposResult) -> some View {
        let manageLinks: [(label: String, url: URL)] = data.installations.compactMap { inst in
            guard !inst.manageUrl.isEmpty, let url = URL(string: inst.manageUrl) else { return nil }
            return (GithubCopy.installationLabel(login: inst.accountLogin, installationId: inst.installationId), url)
        }
        VStack(alignment: .leading, spacing: 8) {
            Text(GithubCopy.footerSentence)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            if !manageLinks.isEmpty {
                FlowLayout(spacing: 0) {
                    ForEach(Array(manageLinks.enumerated()), id: \.offset) { index, link in
                        HStack(spacing: 0) {
                            Link(destination: link.url) {
                                HStack(spacing: 2) {
                                    Text(link.label)
                                    AppIcon(AppIcons.uiExternalLink, size: 11)
                                }
                                .foregroundStyle(.white)
                            }
                            if index < manageLinks.count - 1 {
                                Text(", ")
                                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            }
                        }
                        .font(.caption)
                    }
                }
            }
            if data.hasMore {
                Text(GithubCopy.capNote)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
            FlowLayout(spacing: 8) {
                GlassPill(
                    GithubCopy.refresh,
                    icon: AppIcons.uiRefresh,
                    mode: .action { refreshAccess(data) },
                    enabled: !loading
                )
                if data.installUrl != nil {
                    GlassPill(GithubCopy.installOnAnotherAccount, icon: AppIcons.uiAdd, mode: .action {
                        connectError = nil
                        openInBrowser(data.installUrl)
                    })
                }
            }
            HStack(spacing: 8) {
                GlassTextField(GithubCopy.lookupPlaceholder, text: $lookupName, horizontalPadding: 12, verticalPadding: 8) {
                    EmptyView()
                } trailing: {
                    EmptyView()
                }
                .font(.subheadline.monospaced())
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
                .accessibilityLabel(GithubCopy.lookupAccessibility)
                .onSubmit { Task { await lookup() } }
                .onChange(of: lookupName) { _, _ in
                    if lookupError != nil { lookupError = nil }
                }
                GlassPill(
                    GithubCopy.lookUp,
                    mode: .action { Task { await lookup() } },
                    enabled: RepoFullName.isValid(lookupName.trimmingCharacters(in: .whitespaces)) && !lookupBusy && adding == nil
                ) {
                    if lookupBusy {
                        ProgressView().controlSize(.mini).tint(.white)
                    }
                }
            }
            if let lookupError {
                Text(lookupError)
                    .font(.caption)
                    .foregroundStyle(.red.opacity(0.8))
            }
        }
        .githubBox(dashed: true)
    }

    // FEED-30: on OAuth instances the list IS the viewer's grant snapshot,
    // which only the OAuth re-auth rewrites — so "Refresh" runs the re-auth
    // hop there (its completion re-lists) and a plain forced re-list where
    // there is no OAuth.
    private func refreshAccess(_ data: GithubReposResult) {
        if let connectUrl = data.connectUrl {
            connectError = nil
            openInBrowser(connectUrl)
            return
        }
        Task { await load(refresh: true) }
    }

    // FEED-30: integrations.github.lookupRepo for the typed `owner/name`; a
    // hit is ADDED exactly like a row tap, a miss shows the server's message
    // inline.
    private func lookup() async {
        let fullName = lookupName.trimmingCharacters(in: .whitespaces)
        guard RepoFullName.isValid(fullName), !lookupBusy, adding == nil else { return }
        lookupBusy = true
        lookupError = nil
        do {
            let repo = try await integrationsApi.lookupRepo(accountId: accountId, teamId: teamId, fullName: fullName)
            lookupBusy = false
            lookupName = ""
            await add(repo)
        } catch {
            lookupError = error.trpcUserMessage
            lookupBusy = false
        }
    }

    // FEED-42: tap adds, inside the sheet. Success dismisses; a failure stays
    // open with the error inline. FORBIDDEN is checked FIRST so a stale GitHub
    // grant is never misread as a plan limit (desktop parity).
    private func add(_ repo: GithubPickerRepo) async {
        guard adding == nil else { return }
        adding = repo.fullName
        addError = nil
        addForbidden = false
        do {
            try await onAdd(repo)
            adding = nil
            dismiss()
        } catch {
            adding = nil
            let message = error.trpcUserMessage
            if GithubCopy.isGrantForbidden(code: error.trpcErrorCode, message: message) {
                addForbidden = true
                addError = GithubCopy.addForbidden
            } else {
                addError = message
            }
        }
    }

    // Connect action: claim a GitHub account for this team. Prefer the
    // mobile-friendly OAuth `connectUrl` and fall back to the install page.
    private func openConnect(_ data: GithubReposResult) {
        connectError = nil
        openInBrowser(data.connectUrl ?? data.installUrl)
    }

    // Opened in an ASWebAuthenticationSession: mobile-width rendering, and the
    // server's `exponential://github-connected` redirect dismisses it and
    // hands control back — carrying the error slug when the connect failed
    // (EXP-390).
    private func openInBrowser(_ urlString: String?) {
        guard let urlString, let url = URL(string: urlString) else { return }
        installSession.start(url: url) { errorSlug in
            connectError = errorSlug.map { GithubConnect.errorMessage(for: $0) }
            Task { await load(refresh: true) }
        }
    }

    private func load(refresh: Bool = false) async {
        loading = true
        do {
            let r = try await integrationsApi.githubRepos(accountId: accountId, teamId: teamId, refresh: refresh)
            result = r
            error = nil
            loading = false
        } catch {
            self.error = error.trpcUserMessage
            loading = false
        }
    }
}

private extension View {
    /// The picker's bordered box — dashed for notices and the footer, solid
    /// for the loading/empty states (web `rounded-md border[-dashed]`).
    func githubBox(dashed: Bool = false) -> some View {
        self
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .overlay(
                RoundedRectangle(cornerRadius: GlassTokens.rowRadius)
                    .stroke(
                        GlassTokens.strokeCard,
                        style: StrokeStyle(lineWidth: 1, dash: dashed ? [4, 3] : [])
                    )
            )
    }
}

/// Presents the GitHub App install page in an ASWebAuthenticationSession so it
/// (a) renders as a phone-sized in-app page instead of desktop Safari and
/// (b) auto-dismisses when the server's post-install page fires the
/// `exponential://github-connected` deep link (callback scheme `exponential`).
/// The completion fires on callback AND on manual dismissal — the install may
/// have landed either way, so callers should re-query regardless. It receives
/// the callback's `?error=` slug (EXP-390) — nil on success AND on manual
/// dismissal, so nil means "no error to show", never "connected".
@MainActor
final class InstallWebAuthSession: NSObject, ASWebAuthenticationPresentationContextProviding {
    private var session: ASWebAuthenticationSession?

    func start(url: URL, onFinished: @escaping @MainActor (String?) -> Void) {
        session?.cancel()
        let session = ASWebAuthenticationSession(
            url: url,
            callbackURLScheme: "exponential"
        ) { [weak self] callbackURL, _ in
            let errorSlug = callbackURL.flatMap { GithubConnect.errorSlug(from: $0) }
            Task { @MainActor in
                self?.session = nil
                onFinished(errorSlug)
            }
        }
        session.presentationContextProvider = self
        // The user is signing in to GitHub — share the persistent cookie jar so
        // an existing GitHub session is reused instead of forcing a fresh login.
        session.prefersEphemeralWebBrowserSession = false
        self.session = session
        session.start()
    }

    nonisolated func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        MainActor.assumeIsolated {
            UIApplication.shared.connectedScenes
                .compactMap { $0 as? UIWindowScene }
                .flatMap(\.windows)
                .first { $0.isKeyWindow } ?? ASPresentationAnchor()
        }
    }
}

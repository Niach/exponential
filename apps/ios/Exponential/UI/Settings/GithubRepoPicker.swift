import AuthenticationServices
import Combine
import ExpUI
import ExpCore
import SwiftUI

// Installed-repo picker (web github-repo-picker.tsx): lists the repos the user's
// GitHub App is installed on and returns the chosen one to the caller. v4: it no
// longer links a repo to a board directly — instead it feeds the create-board
// inline-connect path (`repository: { fullName }`). Handles not-configured /
// not-installed (in-app install flow + auto re-query) / installed (searchable
// list). The link/upsert happens server-side in `boards.create`.
//
// EXP-8: the install URL opens in an ASWebAuthenticationSession (mobile-width
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
    /// Called with the picked repo; the sheet dismisses itself afterwards.
    var onPick: (GithubPickerRepo) -> Void

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
    // FEED-30: the footer's "Add by name" escape hatch — `owner/name`, looked
    // up through integrations.github.lookupRepo (the connect path's own
    // checks) and, on a hit, picked exactly like a row.
    @State private var lookupName = ""
    @State private var lookupBusy = false
    @State private var lookupError: String?

    // Bottom-sheet presentation (EXP-390, Android parity): the shared glass
    // sheet chrome, content-fitted — a short state (connect prompt, empty
    // list) sizes to fit instead of a half-screen of empty glass (EXP-577).
    var body: some View {
        GlassSheetChrome(title: "Add repository") {
            VStack(alignment: .leading, spacing: 16) {
                content
                if let connectError {
                    Text(connectError).font(.caption).foregroundStyle(.red)
                }
                if let error {
                    Text(error).font(.caption).foregroundStyle(.red)
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 16)
        }
        .task { await load() }
        .onChange(of: scenePhase) { _, phase in
            // Self-heal after any trip through another app/browser (e.g. an
            // install finished externally); bypass the server cache so a
            // just-granted repo shows up.
            if phase == .active { Task { await load(refresh: true) } }
        }
        // The app-level deep-link path for `exponential://github-connected` — covers
        // an install that finishes outside the in-app auth session. An error
        // slug means the connect FAILED: surface it instead of refreshing.
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
                Text("Loading your GitHub repositories…")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
            .padding(.vertical, 24)
        } else if let data = result, data.configured {
            if data.installed {
                installedList(data)
            } else {
                notInstalled(data)
            }
        } else {
            Text("GitHub isn't configured on this server, so repositories can't be connected.")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        }
    }

    @ViewBuilder private func notInstalled(_ data: GithubReposResult) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Connect the Exponential GitHub App to pick a repository. You'll come right back here.")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            // EXP-687: the app's ONE primary button, never a system tint.
            GlassSubmitButton("Connect GitHub") {
                openConnect(data)
            }
            // Android parity (EXP-577): the escape hatch is a neutral white
            // outline with the refresh glyph, never the system-blue tint.
            Button {
                Task { await load(refresh: true) }
            } label: {
                HStack(spacing: 6) {
                    AppIcon(AppIcons.uiRefresh, size: AppIcon.Size.medium)
                    Text("I've connected")
                }
                .foregroundStyle(.white)
                .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .tint(.white)
        }
    }

    // Grant model: the list shows exactly the repos the user's last OAuth
    // connect proved access to — never the installation-wide selection. So a
    // repo created or shared since that connect only appears after re-running
    // the connect hop (`openConnect`), and a team linked before grants
    // existed (`needsReauth`) yields zero repos until someone reconnects.
    @ViewBuilder private func installedList(_ data: GithubReposResult) -> some View {
        let repos = data.repos.filter {
            query.isEmpty || $0.fullName.localizedCaseInsensitiveContains(query.trimmingCharacters(in: .whitespaces))
        }
        VStack(alignment: .leading, spacing: 8) {
            if data.repos.isEmpty {
                emptyState(data)
                // FEED-30: the footer explains the empty list too.
                footer(data)
            } else {
                if data.installations.contains(where: { $0.isSuspended }) {
                    suspendedNotice(data)
                }
                if data.installations.contains(where: { $0.needsReauth && !$0.isSuspended }) {
                    reconnectNotice(data)
                }

                GlassSheetSearchField(placeholder: "Search repositories…", text: $query)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()

                if repos.isEmpty {
                    Text("No repositories found.")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .padding(.vertical, 8)
                }

                ForEach(repos) { repo in
                    Button {
                        onPick(repo)
                        dismiss()
                    } label: {
                        HStack(spacing: 10) {
                            AppIcon(AppIcons.uiRepository, size: AppIcon.Size.small)
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            Text(repo.fullName)
                                .font(.subheadline.monospaced())
                                .foregroundStyle(.white)
                                .lineLimit(1)
                            Spacer()
                            if repo.`private` {
                                AppIcon(AppIcons.uiPrivate, size: 11)
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            }
                        }
                        .padding(.horizontal, 12)
                        .padding(.vertical, 10)
                        .glassRow()
                    }
                    .buttonStyle(.plain)
                }

                footer(data)
            }
        }
    }

    // FEED-30: the list explains itself. A missing repo is (almost) always an
    // installation whose repo selection doesn't include it, or a repo on an
    // account that isn't installed at all — say so, link the exact GitHub
    // page per account, offer the two fixes, and the by-name escape hatch
    // (its error names the real reason). Rendered in EVERY installed state.
    @ViewBuilder private func footer(_ data: GithubReposResult) -> some View {
        let manageLinks: [(label: String, url: URL)] = data.installations.compactMap { inst in
            guard !inst.manageUrl.isEmpty, let url = URL(string: inst.manageUrl) else { return nil }
            return (inst.accountLogin ?? "installation", url)
        }
        VStack(alignment: .leading, spacing: 8) {
            Text("Only repositories your GitHub installation grants appear here. Missing one? Grant it on GitHub, then refresh.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            if !manageLinks.isEmpty {
                FlowLayout(spacing: 6) {
                    ForEach(Array(manageLinks.enumerated()), id: \.offset) { _, link in
                        Link(destination: link.url) {
                            GlassPill(link.label) {
                                AppIcon(AppIcons.uiExternalLink, size: GlassPillTokens.glyphSm)
                            }
                            .contentShape(Capsule())
                        }
                    }
                }
            }
            if data.hasMore {
                Text("Showing the first 500 repositories per account — use the field below for the rest.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
            FlowLayout(spacing: 8) {
                GlassPill("Refresh", icon: AppIcons.uiRefresh, mode: .action { refreshAccess(data) })
                if data.installUrl != nil {
                    GlassPill("Install on another account", icon: AppIcons.uiAdd, mode: .action {
                        connectError = nil
                        openInBrowser(data.installUrl)
                    })
                }
            }
            HStack(spacing: 8) {
                GlassTextField("owner/name", text: $lookupName, horizontalPadding: 12, verticalPadding: 10) {
                    EmptyView()
                } trailing: {
                    EmptyView()
                }
                .font(.subheadline.monospaced())
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
                .onSubmit { Task { await lookup() } }
                .onChange(of: lookupName) { _, _ in
                    if lookupError != nil { lookupError = nil }
                }
                GlassPill(
                    "Look up",
                    mode: .action { Task { await lookup() } },
                    enabled: RepoFullName.isValid(lookupName.trimmingCharacters(in: .whitespaces)) && !lookupBusy
                )
            }
            if let lookupError {
                Text(lookupError)
                    .font(.caption)
                    .foregroundStyle(.red.opacity(0.8))
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .glassRow()
    }

    // FEED-30: on OAuth instances the list IS the viewer's grant snapshot,
    // which only the OAuth re-auth (or the installation_repositories webhook)
    // rewrites — a bare cache refresh can't surface a repo granted since. So
    // "Refresh" runs the re-auth hop there (its completion re-lists) and a
    // plain forced re-list where there is no OAuth.
    private func refreshAccess(_ data: GithubReposResult) {
        if let connectUrl = data.connectUrl {
            connectError = nil
            openInBrowser(connectUrl)
            return
        }
        Task { await load(refresh: true) }
    }

    // FEED-30: integrations.github.lookupRepo for the typed `owner/name`; a
    // hit is picked exactly like a row, a miss shows the server's message
    // inline (grant it, connect that account, or reconnect).
    private func lookup() async {
        let fullName = lookupName.trimmingCharacters(in: .whitespaces)
        guard RepoFullName.isValid(fullName), !lookupBusy else { return }
        await MainActor.run {
            lookupBusy = true
            lookupError = nil
        }
        do {
            let repo = try await integrationsApi.lookupRepo(accountId: accountId, teamId: teamId, fullName: fullName)
            await MainActor.run {
                lookupName = ""
                lookupBusy = false
                onPick(repo)
                dismiss()
            }
        } catch {
            await MainActor.run {
                lookupError = error.trpcUserMessage
                lookupBusy = false
            }
        }
    }

    // Installed but zero repos — three DISTINCT states (EXP-365, web parity):
    // a suspended installation needs an UNSUSPEND on GitHub (a reconnect
    // cannot fix it — never nudge the wrong fix), a needs-reauth one needs the
    // OAuth reconnect, and an account that genuinely has no reachable repos
    // needs neither.
    @ViewBuilder private func emptyState(_ data: GithubReposResult) -> some View {
        let suspended = data.installations.filter { $0.isSuspended }
        let needsReauth = data.installations.contains { $0.needsReauth && !$0.isSuspended }
        if !suspended.isEmpty {
            suspendedNotice(data)
        } else if needsReauth {
            VStack(alignment: .leading, spacing: 12) {
                Text("Reconnect GitHub to load the repositories you can access\(reauthAccountSuffix(data)).")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                GlassSubmitButton("Reconnect GitHub") {
                    openConnect(data)
                }
            }
        } else {
            Text("None of your connected GitHub accounts grants a repository yet.")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        }
    }

    // GitHub-side App suspension (REV2-29): the installation lists no repos
    // and mints no tokens until it's unsuspended on GitHub.
    @ViewBuilder private func suspendedNotice(_ data: GithubReposResult) -> some View {
        let names = data.installations
            .filter { $0.isSuspended }
            .map { $0.accountLogin ?? "a connected account" }
            .joined(separator: ", ")
        Text("GitHub suspended the Exponential app for \(names). Its repositories can't be connected until you unsuspend it on GitHub.")
            .font(.caption)
            .foregroundStyle(.red.opacity(0.8))
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
    }

    // " from a, b" when the stale accounts are known — names make the fix
    // actionable when several accounts are linked.
    private func reauthAccountSuffix(_ data: GithubReposResult, preposition: String = "from") -> String {
        let names = data.installations
            .filter { $0.needsReauth && !$0.isSuspended }
            .compactMap { $0.accountLogin }
        return names.isEmpty ? "" : " \(preposition) \(names.joined(separator: ", "))"
    }

    // A linked account whose grants were never captured — its repos are missing
    // from the (non-empty) list until the user reconnects.
    @ViewBuilder private func reconnectNotice(_ data: GithubReposResult) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Reconnect GitHub\(reauthAccountSuffix(data, preposition: "for")) to refresh. Repos created or shared with you since your last connect won't appear until you do.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Button {
                openConnect(data)
            } label: {
                HStack(spacing: 6) {
                    AppIcon(AppIcons.uiRefresh, size: AppIcon.Size.medium)
                    Text("Reconnect GitHub")
                }
                .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .glassRow()
    }

    // Connect action: claim a GitHub account for this team. Prefer the
    // mobile-friendly OAuth `connectUrl` (single consent screen) and fall back
    // to the GitHub App install page when it's absent.
    private func openConnect(_ data: GithubReposResult) {
        connectError = nil
        openInBrowser(data.connectUrl ?? data.installUrl)
    }

    // Web parity (github-repo-picker.tsx): the old `/account/integrations`
    // fallback was removed in v5 (repo management lives in team settings →
    // Repositories). Opened in an ASWebAuthenticationSession: mobile-width
    // rendering, and the server's `exponential://github-connected` redirect
    // dismisses it and hands control back — carrying the error slug when the
    // connect failed (EXP-390).
    private func openInBrowser(_ urlString: String?) {
        guard let urlString, let url = URL(string: urlString) else { return }
        installSession.start(url: url) { errorSlug in
            connectError = errorSlug.map { GithubConnect.errorMessage(for: $0) }
            Task { await load(refresh: true) }
        }
    }

    private func load(refresh: Bool = false) async {
        await MainActor.run { loading = true }
        do {
            let r = try await integrationsApi.githubRepos(accountId: accountId, teamId: teamId, refresh: refresh)
            await MainActor.run {
                result = r
                error = nil
                loading = false
            }
        } catch {
            await MainActor.run { self.error = error.trpcUserMessage; loading = false }
        }
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

import Combine
import ExpUI
import ExpCore
import SwiftUI

// The Add-repository picker (web github-repo-picker.tsx; SLOP-7/SLOP-26
// canonical spec, copy in ExpCore `GithubCopy`): lists the repos the VIEWER
// can push to, LIVE off GitHub (`integrations.github.repos`), and ADDS the
// tapped one (or a successful Add-by-name) through the host's `onAdd`,
// performed INSIDE the sheet — a failure keeps the sheet open with the error
// inline; success dismisses. When a prerequisite is missing it says which one
// and offers the ONE fix — Connect GitHub (the link-ticket hop,
// `GithubConnectSession`), Reconnect GitHub (the same hop), or Install the
// app (`installUrl` in the system browser) — plus "I’ve done that" to
// re-list. Back from any hop (the `github-connected` deep link, the auth
// sheet's callback, or just the scene turning active) re-queries with
// `refresh: true` so the newly connected repos appear without a manual step.
struct GithubRepoPicker: View {
    let accountId: String
    /// The team the pick is added to (every integrations query is team-scoped).
    let teamId: String
    let integrationsApi: IntegrationsApi
    /// Adds the picked repo. Runs inside the sheet: a throw keeps it open with
    /// the error inline, a return dismisses it.
    var onAdd: @MainActor (GithubPickerRepo) async throws -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.openURL) private var openURL
    @State private var result: GithubReposResult?
    @State private var loading = true
    @State private var query = ""
    @State private var error: String?
    // A failed connect hop's message — separate from `error`, which belongs
    // to the repos query and has its own lifecycle.
    @State private var connectError: String?
    @State private var connectSession = GithubConnectSession()
    @State private var connecting = false
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
        GlassSheetChrome(title: GithubCopy.pickerTitle) {
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
        // the guided page's "Return to the app" after an install in the
        // system browser. An error slug means the hop FAILED: say so.
        .onReceive(NotificationCenter.default.publisher(for: .githubConnected)) { notification in
            if notification.userInfo?["error"] != nil {
                connectError = GithubCopy.linkFailed
            } else {
                connectError = nil
                Task { await load(refresh: true) }
            }
        }
        .onDisappear { connectSession.cancel() }
    }

    @ViewBuilder private var content: some View {
        if loading && result == nil {
            HStack(spacing: 10) {
                ProgressView().controlSize(.small).tint(.white)
                Text(GithubCopy.pickerLoading)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Spacer(minLength: 0)
            }
            .githubBox()
        } else if let data = result, data.configured {
            switch data.prerequisite {
            case .notLinked:
                prerequisite(text: GithubCopy.pickerNotLinked, fix: GithubCopy.connectGithub) { connect(data) }
            case .expired:
                prerequisite(text: GithubCopy.pickerReconnectBanner, fix: GithubCopy.reconnectGithub) { connect(data) }
            case .notInstalled:
                prerequisite(text: GithubCopy.pickerNotInstalled, fix: GithubCopy.installApp, enabled: data.installUrl != nil) {
                    openInstall(data.installUrl)
                }
            case .notConfigured, nil:
                installedList(data)
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

    /// A boxed [GitHub] + copy notice (loading/not-configured/prerequisite).
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

    // A prerequisite is missing: one sentence naming it, ONE button fixing
    // it, and the "I’ve done that" re-list (web `repo-picker-prerequisite`).
    @ViewBuilder private func prerequisite(
        text: String, fix: String, enabled: Bool = true, action: @escaping () -> Void
    ) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            githubNotice(text)
            // EXP-687: the app's ONE primary button, never a system tint.
            Button(action: action) {
                GlassSubmitLabel(fix, enabled: enabled && !connecting, loading: connecting) {
                    AppIcon(AppIcons.uiGithub, size: AppIcon.Size.medium)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(!enabled || connecting)
            .accessibilityIdentifier("github-picker-fix")
            GlassPill(GithubCopy.pickerConnectedCheck, icon: AppIcons.uiRefresh, mode: .action {
                Task { await load(refresh: true) }
            })
        }
    }

    // Installed → the live, searchable list of push-able repositories. A
    // suspended installation lists nothing — say why (REV2-29); the footer
    // renders in every installed state.
    @ViewBuilder private func installedList(_ data: GithubReposResult) -> some View {
        let trimmed = query.trimmingCharacters(in: .whitespaces)
        let repos = data.repos.filter {
            trimmed.isEmpty || $0.fullName.localizedCaseInsensitiveContains(trimmed)
        }
        let suspended = data.installations.filter { $0.isSuspended }
        let empty = data.repos.isEmpty
        VStack(alignment: .leading, spacing: 8) {
            if !suspended.isEmpty {
                suspendedBanner(suspended)
            }

            addErrorBox(data)

            if !empty {
                GlassSheetSearchField(placeholder: GithubCopy.searchPlaceholder, text: $query)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()

                if repos.isEmpty {
                    Text(GithubCopy.noMatch)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .padding(.vertical, 8)
                }

                ForEach(repos) { repo in
                    repoRow(repo)
                }
            } else if suspended.isEmpty {
                Text(GithubCopy.nonePushable)
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
            AppIcon(AppIcons.uiWarning, size: AppIcon.Size.small)
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

    // FEED-42: a failed add, inline — the (neutral) server message, or the
    // FORBIDDEN arm with its Reconnect GitHub. No web upgrade pointer on a
    // store app (EXP-216).
    @ViewBuilder private func addErrorBox(_ data: GithubReposResult) -> some View {
        if let addError {
            VStack(alignment: .leading, spacing: 8) {
                Text(addError)
                    .font(.caption)
                    .foregroundStyle(.red.opacity(0.8))
                if addForbidden {
                    GlassPill(GithubCopy.reconnectGithub, icon: AppIcons.uiRefresh, mode: .action { connect(data) }, enabled: !connecting)
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
            Text(GithubCopy.footerExplain)
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
                    mode: .action { Task { await load(refresh: true) } },
                    enabled: !loading
                )
                if data.installUrl != nil {
                    GlassPill(GithubCopy.installAnother, icon: AppIcons.uiAdd, mode: .action {
                        openInstall(data.installUrl)
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

    // FEED-30: integrations.github.lookupRepo for the typed `owner/name`; a
    // hit is ADDED exactly like a row tap, a miss shows the server's message
    // inline (it runs the connect gate, so it names the real reason).
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
    // open with the error inline. FORBIDDEN is checked FIRST so a dead GitHub
    // connection is never misread as a plan limit (desktop parity).
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

    // Connect (or reconnect) the viewer's GitHub account: the link-ticket hop,
    // the guided page as the fallback. Every outcome but a failure re-lists.
    private func connect(_ data: GithubReposResult) {
        guard !connecting else { return }
        connectError = nil
        connecting = true
        connectSession.start(deps: deps, accountId: accountId, connectUrl: data.connectUrl) { outcome in
            connecting = false
            switch outcome {
            case .connected, .cancelled:
                Task { await load(refresh: true) }
            case let .failed(message):
                connectError = message
            }
        }
    }

    // GitHub's install page in the system browser: the App's setup URL lands
    // on the web page, whose "Return to the app" deep-links back here.
    private func openInstall(_ urlString: String?) {
        guard let urlString, let url = URL(string: urlString) else { return }
        connectError = nil
        openURL(url)
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

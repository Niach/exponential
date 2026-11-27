import ExpCore
import ExpUI
import SwiftUI

/// #EXP-2171 NEW › REPOSITORY (repo-page.json `copy.new`, owners; web
/// `/t/$teamSlug/new/repository`): the Settings › Repositories Add sheet's
/// choice as a pushed page. `ChoiceTiles` Host on Exponential (first,
/// preselected) | Link from GitHub. Hosted: Name, Description, Default
/// branch, Run CI (on), Mirror to GitHub revealing its URL, then Create
/// repository (`repositories.create`, then `ci.setRepoConfig {enabled}`
/// while Run CI is on). GitHub: today's connect + picker flow
/// (`repositories.add`). A created repository opens its page; a failure is a
/// toast, an invalid name inline.
struct NewRepositoryPage: View {
    let teamId: String
    /// The new repository's id: the host replaces this page with its page.
    var onCreated: @MainActor (String) -> Void = { _ in }

    enum Provider: Int { case hosted = 0, github = 1 }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @State private var provider: Provider = .hosted
    @State private var name = ""
    @State private var summary = ""
    @State private var branch = RepoPage.addBranches[0]
    @State private var runCi = true
    @State private var mirror = false
    @State private var mirrorUrl = ""
    @State private var pending = false
    @State private var picking = false
    /// The GitHub repository the picker linked: its page opens once the sheet is gone.
    @State private var linked: String?

    private typealias C = RepoPage.Copy

    private var trimmedName: String { name.trimmingCharacters(in: .whitespaces) }
    private var nameInvalid: Bool { !trimmedName.isEmpty && !RepoPage.validName(trimmedName) }
    private var valid: Bool {
        RepoPage.validName(trimmedName) && (!mirror || mirrorUrl.trimmingCharacters(in: .whitespaces).hasPrefix("https://"))
    }

    var body: some View {
        ZStack {
            AppBackground()
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    tiles
                    switch provider {
                    case .hosted: hosted
                    case .github: github
                    }
                }
                .padding(16)
            }
            .scrollDismissesKeyboard(.interactively)
        }
        .pageHeader(C.New.title, pushed: true)
        .accessibilityIdentifier("new-repository-page")
        .sheet(isPresented: $picking, onDismiss: {
            if let linked {
                self.linked = nil
                onCreated(linked)
            }
        }) {
            GithubRepoPicker(
                accountId: accountId,
                teamId: teamId,
                integrationsApi: deps.integrationsApi,
                onAdd: { repo in
                    linked = try await deps.repositoriesApi.add(
                        accountId: accountId,
                        teamId: teamId,
                        fullName: repo.fullName,
                        defaultBranch: repo.defaultBranch,
                        isPrivate: repo.`private`
                    )
                    RepositoryDirectory.invalidate(accountId: accountId, teamId: teamId)
                },
                title: C.New.link
            )
        }
    }

    private var tiles: some View {
        GlassChoiceTiles(
            [
                GlassChoiceTile(
                    label: C.New.hosted, description: C.New.hostedHint, hasPicture: false,
                    identifier: "new-repo-provider-hosted"
                ),
                GlassChoiceTile(
                    label: C.New.github, description: C.New.githubHint, hasPicture: false,
                    identifier: "new-repo-provider-github"
                ),
            ],
            selected: [provider.rawValue],
            multiple: false,
            columns: 2,
            onToggle: { provider = Provider(rawValue: $0) ?? .hosted }
        ) { _ in EmptyView() }
        .accessibilityIdentifier("new-repo-provider")
    }

    // MARK: - Host on Exponential

    private var hosted: some View {
        VStack(alignment: .leading, spacing: 16) {
            VStack(spacing: 0) {
                // An invalid name reads red in place (web `aria-invalid`).
                field(C.Add.name, text: $name, placeholder: "app", id: "new-repo-name", invalid: nameInvalid)
                field(C.New.description, text: $summary, placeholder: "", id: "new-repo-description")
                GlassPickerRow(C.Add.defaultBranch, selection: $branch, options: RepoPage.addBranches, label: { $0 })
                    .formRowPadding()
                GlassToggleRow(C.New.runCi, isOn: $runCi)
                    .formRowPadding()
                    .accessibilityIdentifier("new-repo-run-ci")
                GlassToggleRow(C.Add.mirror, isOn: $mirror)
                    .formRowPadding()
                    .accessibilityIdentifier("new-repo-mirror")
                if mirror {
                    field(
                        C.Add.mirrorUrl, text: $mirrorUrl, placeholder: "https://github.com/owner/name",
                        id: "new-repo-mirror-url", url: true
                    )
                }
            }
            .glassSection()
            // `.contain` keeps the fields' own ids reachable.
            .accessibilityElement(children: .contain)
            Button { create() } label: {
                GlassSubmitLabel(C.New.create, enabled: valid && !pending, loading: pending)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.pressScale)
            .disabled(!valid || pending)
            .accessibilityIdentifier("new-repo-create")
        }
    }

    private func field(
        _ label: String, text: Binding<String>, placeholder: String, id: String, url: Bool = false, invalid: Bool = false
    ) -> some View {
        HStack(spacing: 12) {
            Text(label)
                .foregroundStyle(.white)
                .fixedSize()
            GlassTextField(
                placeholder, text: text, bordered: false, accessibilityIdentifier: id,
                foreground: invalid ? DesignTokens.Semantic.red : .white.opacity(TextOpacity.secondary)
            )
            .multilineTextAlignment(.trailing)
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
            .keyboardType(url ? .URL : .default)
        }
        .formRowPadding()
    }

    // MARK: - Link from GitHub

    /// The existing picker (it walks a missing GitHub connection first); a
    /// linked repository opens its page.
    private var github: some View {
        Button { picking = true } label: {
            GlassSubmitLabel(C.New.link, enabled: true) {
                AppIcon(AppIcons.uiGithub, size: AppIcon.Size.small)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.pressScale)
        .accessibilityIdentifier("new-repo-link")
    }

    // MARK: - Create

    private func create() {
        guard valid, !pending else { return }
        pending = true
        let trimmedSummary = summary.trimmingCharacters(in: .whitespacesAndNewlines)
        Task {
            defer { pending = false }
            do {
                let id = try await deps.repositoriesApi.create(
                    accountId: accountId,
                    teamId: teamId,
                    name: trimmedName,
                    defaultBranch: branch,
                    mirrorUrl: mirror ? mirrorUrl.trimmingCharacters(in: .whitespaces) : nil,
                    description: trimmedSummary.isEmpty ? nil : trimmedSummary
                )
                RepositoryDirectory.invalidate(accountId: accountId, teamId: teamId)
                guard let id else { return }
                if runCi {
                    do {
                        try await deps.ciApi.setRepoConfig(accountId: accountId, repositoryId: id, enabled: true)
                    } catch {
                        toaster.error(
                            FeedbackStates.Errors.title(verb: "turn on", thing: "CI"), description: error.trpcUserMessage
                        )
                    }
                }
                onCreated(id)
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "create", thing: "the repository"),
                    description: error.trpcUserMessage,
                    action: ToastAction(label: FeedbackStates.Errors.retry) { Task { @MainActor in create() } }
                )
            }
        }
    }
}

private extension View {
    func formRowPadding() -> some View {
        padding(.horizontal, 12)
            .padding(.vertical, 10)
    }
}

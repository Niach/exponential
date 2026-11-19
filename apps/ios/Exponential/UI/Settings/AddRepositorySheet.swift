import ExpCore
import ExpUI
import SwiftUI

/// #EXP-1616 (repo-page.json `copy.add`; web `repositories-section.tsx` +
/// `add-ecode-repository.tsx`): Add repository as THE sheet. Owners pick
/// where the repo lives (`GlassChoiceTiles` GitHub | Exponential); GitHub
/// keeps the existing picker, Exponential creates an empty repo on the team
/// bucket (`repositories.create`). A member sees the plain GitHub picker.
struct AddRepositorySheet: View {
    let accountId: String
    let teamId: String
    let isOwner: Bool
    let integrationsApi: IntegrationsApi
    let repositoriesApi: RepositoriesApi
    /// The GitHub pick, added inside the sheet (a throw keeps it open).
    var onAddGithub: @MainActor (GithubPickerRepo) async throws -> Void
    /// An Exponential repo was created.
    var onCreated: @MainActor () -> Void

    enum Provider: Int { case github = 0, ecode = 1 }

    @State private var provider: Provider

    init(
        accountId: String,
        teamId: String,
        isOwner: Bool,
        integrationsApi: IntegrationsApi,
        repositoriesApi: RepositoriesApi,
        initialProvider: Provider = .github,
        onAddGithub: @escaping @MainActor (GithubPickerRepo) async throws -> Void,
        onCreated: @escaping @MainActor () -> Void
    ) {
        self.accountId = accountId
        self.teamId = teamId
        self.isOwner = isOwner
        self.integrationsApi = integrationsApi
        self.repositoriesApi = repositoriesApi
        self.onAddGithub = onAddGithub
        self.onCreated = onCreated
        _provider = State(initialValue: initialProvider)
    }

    var body: some View {
        if isOwner && provider == .ecode {
            AddEcodeRepositoryForm(
                accountId: accountId,
                teamId: teamId,
                repositoriesApi: repositoriesApi,
                tiles: AnyView(tiles),
                onCreated: onCreated
            )
        } else {
            GithubRepoPicker(
                accountId: accountId,
                teamId: teamId,
                integrationsApi: integrationsApi,
                onAdd: onAddGithub,
                title: isOwner ? RepoPage.Copy.Add.title : GithubCopy.pickerTitle,
                header: isOwner ? AnyView(tiles) : nil
            )
        }
    }

    private var tiles: some View {
        GlassChoiceTiles(
            [
                GlassChoiceTile(label: RepoPage.Copy.Add.github, hasPicture: false, identifier: "add-repo-provider-github"),
                GlassChoiceTile(label: RepoPage.Copy.Add.ecode, hasPicture: false, identifier: "add-repo-provider-ecode"),
            ],
            selected: [provider.rawValue],
            multiple: false,
            columns: 2,
            onToggle: { index in provider = Provider(rawValue: index) ?? .github }
        ) { _ in EmptyView() }
        .accessibilityIdentifier("add-repo-provider")
    }
}

/// The Exponential side of Add repository: Name, Default branch (main |
/// master | develop), Mirror to GitHub (revealing the Mirror URL), then
/// Create. A failure = a toast with Retry, the sheet stays.
private struct AddEcodeRepositoryForm: View {
    let accountId: String
    let teamId: String
    let repositoriesApi: RepositoriesApi
    let tiles: AnyView
    var onCreated: @MainActor () -> Void

    @Environment(\.dismiss) private var dismiss
    @Environment(\.toaster) private var toaster
    @State private var name = ""
    @State private var branch = RepoPage.addBranches[0]
    @State private var mirror = false
    @State private var mirrorUrl = ""
    @State private var pending = false

    private var valid: Bool {
        RepoPage.validName(name) && (!mirror || mirrorUrl.trimmingCharacters(in: .whitespaces).hasPrefix("https://"))
    }

    var body: some View {
        GlassSheetChrome(title: RepoPage.Copy.Add.title) {
            VStack(alignment: .leading, spacing: 16) {
                tiles
                VStack(spacing: 0) {
                    HStack(spacing: 12) {
                        Text(RepoPage.Copy.Add.name)
                            .foregroundStyle(.white)
                            .fixedSize()
                        GlassTextField("app", text: $name, bordered: false, accessibilityIdentifier: "ecode-repo-name")
                            .multilineTextAlignment(.trailing)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                    }
                    .formRowPadding()
                    GlassPickerRow(
                        RepoPage.Copy.Add.defaultBranch,
                        selection: $branch,
                        options: RepoPage.addBranches,
                        label: { $0 }
                    )
                    .formRowPadding()
                    GlassToggleRow(RepoPage.Copy.Add.mirror, isOn: $mirror)
                        .formRowPadding()
                        .accessibilityIdentifier("ecode-repo-mirror")
                    if mirror {
                        HStack(spacing: 12) {
                            Text(RepoPage.Copy.Add.mirrorUrl)
                                .foregroundStyle(.white)
                                .fixedSize()
                            GlassTextField(
                                "https://github.com/owner/name",
                                text: $mirrorUrl,
                                bordered: false,
                                accessibilityIdentifier: "ecode-repo-mirror-url"
                            )
                            .multilineTextAlignment(.trailing)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            .keyboardType(.URL)
                        }
                        .formRowPadding()
                    }
                }
                .glassSection()
                // `.contain` keeps the fields' own ids reachable.
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("add-ecode-repository")
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 16)
        } primaryAction: {
            Button { create() } label: {
                GlassSubmitLabel(
                    RepoPage.Copy.Add.create,
                    enabled: valid && !pending,
                    loading: pending
                )
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(!valid || pending)
            .accessibilityIdentifier("ecode-repo-create")
        }
    }

    private func create() {
        guard valid, !pending else { return }
        pending = true
        Task {
            defer { pending = false }
            do {
                try await repositoriesApi.create(
                    accountId: accountId,
                    teamId: teamId,
                    name: name,
                    defaultBranch: branch,
                    mirrorUrl: mirror ? mirrorUrl.trimmingCharacters(in: .whitespaces) : nil
                )
                onCreated()
                dismiss()
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "create", thing: "the repository"),
                    description: error.trpcUserMessage,
                    action: ToastAction(label: FeedbackStates.Errors.retry) {
                        Task { @MainActor in create() }
                    }
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

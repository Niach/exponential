import ExpUI
import ExpCore
import SwiftUI

/// The board form's repository + branch block (EXP-712), shared by the
/// create-board form and the per-board settings sheet — the iOS twin of web's
/// `board-repo-field.tsx`.
///
/// The repository control is ONE select, never a list of rows: "No
/// repository", the team's connected repos, then a trailing "Connect another
/// repository…" that opens the installed-repos picker (a brand-new repo is
/// reported through `onConnectNew`; the host decides whether that connects now
/// or on submit). Below it, only once a repo is chosen, the branch its coding
/// sessions start from — the repo's default unless the board pins another.
/// Nothing here mutates: the host owns persistence (create saves on submit,
/// settings mutates per change).
struct BoardRepoField: View {
    let accountId: String
    let teamId: String
    /// The selected registry repo, or nil for "No repository".
    var repositoryId: String?
    /// A repo picked in the GitHub picker but not connected yet (the create
    /// form connects it on submit). When set it IS the selection.
    var inlineRepo: GithubPickerRepo?
    let onSelectRegistry: (TeamRepo?) -> Void
    let onConnectNew: (GithubPickerRepo) -> Void
    /// The board's own branch pin; nil = the repo's default.
    var branch: String?
    let onBranchChange: (String?) -> Void
    var disabled = false
    var errorText: String?

    @Environment(AppDependencies.self) private var deps

    @State private var repos: [TeamRepo]?
    @State private var loadError: String?
    @State private var showOptions = false
    @State private var showPicker = false
    /// Set by the select's trailing action; the picker opens once the options
    /// sheet has finished dismissing (two sheets from one view never overlap).
    @State private var pendingConnect = false
    /// Local mirror of the inline repo's typed branch — the host only hears
    /// about trimmed, non-empty values.
    @State private var inlineBranch = ""

    /// One row of the repository select.
    private enum RepoOption: Hashable {
        case none
        case registry(String)
        case inline(String)
        case connect
    }

    private var loading: Bool { repos == nil }

    private var selectedRepo: TeamRepo? {
        guard let repositoryId else { return nil }
        return repos?.first { $0.id == repositoryId }
    }

    private var selection: RepoOption {
        if let inlineRepo { return .inline(inlineRepo.fullName) }
        if let repositoryId { return .registry(repositoryId) }
        return .none
    }

    private var options: [RepoOption] {
        var list: [RepoOption] = [.none]
        list.append(contentsOf: (repos ?? []).map { .registry($0.id) })
        if let inlineRepo { list.append(.inline(inlineRepo.fullName)) }
        list.append(.connect)
        return list
    }

    private var hasRepos: Bool { !(repos ?? []).isEmpty || inlineRepo != nil }

    private var connectLabel: String {
        hasRepos ? "Connect another repository…" : "Connect a GitHub repository…"
    }

    /// The branch that means "follow the repo": `repositories.list` already
    /// folds the team's pin into `defaultBranch`; a not-yet-connected repo
    /// only knows GitHub's default.
    private var repoDefault: String? {
        if let inlineRepo { return inlineRepo.defaultBranch }
        return selectedRepo?.defaultBranch
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            VStack(alignment: .leading, spacing: 8) {
                fieldLabel("Repository")
                repositoryRow
            }

            if let repoDefault {
                VStack(alignment: .leading, spacing: 8) {
                    fieldLabel("Branch")
                    branchControl(repoDefault: repoDefault)
                }
            }

            // The ONE explanatory line under the block — byte-identical on
            // every client (lib/board-copy.ts, gated by board-copy.test.ts).
            Text("Coding sessions start from here.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))

            if let message = errorText ?? loadError {
                Text(message).font(.caption).foregroundStyle(.red.opacity(0.8))
            }
        }
        .task(id: teamId) { await load() }
        // The typed branch belongs to the repo it was typed for — a new
        // selection drops it (the host has already cleared its own pin).
        .onChange(of: selectedFullName) { _, _ in
            inlineBranch = branch ?? ""
        }
        // A repo connected through the picker isn't in the list yet — re-read
        // the registry so the trigger and the branch field can resolve it.
        .onChange(of: repositoryId) { _, id in
            guard let id, repos?.contains(where: { $0.id == id }) == false else { return }
            Task { await load() }
        }
        .sheet(isPresented: $showOptions, onDismiss: {
            guard pendingConnect else { return }
            pendingConnect = false
            showPicker = true
        }) {
            GlassPickerSheet(
                title: "Repository",
                items: options,
                selectedID: selection,
                idFor: { $0 },
                onSelect: select
            ) { option in
                optionRow(option)
            }
        }
        .sheet(isPresented: $showPicker) {
            GithubRepoPicker(
                accountId: accountId,
                teamId: teamId,
                integrationsApi: deps.integrationsApi
            ) { repo in
                onConnectNew(repo)
            }
        }
    }

    // MARK: - Repository select

    private var repositoryRow: some View {
        Button {
            guard !disabled, !loading else { return }
            showOptions = true
        } label: {
            HStack(spacing: 10) {
                AppIcon(AppIcons.uiRepository, size: 13)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text(triggerLabel)
                    .font(selectedFullName == nil ? .subheadline : .subheadline.monospaced())
                    .foregroundStyle(.white.opacity(selectedFullName == nil
                        ? TextOpacity.secondary
                        : TextOpacity.primary))
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 8)
                if selectedIsPrivate {
                    AppIcon(AppIcons.uiPrivate, size: 11)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                AppIcon(AppIcons.uiChevronDown, size: 12)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
        }
        .buttonStyle(.plain)
        .disabled(disabled || loading)
        .opacity(disabled ? 0.5 : 1)
    }

    private var selectedFullName: String? {
        inlineRepo?.fullName ?? selectedRepo?.fullName
    }

    private var selectedIsPrivate: Bool {
        inlineRepo?.`private` ?? selectedRepo?.isPrivate ?? false
    }

    private var triggerLabel: String {
        if loading { return "Loading…" }
        return selectedFullName ?? "No repository"
    }

    @ViewBuilder
    private func optionRow(_ option: RepoOption) -> some View {
        switch option {
        case .none:
            Text("No repository")
                .font(.subheadline)
                .foregroundStyle(.white)
        case let .registry(id):
            let repo = repos?.first { $0.id == id }
            repoOptionLabel(fullName: repo?.fullName ?? id, isPrivate: repo?.isPrivate ?? false)
        case let .inline(fullName):
            repoOptionLabel(fullName: fullName, isPrivate: inlineRepo?.`private` ?? false)
        case .connect:
            HStack(spacing: 10) {
                AppIcon(AppIcons.uiAdd, size: 13)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text(connectLabel)
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
            }
        }
    }

    private func repoOptionLabel(fullName: String, isPrivate: Bool) -> some View {
        HStack(spacing: 10) {
            AppIcon(AppIcons.uiGithub, size: 13)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text(fullName)
                .font(.subheadline.monospaced())
                .foregroundStyle(.white)
                .lineLimit(1)
                .truncationMode(.middle)
            if isPrivate {
                AppIcon(AppIcons.uiPrivate, size: 11)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        }
    }

    private func select(_ option: RepoOption) {
        switch option {
        case .connect:
            pendingConnect = true
        case .none:
            inlineBranch = ""
            onSelectRegistry(nil)
        case let .registry(id):
            guard let repo = repos?.first(where: { $0.id == id }) else { return }
            inlineBranch = ""
            onSelectRegistry(repo)
        case .inline:
            break
        }
    }

    // MARK: - Branch

    @ViewBuilder
    private func branchControl(repoDefault: String) -> some View {
        if inlineRepo != nil || selectedRepo == nil {
            // A repo that isn't connected yet has no id to list branches from,
            // so the branch is typed; the placeholder IS what the board follows.
            GlassTextField(repoDefault, text: Binding(
                get: { inlineBranch },
                set: { value in
                    inlineBranch = value
                    let trimmed = value.trimmingCharacters(in: .whitespaces)
                    onBranchChange(trimmed.isEmpty ? nil : trimmed)
                }
            ), horizontalPadding: 12, verticalPadding: 10) {
                AppIcon(AppIcons.uiBranch, size: 13)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            } trailing: {
                EmptyView()
            }
            .font(.subheadline.monospaced())
            .autocorrectionDisabled()
            .textInputAutocapitalization(.never)
            .disabled(disabled)
        } else if let repo = selectedRepo {
            BranchPickerRow(
                accountId: accountId,
                repositoryId: repo.id,
                value: branch ?? repoDefault,
                repoDefault: repoDefault,
                disabled: disabled,
                onPick: onBranchChange
            )
        }
    }

    // MARK: - Data

    private func fieldLabel(_ text: String) -> some View {
        Text(text)
            .font(.caption.weight(.medium))
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
    }

    private func load() async {
        do {
            repos = try await deps.repositoriesApi.list(accountId: accountId, teamId: teamId)
            loadError = nil
        } catch {
            repos = []
            loadError = error.trpcUserMessage
        }
    }
}

/// The board's branch, as a select-like row over `repositories.listBranches`
/// (EXP-712, web's `BranchCombobox`). `value` is the EFFECTIVE branch and is
/// always offered even when GitHub no longer has it (a pin deleted upstream) —
/// otherwise the list couldn't show what the board is set to. Picking
/// `repoDefault` reports nil: follow the repo again.
private struct BranchPickerRow: View {
    let accountId: String
    let repositoryId: String
    let value: String
    let repoDefault: String
    var disabled = false
    let onPick: (String?) -> Void

    @State private var showSheet = false

    var body: some View {
        Button {
            guard !disabled else { return }
            showSheet = true
        } label: {
            HStack(spacing: 10) {
                AppIcon(AppIcons.uiBranch, size: 13)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text(value)
                    .font(.subheadline.monospaced())
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 8)
                AppIcon(AppIcons.uiChevronDown, size: 12)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
        }
        .buttonStyle(.plain)
        .disabled(disabled)
        .opacity(disabled ? 0.5 : 1)
        .sheet(isPresented: $showSheet) {
            BranchPickerSheet(
                accountId: accountId,
                repositoryId: repositoryId,
                value: value,
                repoDefault: repoDefault,
                onPick: onPick
            )
        }
    }
}

private struct BranchPickerSheet: View {
    let accountId: String
    let repositoryId: String
    let value: String
    let repoDefault: String
    let onPick: (String?) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss

    @State private var branches: [String]?
    @State private var query = ""
    @State private var errorText: String?

    /// The effective branch always renders, even when GitHub dropped it.
    private var names: [String] {
        guard let branches else { return [] }
        let all = branches.contains(value) ? branches : [value] + branches
        let needle = query.trimmingCharacters(in: .whitespaces)
        guard !needle.isEmpty else { return all }
        return all.filter { $0.localizedCaseInsensitiveContains(needle) }
    }

    var body: some View {
        GlassSheetChrome(title: "Branch") {
            VStack(alignment: .leading, spacing: 8) {
                if branches == nil && errorText == nil {
                    HStack(spacing: 8) {
                        ProgressView().controlSize(.small).tint(.white.opacity(0.5))
                        Text("Loading branches…")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                    .padding(.vertical, 12)
                }

                if let errorText {
                    Text(errorText).font(.caption).foregroundStyle(.red.opacity(0.8))
                    GlassPillButton("Retry", icon: AppIcons.uiRefresh) {
                        Task { await load() }
                    }
                }

                if branches != nil {
                    GlassSheetSearchField(placeholder: "Search branches…", text: $query)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()

                    if names.isEmpty {
                        Text("No branches found.")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .padding(.vertical, 8)
                    }

                    ForEach(names, id: \.self) { name in
                        Button {
                            dismiss()
                            guard name != value else { return }
                            onPick(name == repoDefault ? nil : name)
                        } label: {
                            HStack(spacing: 10) {
                                Text(name)
                                    .font(.subheadline.monospaced())
                                    .foregroundStyle(.white)
                                    .lineLimit(1)
                                    .truncationMode(.middle)
                                if name == repoDefault {
                                    Text("default")
                                        .font(.caption)
                                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                }
                                Spacer(minLength: 8)
                                if name == value {
                                    AppIcon(AppIcons.uiCheck, size: 15, weight: .semibold)
                                        .foregroundStyle(.white)
                                }
                            }
                            .padding(.horizontal, 12)
                            .frame(minHeight: 44)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 16)
        }
        .task { await load() }
    }

    private func load() async {
        errorText = nil
        do {
            branches = try await deps.repositoriesApi.listBranches(
                accountId: accountId,
                repositoryId: repositoryId
            )
        } catch {
            branches = nil
            errorText = error.trpcUserMessage
        }
    }
}

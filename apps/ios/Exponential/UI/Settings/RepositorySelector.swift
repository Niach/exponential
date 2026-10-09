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
/// EXP-862: both controls are glass picker ROWS with their labels inside them
/// (×4) — the "Repository" / "Branch" captions above them are gone; only the
/// one shared explanatory line survives.
///
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
    /// FEED-32: the one-shot re-list for a linked id the list doesn't know —
    /// `reloadedForId` guards against looping on a genuinely unknown id (an
    /// archived repo), `resolvingId` drives the "Loading repository…" label
    /// while that re-list is in flight.
    @State private var reloadedForId: String?
    @State private var resolvingId: String?
    @State private var showOptions = false
    @State private var showPicker = false
    /// Set by the select's trailing action; the picker opens once the options
    /// sheet has finished dismissing (two sheets from one view never overlap).
    @State private var pendingConnect = false
    /// Local mirror of the inline repo's typed branch — the host only hears
    /// about trimmed, non-empty values.
    @State private var inlineBranch = ""

    private var loading: Bool { repos == nil }

    private var selectedRepo: TeamRepo? {
        guard let repositoryId else { return nil }
        return repos?.first { $0.id == repositoryId }
    }

    /// The inline (not yet connected) repo's row id — a full name can never
    /// collide with a registry UUID, but the prefix makes it explicit.
    private static let inlinePrefix = "inline:"

    /// The picked row: the inline repo when there is one, else the registry id.
    private var selection: String? {
        if let inlineRepo { return Self.inlinePrefix + inlineRepo.fullName }
        return repositoryId
    }

    /// The registry's repos, then the inline pick (THE repository rows).
    private var pickerRows: [RepositoryPickerRow] {
        var rows = (repos ?? []).map {
            RepositoryPickerRow(id: $0.id, fullName: $0.fullName, isPrivate: $0.isPrivate)
        }
        if let inlineRepo {
            rows.append(RepositoryPickerRow(
                id: Self.inlinePrefix + inlineRepo.fullName,
                fullName: inlineRepo.fullName,
                isPrivate: inlineRepo.`private`
            ))
        }
        return rows
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
        VStack(alignment: .leading, spacing: 8) {
            // EXP-862: picker ROWS that carry their own labels — the captions
            // that used to sit above each control are gone (×4).
            repositoryRow

            if let repoDefault {
                branchControl(repoDefault: repoDefault)
            }

            // The ONE explanatory line under the block — byte-identical on
            // every client (lib/board-copy.ts, gated by board-copy.test.ts).
            Text("Runs start from here.")
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
        // the registry so the trigger and the branch field can resolve it
        // (FEED-32: once per unknown id, never a loop).
        .onChange(of: repositoryId) { _, _ in
            resolveUnknownRepo()
        }
        .sheet(isPresented: $showPicker) {
            GithubRepoPicker(
                accountId: accountId,
                teamId: teamId,
                integrationsApi: deps.integrationsApi
            ) { repo in
                // Nothing to persist here — the host connects the pick (now or
                // on submit), so the in-sheet add always succeeds.
                onConnectNew(repo)
            }
        }
    }

    // MARK: - Repository select

    /// EXP-1021: the SHARED `RepositoryPicker`; the row is its trigger. "No
    /// repository" leads, the trailing "Connect another repository…" is the
    /// sheet's footer — it closes the select, and the GitHub listing opens
    /// once that sheet has finished dismissing.
    private var repositoryRow: some View {
        RepositoryPicker(
            repositories: pickerRows,
            value: selection,
            onChange: select,
            noneLabel: "No repository",
            emptyText: nil,
            disabled: disabled || loading,
            footer: { AnyView(connectRow) },
            open: $showOptions,
            onDismiss: {
                guard pendingConnect else { return }
                pendingConnect = false
                showPicker = true
            }
        ) {
            HStack(spacing: 10) {
                Text("Repository")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.primary))
                Spacer(minLength: 8)
                Text(triggerLabel)
                    .font(selectedFullName == nil ? .subheadline : .subheadline.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
                    .truncationMode(.middle)
                if selectedIsPrivate {
                    AppIcon(AppIcons.uiPrivate, size: 11)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                AppIcon(AppIcons.uiChevronRight, size: 12)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
            .contentShape(Rectangle())
        }
        .opacity(disabled ? 0.5 : 1)
    }

    /// The select's footer row: hand off to the GitHub listing.
    private var connectRow: some View {
        Button {
            pendingConnect = true
            showOptions = false
        } label: {
            HStack(spacing: 10) {
                AppIcon(AppIcons.uiAdd, size: AppIcon.Size.medium)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .frame(width: GlassPickerTokens.markWidth)
                Text(connectLabel)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.primary))
                    .lineLimit(1)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, GlassPickerTokens.rowHPadding)
            .frame(minHeight: GlassPickerTokens.rowMinHeight)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("board-repo-connect")
    }

    private var selectedFullName: String? {
        inlineRepo?.fullName ?? selectedRepo?.fullName
    }

    private var selectedIsPrivate: Bool {
        inlineRepo?.`private` ?? selectedRepo?.isPrivate ?? false
    }

    // FEED-32: never blank — a linked id the list can't resolve reads
    // "Loading repository…" while the one-shot re-list runs and "Repository
    // unavailable" once it came back without the id (web parity).
    private var triggerLabel: String {
        BoardRepoLabel.trigger(
            selectedName: selectedFullName,
            repositoryId: repositoryId,
            loading: loading,
            resolving: repositoryId != nil && resolvingId == repositoryId
        )
    }

    private func select(_ id: String?) {
        guard let id else {
            inlineBranch = ""
            onSelectRegistry(nil)
            return
        }
        // The inline row is already the selection; nothing to report.
        guard !id.hasPrefix(Self.inlinePrefix),
              let repo = repos?.first(where: { $0.id == id }) else { return }
        inlineBranch = ""
        onSelectRegistry(repo)
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
                Text("Branch")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.primary))
            } trailing: {
                EmptyView()
            }
            .multilineTextAlignment(.trailing)
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

    private func load() async {
        do {
            repos = try await deps.repositoriesApi.list(accountId: accountId, teamId: teamId)
            loadError = nil
        } catch {
            // FEED-32: a failed list used to collapse silently — keep the
            // last list and say why the selection can't be resolved.
            repos = repos ?? []
            loadError = "Couldn't load the team's repositories: \(error.trpcUserMessage)"
        }
        resolveUnknownRepo()
    }

    /// FEED-32: the host can point the board at a repo this list has never
    /// seen — the settings sheet connects a new repo and the LIVE board row
    /// flips `repositoryId` before this copy of `repositories.list` refreshed.
    /// Re-list ONCE per unknown id (a genuinely unknown id must not loop) and
    /// label the trigger explicitly meanwhile.
    private func resolveUnknownRepo() {
        guard let id = repositoryId, let repos, !repos.contains(where: { $0.id == id }) else { return }
        guard reloadedForId != id else { return }
        reloadedForId = id
        resolvingId = id
        Task {
            await load()
            if resolvingId == id { resolvingId = nil }
        }
    }
}

/// The board's branch, as a select-like row over `repositories.listBranches`
/// (EXP-712, web's `BranchPicker`): the SHARED `BranchPicker`, its rows read
/// when the sheet opens. `value` is the EFFECTIVE branch; picking
/// `repoDefault` reports nil — follow the repo again.
private struct BranchPickerRow: View {
    let accountId: String
    let repositoryId: String
    let value: String
    let repoDefault: String
    var disabled = false
    let onPick: (String?) -> Void

    @Environment(AppDependencies.self) private var deps

    @State private var showSheet = false
    @State private var branches: [String]?
    @State private var errorText: String?

    var body: some View {
        BranchPicker(
            branches: branches,
            value: value,
            repoDefault: repoDefault,
            onChange: onPick,
            errorText: errorText,
            onRetry: { Task { await load() } },
            disabled: disabled,
            open: $showSheet
        ) {
            HStack(spacing: 10) {
                Text("Branch")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.primary))
                Spacer(minLength: 8)
                Text(value)
                    .font(.subheadline.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
                    .truncationMode(.middle)
                AppIcon(AppIcons.uiChevronRight, size: 12)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
            .contentShape(Rectangle())
        }
        .opacity(disabled ? 0.5 : 1)
        .onChange(of: showSheet) { _, open in
            if open { Task { await load() } }
        }
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

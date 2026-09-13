import ExpUI
import ExpCore
import SwiftUI

/// The create-board form's state, hoisted out of the view (EXP-687) so a host
/// can own the submit button: `CreateBoardSheet` pins it to the sheet bottom
/// while the onboarding PAGE keeps its inline one. Android's twin is
/// `rememberCreateBoardFormState()`.
@MainActor
@Observable
final class CreateBoardDraft {
    var name = ""
    var prefix = ""
    /// Stop deriving the prefix from the name once the user edits it by hand.
    var prefixEdited = false
    var color = DEFAULT_LABEL_COLOR
    var icon = "square-kanban"
    /// The picked registry repo (EXP-712: the field is one select, so the two
    /// selection kinds are held apart instead of pre-folded into the wire
    /// choice). Exactly one of these is ever set.
    var repositoryId: String?
    /// A repo picked in the GitHub picker but not connected yet — `create`
    /// connects it inline by `fullName`.
    var inlineRepo: GithubPickerRepo?
    /// The board's own branch; nil = the repo's default. Reset with the repo —
    /// a branch belongs to the repo it was picked in.
    var defaultBranch: String?
    var saving = false
    var errorText: String?
    /// Plan-cap failures render as a softer nudge than hard errors.
    var limitText: String?

    /// A repository is optional on every board — creation only needs a name
    /// and prefix.
    var canCreate: Bool {
        !name.trimmingCharacters(in: .whitespaces).isEmpty
            && !prefix.trimmingCharacters(in: .whitespaces).isEmpty
            && !saving
    }

    var submitLabel: String { saving ? "Creating…" : "Create board" }

    /// The wire form of the repo selection (`boards.create`'s union).
    var repository: BoardRepositoryChoice? {
        if let inlineRepo {
            return .fullName(
                inlineRepo.fullName,
                defaultBranch: inlineRepo.defaultBranch,
                isPrivate: inlineRepo.`private`
            )
        }
        return repositoryId.map { .repositoryId($0) }
    }

    /// Pick a connected repo (nil = no repository). Drops any pinned branch.
    func selectRepository(_ repo: TeamRepo?) {
        repositoryId = repo?.id
        inlineRepo = nil
        defaultBranch = nil
    }

    /// A repo picked in the GitHub picker — connected inline on create.
    func connectRepository(_ repo: GithubPickerRepo) {
        inlineRepo = repo
        repositoryId = nil
        defaultBranch = nil
    }

    func onNameChange(_ value: String) {
        name = value
        if !prefixEdited { prefix = CreateBoardForm.derivePrefix(value) }
    }

    func onPrefixChange(_ value: String) {
        prefixEdited = true
        prefix = String(value.uppercased().prefix(4))
    }

    /// Returns the new board id, or nil when the create failed (the message is
    /// already on the draft). `saving` is left set on success — the caller
    /// swaps the form out.
    func create(accountId: String, teamId: String, boardsApi: BoardsApi) async -> String? {
        guard canCreate else { return nil }
        saving = true
        errorText = nil
        limitText = nil
        do {
            // The repo is optional on every board; send whatever's selected
            // and let coding affordances gate on its presence later.
            return try await boardsApi.create(
                accountId: accountId,
                CreateBoardInput(
                    teamId: teamId,
                    name: name.trimmingCharacters(in: .whitespaces),
                    prefix: prefix.trimmingCharacters(in: .whitespaces),
                    color: color,
                    icon: icon,
                    repository: repository,
                    defaultBranch: defaultBranch
                )
            )
        } catch {
            if error.isPlanLimitError {
                limitText = error.trpcUserMessage
            } else {
                errorText = error.trpcUserMessage
            }
            saving = false
            return nil
        }
    }
}

// The create-first-board form (web onboarding parity, wizard.tsx): ONE row of
// icon + colour + name (EXP-862, ×4), the prefix, and an ALWAYS-optional
// repository plus the branch its coding sessions start from (EXP-712). Every
// field is a glass row carrying its own label — no captions above. One
// `boards.create` call carries `icon` (never the deprecated `type`). Reused by
// the first-run onboarding page and the empty-state "Create board" sheets.
struct CreateBoardForm: View {
    let accountId: String
    let teamId: String
    /// Onboarding renders the minimal spec form (shared mobile onboarding
    /// spec): the icon/colour/name row plus the optional repository. The
    /// prefix stays auto-derived from the name there; the regular sheets show
    /// its row too.
    var minimal = false
    /// The shared state. A host that pins the submit button (the sheet) passes
    /// its own; the onboarding page lets the form make one.
    @State var draft = CreateBoardDraft()
    /// False when the host renders the submit button itself.
    var showsSubmit = true
    /// Called with the new board id once `boards.create` succeeds. The
    /// caller owns what happens next (finish onboarding, dismiss a sheet, …).
    let onCreated: (String) -> Void

    @Environment(AppDependencies.self) private var deps

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            // EXP-862: ONE row — icon, colour, name (×4). The two pickers are
            // the same 36pt rounded square (a rounded square is a PICKER), the
            // name field carries its own label as its placeholder, and there is
            // no caption line above any of it.
            HStack(spacing: 8) {
                IconPicker(selection: $draft.icon, tint: Color(hex: draft.color))
                ColorSwatchPicker(selection: $draft.color)
                GlassTextField("Board name", text: Binding(
                    get: { draft.name },
                    set: { draft.onNameChange($0) }
                ), verticalPadding: 9)
                .font(.subheadline)
            }

            // The prefix rides its own glass row with the label INSIDE it —
            // the onboarding's minimal form derives it from the name instead.
            if !minimal {
                HStack(spacing: 8) {
                    Text("Prefix")
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(TextOpacity.primary))
                    Spacer(minLength: 8)
                    GlassTextField("API", text: Binding(
                        get: { draft.prefix },
                        set: { draft.onPrefixChange($0) }
                    ), bordered: false)
                    .font(.subheadline.monospaced())
                    .multilineTextAlignment(.trailing)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.characters)
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .glassRow()
            }

            // Repository + branch (always optional) — picker rows that carry
            // their own labels, plus the one shared caption line.
            BoardRepoField(
                accountId: accountId,
                teamId: teamId,
                repositoryId: draft.repositoryId,
                inlineRepo: draft.inlineRepo,
                onSelectRegistry: { draft.selectRepository($0) },
                onConnectNew: { draft.connectRepository($0) },
                branch: draft.defaultBranch,
                onBranchChange: { draft.defaultBranch = $0 }
            )

            if let errorText = draft.errorText {
                Text(errorText)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color.red.opacity(0.1))
                    .clipShape(RoundedRectangle(cornerRadius: 8))
            }
            if let limitText = draft.limitText {
                HStack(alignment: .top, spacing: 8) {
                    AppIcon(AppIcons.navGettingStarted, size: AppIcon.Size.small)
                        .foregroundStyle(DesignTokens.Semantic.blue)
                    Text(limitText)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                .frame(maxWidth: .infinity, alignment: .leading)
                .glassRow()
            }

            if showsSubmit {
                GlassSubmitButton(draft.submitLabel, enabled: draft.canCreate) {
                    Task { await create() }
                }
                .padding(.top, 8)
            }
        }
    }

    private func create() async {
        if let boardId = await draft.create(
            accountId: accountId,
            teamId: teamId,
            boardsApi: deps.boardsApi
        ) {
            onCreated(boardId)
        }
    }

    // MARK: - Editing

    /// Port of web `derivePrefix` (lib/board.ts): first letter of each word,
    /// uppercased, capped at 4 (the server cap, REV-4). Separators are
    /// whitespace / `-` / `_`.
    static func derivePrefix(_ name: String) -> String {
        let letters = name
            .split(whereSeparator: { $0 == "-" || $0 == "_" || $0.isWhitespace })
            .compactMap { $0.first.map(String.init) }
            .joined()
            .uppercased()
        return String(letters.prefix(4))
    }
}

// Sheet wrapper for the empty-state "Create board" entry points (Issues home,
// team settings). Dismisses itself once the board is created.
struct CreateBoardSheet: View {
    let accountId: String
    let teamId: String
    var onCreated: (String) -> Void = { _ in }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss
    // Owned here, not by the form: the sheet's pinned button is the submit.
    @State private var draft = CreateBoardDraft()

    var body: some View {
        GlassSheetChrome(
            title: "New board",
            content: {
                CreateBoardForm(
                    accountId: accountId,
                    teamId: teamId,
                    draft: draft,
                    showsSubmit: false,
                    onCreated: { _ in }
                )
                .padding(16)
            },
            primaryAction: {
                GlassSubmitButton(draft.submitLabel, enabled: draft.canCreate) {
                    Task { await create() }
                }
            }
        )
    }

    private func create() async {
        if let boardId = await draft.create(
            accountId: accountId,
            teamId: teamId,
            boardsApi: deps.boardsApi
        ) {
            onCreated(boardId)
            dismiss()
        }
    }
}

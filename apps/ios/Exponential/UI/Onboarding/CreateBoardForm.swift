import ExpUI
import ExpCore
import SwiftUI

// The create-first-board form (web onboarding parity, wizard.tsx): a plain
// form of name, prefix, color, icon, and an ALWAYS-optional repository. One
// `boards.create` call carries `icon` (never the deprecated `type`). Reused
// by the first-run onboarding page and the empty-state "Create board" sheets.
struct CreateBoardForm: View {
    let accountId: String
    let teamId: String
    /// Onboarding renders the minimal spec form (shared mobile onboarding
    /// spec): name + icon + optional repository. The prefix stays
    /// auto-derived from the name and the color keeps its default — the full
    /// form (prefix + color fields) remains for the regular sheets.
    var minimal = false
    /// Called with the new board id once `boards.create` succeeds. The
    /// caller owns what happens next (finish onboarding, dismiss a sheet, …).
    let onCreated: (String) -> Void

    @Environment(AppDependencies.self) private var deps

    @State private var name = ""
    @State private var prefix = ""
    // Stop deriving the prefix from the name once the user edits it by hand.
    @State private var prefixEdited = false
    @State private var color = DEFAULT_LABEL_COLOR
    @State private var icon = "square-kanban"
    @State private var repository: BoardRepositoryChoice?
    @State private var saving = false
    @State private var errorText: String?
    // Plan-cap failures render as a softer nudge than hard errors.
    @State private var limitText: String?

    // A repository is optional on every board — creation only needs a name
    // and prefix.
    private var canCreate: Bool {
        !name.trimmingCharacters(in: .whitespaces).isEmpty
            && !prefix.trimmingCharacters(in: .whitespaces).isEmpty
            && !saving
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            // Name, with the icon picker LEFT of the input (EXP-584 — web,
            // desktop and Android share the row). The picker lives in ExpUI
            // so the Start-coding sheet's `icon` action input picks from the
            // exact same curated swatches (EXP-273/575).
            VStack(alignment: .leading, spacing: 8) {
                fieldLabel("Board name")
                HStack(spacing: 8) {
                    IconPicker(selection: $icon, tint: Color(hex: color))
                    GlassTextField("e.g. Backend API", text: Binding(
                        get: { name },
                        set: { onNameChange($0) }
                    ))
                    .font(.subheadline)
                }
            }

            if !minimal {
                VStack(alignment: .leading, spacing: 8) {
                    fieldLabel("Prefix")
                    GlassTextField("e.g. API", text: Binding(
                        get: { prefix },
                        set: { onPrefixChange($0) }
                    ))
                    .font(.subheadline.monospaced())
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.characters)
                }

                // Color
                VStack(alignment: .leading, spacing: 8) {
                    fieldLabel("Color")
                    ColorSwatchGrid(selection: $color)
                }
            }

            // Repository (always optional) — the selector renders its own label.
            RepositorySelector(
                accountId: accountId,
                teamId: teamId,
                selection: $repository
            )

            if let errorText {
                Text(errorText)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color.red.opacity(0.1))
                    .clipShape(RoundedRectangle(cornerRadius: 8))
            }
            if let limitText {
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

            GlassSubmitButton(saving ? "Creating…" : "Create board", enabled: canCreate) {
                Task { await create() }
            }
        }
    }

    private func fieldLabel(_ text: String) -> some View {
        Text(text)
            .font(.caption.weight(.medium))
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
    }

    // MARK: - Editing

    private func onNameChange(_ value: String) {
        name = value
        if !prefixEdited { prefix = Self.derivePrefix(value) }
    }

    private func onPrefixChange(_ value: String) {
        prefixEdited = true
        prefix = String(value.uppercased().prefix(4))
    }

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

    // MARK: - Create

    private func create() async {
        guard canCreate else { return }
        saving = true
        errorText = nil
        limitText = nil
        do {
            // The repo is optional on every board; send whatever's selected
            // and let coding affordances gate on its presence later.
            let boardId = try await deps.boardsApi.create(
                accountId: accountId,
                CreateBoardInput(
                    teamId: teamId,
                    name: name.trimmingCharacters(in: .whitespaces),
                    prefix: prefix.trimmingCharacters(in: .whitespaces),
                    color: color,
                    icon: icon,
                    repository: repository
                )
            )
            // Leave `saving` set — the caller swaps this view out on success.
            onCreated(boardId)
        } catch {
            if error.isPlanLimitError {
                limitText = error.trpcUserMessage
            } else {
                errorText = error.trpcUserMessage
            }
            saving = false
        }
    }
}

// Sheet wrapper for the empty-state "Create board" entry points (Issues home,
// team settings). Dismisses itself once the board is created.
struct CreateBoardSheet: View {
    let accountId: String
    let teamId: String
    var onCreated: (String) -> Void = { _ in }

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            ZStack {
                AppBackground()
                ScrollView {
                    CreateBoardForm(
                        accountId: accountId,
                        teamId: teamId,
                        onCreated: { boardId in
                            onCreated(boardId)
                            dismiss()
                        }
                    )
                    .padding(16)
                }
            }
            .navigationTitle("New board")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) { Button("Cancel") { dismiss() } }
            }
        }
    }
}

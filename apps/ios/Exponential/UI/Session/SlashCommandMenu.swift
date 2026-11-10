import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// EXP-724: the composers' `/` menu — NEW-2: the curated builtins the
/// session's agent can run (compact/model/effort), then the team's PROMPTS
/// (icon + name), filtered by what has been typed so far
/// (`SlashCommands.candidates`; the launch composer lists prompts alone). Same
/// chrome as the markdown editor's `@`/`#` autocomplete (`GlassMenuSurface`,
/// `EditorAutocompleteMenu`'s row layout) and the same row content as the web
/// `steer-command-menu.tsx` and Android's `SlashCommandMenu`:
///
///   mono `/name` · muted description · trailing mono `<arg>` (a command)
///   prompt glyph · name (a prompt, web `PromptCandidateRow`)
///
/// Web's geometry: the composer's full width, every row listed (the builtins
/// AND all of the team's prompts); the list scrolls only past `maxRows`.
///
/// Rows are plain `Button`s: tapping routes back through the host, which
/// rewrites the draft and keeps the field first responder, so the keyboard
/// never drops and NOTHING is ever sent by accepting a row.
///
/// The HOST mounts it (above the composer card, gated on focus + matches +
/// not-dismissed) — the menu itself owns no state.
struct SlashCommandMenu: View {
    let candidates: [SlashCandidate]
    /// Index of the keyboard-highlighted row (hardware keyboards only; ↑/↓
    /// wrap). Out-of-range highlights nothing.
    let highlighted: Int
    let onSelect: (SlashCandidate) -> Void

    /// Rows visible before the list scrolls: the four builtins plus a
    /// team's prompts all show (web lists them uncapped); a long prompt list
    /// scrolls past this.
    private static let maxRows: CGFloat = 9
    private static let rowHeight: CGFloat = 40

    var body: some View {
        GlassMenuSurface {
            list
        }
        // The host's composer keeps its height first.
        .layoutPriority(-1)
    }

    private var list: some View {
        ScrollView(.vertical, showsIndicators: false) {
            VStack(alignment: .leading, spacing: 0) {
                ForEach(Array(candidates.enumerated()), id: \.element.id) { index, candidate in
                    switch candidate {
                    case let .command(command):
                        row(command, isHighlighted: index == highlighted)
                    case let .prompt(prompt):
                        promptRow(prompt, isHighlighted: index == highlighted)
                    }
                }
            }
            .padding(.vertical, 4)
        }
        // Its whole list when the room allows; above a keyboard it gives
        // up height (down to three rows) before the composer would.
        .frame(
            minHeight: min(CGFloat(candidates.count), 3) * Self.rowHeight + 8,
            idealHeight: min(CGFloat(candidates.count), Self.maxRows) * Self.rowHeight + 8,
            maxHeight: min(CGFloat(candidates.count), Self.maxRows) * Self.rowHeight + 8
        )
    }

    /// NEW-2: a team prompt — its curated glyph and its name.
    private func promptRow(_ prompt: SlashPrompt, isHighlighted: Bool) -> some View {
        Button {
            onSelect(.prompt(prompt))
        } label: {
            HStack(spacing: 8) {
                AppIcon(ActionIconDisplay.iconName(for: prompt.icon), size: 16)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text(prompt.name)
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .truncationMode(.tail)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 12)
            .frame(minHeight: Self.rowHeight)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.white.opacity(isHighlighted ? 0.08 : 0))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(prompt.name)
        .accessibilityIdentifier("slash-prompt-\(prompt.id)")
    }

    private func row(_ command: SlashCommand, isHighlighted: Bool) -> some View {
        Button {
            onSelect(.command(command))
        } label: {
            HStack(spacing: 8) {
                Text(command.token)
                    .font(.expMono(.caption))
                    // A confirm command discards the conversation — the same
                    // semantic red the destructive menu items wear (×4).
                    .foregroundStyle(
                        command.confirm ? DesignTokens.Semantic.red : Color.white
                    )
                    .lineLimit(1)
                    .layoutPriority(1)
                Text(command.description)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
                    .truncationMode(.tail)
                Spacer(minLength: 0)
                if !command.argHint.isEmpty {
                    Text("<\(command.argHint)>")
                        .font(.expMono(.caption))
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                        .fixedSize()
                }
            }
            .padding(.horizontal, 12)
            .frame(minHeight: Self.rowHeight)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.white.opacity(isHighlighted ? 0.08 : 0))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(command.token) \(command.description)")
    }
}

/// NEW-2: the team's prompts as the `/` menu lists them — the synced,
/// body-less `prompts` rows in the team's order (sortOrder, then name), live
/// off GRDB. A host binds it to its team; a pick's body comes from
/// `prompts.get` (`loadBody`).
@MainActor
@Observable
final class TeamSlashPrompts {
    private(set) var prompts: [SlashPrompt] = []
    @ObservationIgnored private var task: Task<Void, Never>?
    @ObservationIgnored private var boundKey: String?

    nonisolated init() {}

    func bind(accountId: String, teamId: String?, db: DatabaseManager) {
        let key = "\(accountId)/\(teamId ?? "")"
        guard boundKey != key else { return }
        stop()
        boundKey = key
        guard let teamId, let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in
            try PromptEntity
                .filter(Column("team_id") == teamId)
                .order(Column("sort_order"), Column("name"))
                .fetchAll(db)
        }
        task = Task { [weak self] in
            do {
                for try await rows in observation.values(in: pool) {
                    self?.prompts = rows.map { SlashPrompt(id: $0.id, name: $0.name, icon: $0.icon) }
                }
            } catch {}
        }
    }

    func stop() {
        task?.cancel()
        task = nil
        boundKey = nil
    }

    /// A picked prompt's body (`prompts.get`, the ONLY body path); a failure
    /// toasts `Couldn't load <name>` (web `failedAction`) and answers nil.
    static func loadBody(_ prompt: SlashPrompt, accountId: String, deps: AppDependencies) async -> String? {
        do {
            return try await deps.promptsApi.get(accountId: accountId, id: prompt.id).body ?? ""
        } catch {
            Toaster.shared.error(
                FeedbackStates.Errors.title(verb: "load", thing: prompt.name),
                description: error.userFacingMessage
            )
            return nil
        }
    }
}

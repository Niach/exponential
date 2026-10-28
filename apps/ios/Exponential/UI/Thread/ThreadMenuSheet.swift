import ExpCore
import ExpUI
import SwiftUI

/// EXP-1295: the strip's menu on a phone — THE Menu in its SHEET
/// presentation (never an inline popover): the "+" opens it untitled with
/// Link an issue · New issue; an issue chip's long press titles it with the
/// identifier, the same two rows, then `Unlink <IDENT>` (destructive). A pick hands the
/// action back; the host runs it once the sheet has dismissed (a push or a
/// second sheet from under a dismissing one is dropped). EXP-1307: the
/// Guide header's `…` opens it untitled with `guideMenuEntries` — Close PR
/// without merging (destructive; EXP-1338: no GitHub entry).
struct ThreadMenuSheet: View {
    enum Action: Equatable {
        case linkIssue
        case newIssue
        case unlink
        case closePr
    }

    static let linkIssueLabel = "Link an issue"
    static let newIssueLabel = "New issue"
    static func unlinkLabel(_ identifier: String) -> String { "Unlink \(identifier)" }

    /// The issue tab's identifier; nil = the "+" (no title, no Unlink).
    let identifier: String?
    /// EXP-1307: the Guide's `…` entries; nil = the strip's menu.
    var guideEntries: [SessionTree.GuideMenuEntry]?
    let onPick: (Action) -> Void

    var body: some View {
        GlassSheetChrome(title: identifier) {
            VStack(spacing: 0) {
                if let guideEntries {
                    ForEach(guideEntries, id: \.self) { entry in
                        switch entry {
                        case .closePr:
                            row(.closePr, ClosePrCopy.menuItem, icon: AppIcons.prClosed, destructive: true)
                        }
                    }
                } else {
                    // EXP-1334: THE thread menu's rows ×4 (desktop
                    // `thread_menu_rows`): Link an issue · New issue, and on
                    // an issue chip a separator + Unlink.
                    row(.linkIssue, Self.linkIssueLabel, icon: AppIcons.navSearch)
                    row(.newIssue, Self.newIssueLabel, icon: AppIcons.uiAdd)
                    if let identifier {
                        GlassDivider()
                            .padding(.vertical, 4)
                        row(.unlink, Self.unlinkLabel(identifier), icon: AppIcons.uiClose, destructive: true)
                    }
                }
            }
            .padding(.horizontal, GlassSheetTokens.headerHPadding - GlassMenuTokens.itemHPadding)
            .padding(.top, identifier == nil ? GlassSheetTokens.headerTopPadding : 0)
            .padding(.bottom, 16)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("thread-menu-sheet")
        }
    }

    private func row(
        _ action: Action, _ title: String, icon: String, destructive: Bool = false
    ) -> some View {
        Button {
            onPick(action)
        } label: {
            GlassMenuItemLabel(title, icon: icon, destructive: destructive)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("thread-menu-\(Self.testId(action))")
    }

    private static func testId(_ action: Action) -> String {
        switch action {
        case .linkIssue: "link-issue"
        case .newIssue: "new-issue"
        case .unlink: "unlink"
        case .closePr: "close-pr"
        }
    }
}

/// EXP-1334: a folded chip's subtree, or a zone's "+N" — THE Menu's sheet:
/// THE tree rows (`SessionRow` small + `TreeGuides`, the rows the Threads
/// list nests with). A subtree starts with the chip itself. The shown issue
/// page is the active row. A pick hands the row back; the host acts once
/// the sheet has dismissed.
struct ThreadFoldSheet: View {
    let fold: ThreadStripFold
    /// The synced issue behind an issue row (its status glyph and title).
    var issue: (String) -> IssueEntity? = { _ in nil }
    /// S-3: a row's status through the team's statuses; nil = no glyph.
    var issueStatus: (String) -> ResolvedIssueStatus? = { _ in nil }
    /// The issue page on screen (its row is the active one).
    var shownIssueId: String?
    let onPick: (ThreadStripFold.Row) -> Void

    var body: some View {
        GlassSheetChrome(title: fold.title) {
            let guides = TreeGuides.compute(depths: fold.rows.map(\.depth))
            VStack(spacing: 0) {
                ForEach(Array(fold.rows.enumerated()), id: \.element.id) { index, row in
                    Button {
                        onPick(row)
                    } label: {
                        label(row, guide: guides[index])
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("thread-fold-row-\(row.id)")
                }
            }
            .padding(.horizontal, GlassSheetTokens.headerHPadding - GlassMenuTokens.itemHPadding)
            .padding(.top, fold.title == nil ? GlassSheetTokens.headerTopPadding : 0)
            .padding(.bottom, 16)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("thread-fold-sheet")
        }
    }

    @ViewBuilder
    private func label(_ row: ThreadStripFold.Row, guide: TreeGuide) -> some View {
        switch row {
        case let .issue(issueId, identifier, _):
            SessionRow(
                identifier: identifier,
                title: issue(issueId)?.title ?? "",
                guide: guide,
                active: shownIssueId == issueId
            ) {
                if let status = issueStatus(issueId) {
                    AppIcon(status.iconName, size: AppIcon.Size.small)
                        .foregroundStyle(status.color)
                }
            }
        case let .session(_, name, markState, _):
            SessionRow(title: name, guide: guide, word: markState.word) {
                ThreadSessionChipMark(state: markState)
            }
        }
    }
}

/// EXP-1334: THE thread composer's "+" on a phone — composer-menu.json's
/// rows (`ComposerMenu.composerMenuLayout`, fixture-locked ×4) in THE Menu's
/// sheet, the same rows before and DURING a session: Implement issue (links
/// it into this thread, waking the dispatcher) · Message an agent › (NEW-7,
/// while the team has agents: the pick becomes the composer's subject, the
/// send goes into the caller's DM with it) · Add file or image | Effort ›
/// · Ultracode | MCP servers › · Computer use. Run action has no target in a
/// running thread (an action STARTS one), so it stays off. Toggles flip in
/// place (the host sends `sessions.configure`); a hand-off row closes the
/// sheet and the host opens its picker once it has dismissed.
struct ThreadComposerPlusMenu: View {
    let conditions: ComposerMenu.Conditions
    /// The session's agent (codex words Effort as Reasoning).
    let agent: String
    /// The Effort row's value, the MCP row's value.
    let effortValue: String?
    let mcpValue: String?
    /// NEW-7: Message an agent ›'s value (the picked agent's name).
    var agentValue: String? = nil
    @Binding var ultracode: Bool
    @Binding var computerUse: Bool
    var addFileEnabled = true
    /// An ENDED session: the Effort row and the toggles stand down (the
    /// server refuses `sessions.configure`); Implement issue and Add file
    /// stay, they never configure.
    var rowsDisabled = false
    let onPick: (ComposerMenu.RowId) -> Void

    private var entries: [ComposerMenu.Entry] {
        var out: [ComposerMenu.Entry] = []
        for entry in ComposerMenu.composerMenuLayout(conditions) {
            if entry == .separator, out.last == nil || out.last == .separator { continue }
            out.append(entry)
        }
        while out.last == .separator { out.removeLast() }
        return out
    }

    var body: some View {
        GlassSheetChrome(title: nil) {
            VStack(spacing: 0) {
                ForEach(Array(entries.enumerated()), id: \.offset) { _, entry in
                    switch entry {
                    case .separator:
                        GlassDivider()
                            .padding(.vertical, 4)
                    case let .row(row):
                        menuRow(row)
                    }
                }
            }
            .padding(.horizontal, GlassSheetTokens.headerHPadding)
            .padding(.top, GlassSheetTokens.headerTopPadding)
            .padding(.bottom, 16)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(ComposerMenu.menuTestId)
    }

    @ViewBuilder
    private func menuRow(_ row: ComposerMenu.Row) -> some View {
        let label = row.label(codex: agent == "codex")
        switch row.kind {
        case .toggle:
            GlassToggleRow(label, isOn: row.id == .ultracode ? $ultracode : $computerUse) {
                rowIcon(row)
            }
            .font(.body)
            .padding(.horizontal, 12)
            .frame(minHeight: 48)
            .disabled(rowsDisabled)
            .opacity(rowsDisabled ? 0.45 : 1)
            .accessibilityIdentifier(ComposerMenu.rowTestId(row.id))
        case .submenu, .item:
            let disabled = row.id == .effort && (rowsDisabled || agent == "claude" && ultracode)
                || row.id == .addFile && !addFileEnabled
            Button {
                onPick(row.id)
            } label: {
                HStack(spacing: 12) {
                    rowIcon(row)
                    Text(label)
                        .font(.body)
                        .foregroundStyle(.white)
                    Spacer(minLength: 8)
                    if let value = row.id == .effort ? effortValue
                        : row.id == .mcpServers ? mcpValue
                        : row.id == .messageAgent ? agentValue : nil {
                        Text(value)
                            .font(.subheadline)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .lineLimit(1)
                    }
                    if row.kind == .submenu {
                        AppIcon(AppIcons.uiChevronRight, size: 12)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                }
                .padding(.horizontal, 12)
                .frame(minHeight: 48)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(disabled)
            .opacity(disabled ? 0.45 : 1)
            .accessibilityIdentifier(ComposerMenu.rowTestId(row.id))
        }
    }

    private func rowIcon(_ row: ComposerMenu.Row) -> some View {
        AppIcon(ComposerPlusMenu.icon(row.icon), size: 18, weight: .medium)
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
            .frame(width: 22)
    }
}

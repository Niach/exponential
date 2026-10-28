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
            let status = IssueStatus.from(issue(issueId)?.status)
            SessionRow(
                identifier: identifier,
                title: issue(issueId)?.title ?? "",
                guide: guide,
                active: shownIssueId == issueId
            ) {
                AppIcon(status.iconName, size: AppIcon.Size.small)
                    .foregroundStyle(status.color)
            }
        case let .session(_, name, markState, _):
            SessionRow(title: name, guide: guide, word: markState.word) {
                ThreadSessionChipMark(state: markState)
            }
        }
    }
}

/// EXP-1334 / S-3: THE "+" of a session's composer and an agent chat's on a
/// phone (composer-menu.json `surface` session | agent,
/// `ComposerMenu.composerMenuLayout`, fixture-locked ×4) — the composer's
/// ONLY tool: Photo or file (closes the sheet, the host floats THE Photo |
/// File GlassMenu) · Issue › (THE issue search list as a pushed page; a pick
/// inserts `#IDENT` at the caret) · Model › (THE model picker's rows as a
/// pushed page, while the agent switches models). A running session never
/// moves device or login here, and the work picks are the launch's.
struct ThreadComposerPlusMenu: View {
    let conditions: ComposerMenu.Conditions
    /// A session's "+" or an agent chat's (composer-menu.json `surface`).
    var surface: ComposerMenu.Surface = .session
    /// The session's agent (its models).
    let agent: String
    /// The Model row's value (the picked model) and its pick.
    var modelValue: String?
    var onPickModel: (String) -> Void = { _ in }
    /// The team whose issues the Issue page searches; nil = no page.
    var teamId: String?
    /// The Issue page's pick (the host inserts `#IDENT` at the caret).
    var onPickIssue: (IssueEntity) -> Void = { _ in }
    var addFileEnabled = true
    /// A hand-off row (Photo or file): the host closes the sheet and opens
    /// what it names.
    let onPick: (ComposerMenu.RowId) -> Void

    /// The pushed page, nil = the rows.
    @State private var page: ComposerMenu.RowId?
    @Environment(\.motion) private var motion

    private var entries: [ComposerMenu.Entry] {
        ComposerMenu.composerMenuLayout(conditions, surface: surface)
    }

    var body: some View {
        Group {
            if let page {
                ComposerMenuPage(id: page, onBack: { self.page = nil }) {
                    pageContent(page)
                }
                .transition(.move(edge: .trailing))
            } else {
                rows
                    .transition(.move(edge: .leading))
            }
        }
        .animation(motion.standard, value: page)
    }

    private var rows: some View {
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
    private func pageContent(_ id: ComposerMenu.RowId) -> some View {
        switch id {
        case .model:
            GlassPickerContent(
                items: ModelPicker<EmptyView>.items(agent: agent),
                mode: .single,
                value: modelValue.map { [$0] } ?? [],
                onChange: { picked in picked.first.map(onPickModel) }
            )
        default:
            if let teamId {
                ThreadIssueSearchList(teamId: teamId, onPick: onPickIssue)
            }
        }
    }

    @ViewBuilder
    private func menuRow(_ row: ComposerMenu.Row) -> some View {
        // A session's / agent chat's "+" never holds a toggle (those are
        // the launch's work picks).
        if row.kind != .toggle {
            let disabled = row.id == .addFile && !addFileEnabled
            Button {
                if row.kind == .submenu {
                    page = row.id
                } else {
                    onPick(row.id)
                }
            } label: {
                ComposerMenuRowLabel(
                    row: row,
                    label: row.label(codex: agent == "codex"),
                    value: row.id == .model ? modelValue.map(LaunchVocabulary.modelLabel) : nil
                )
            }
            .buttonStyle(.plain)
            .disabled(disabled)
            .opacity(disabled ? 0.45 : 1)
            .accessibilityIdentifier(ComposerMenu.rowTestId(row.id))
        }
    }
}

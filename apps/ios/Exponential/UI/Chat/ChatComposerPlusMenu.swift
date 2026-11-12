import ExpCore
import ExpUI
import SwiftUI

/// EXP-1334 / S-3: THE "+" of a session's composer and an agent chat's on a
/// phone (composer-menu.json `surface` session | agent,
/// `ComposerMenu.composerMenuLayout`, fixture-locked ×4) — the composer's
/// ONLY tool: Photo or file (closes the sheet, the host floats THE Photo |
/// File GlassMenu) · Issue › (THE issue search list as a pushed page; a pick
/// inserts `#IDENT` at the caret) · Account › (#EXP-1535: THE account
/// picker's rows; a pick = the usage wall's session account switch) · Model ›
/// (THE model picker's rows as a pushed page, while the agent switches
/// models). A running session never moves device here, and the work picks
/// are the launch's.
struct ChatComposerPlusMenu: View {
    let conditions: ComposerMenu.Conditions
    /// A session's "+" or an agent chat's (composer-menu.json `surface`).
    var surface: ComposerMenu.Surface = .session
    /// The session's agent (its models).
    let agent: String
    /// The Model row's value (the picked model) and its pick.
    var modelValue: String?
    var onPickModel: (String) -> Void = { _ in }
    /// #EXP-1535: the Account page (the session account switch); nil = no
    /// Account row (`conditions.account` is false then).
    var account: SessionComposerFooter.AccountMenu?
    /// #FEED-204: an app editor's Device page; nil = no Device row.
    var device: SessionComposerFooter.DeviceMenu?
    /// #EXP-1793: an agent chat's MCP servers page; nil = no row.
    var mcp: SessionComposerFooter.McpMenu?
    /// The team whose issues the Issue page searches; nil = no page.
    var teamId: String?
    /// The Issue page's pick (the host inserts `#IDENT` at the caret).
    var onPickIssue: (IssueEntity) -> Void = { _ in }
    /// #EXP-1434: the Prompts page's rows (the team's prompts, the `/`
    /// menu's order) and its pick (the host inserts the body at the caret).
    var prompts: [SlashPrompt] = []
    var onPickPrompt: (SlashPrompt) -> Void = { _ in }
    /// #EXP-1715: the Teams page (the viewer's Assistant chat only).
    var teams: ComposerTeamsBinding?
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
        case .device:
            // THE device picker's rows (`DevicePicker.items`).
            if let device {
                GlassPickerContent(
                    items: DevicePicker<EmptyView>.items(device.devices),
                    mode: .single,
                    value: device.current.map { [$0] } ?? [],
                    onChange: { picked in picked.first.map(device.onPick) }
                )
            }
        case .model:
            GlassPickerContent(
                items: ModelPicker<EmptyView>.items(agent: agent),
                mode: .single,
                value: modelValue.map { [$0] } ?? [],
                onChange: { picked in picked.first.map(onPickModel) }
            )
        case .account:
            // #EXP-1535: THE account picker's rows (brand mark, email, the
            // health hint and the EXP-992 limit bars), the current login
            // checked; a pick = the session account switch.
            if let account {
                AccountPickerContent(
                    options: account.options,
                    value: account.current?.key,
                    onChange: account.onPick,
                    mark: { AgentBrandMark.image($0) },
                    disabledKeys: account.disabledKeys
                )
            }
        case .prompts:
            ComposerPromptsList(prompts: prompts, onPick: onPickPrompt)
        case .teams:
            if let teams { TeamsPickPage(binding: teams) }
        case .mcpServers:
            // #EXP-1793: THE launch composer's MCP server picker rows.
            if let mcp {
                GlassPickerContent(
                    items: McpServerPicker<EmptyView>.items(mcp.servers),
                    mode: .multi,
                    value: Set(mcp.picked),
                    onChange: { picked in
                        // Keep the team's order, not the set's.
                        mcp.onChange(mcp.servers.map(\.id).filter { picked.contains($0) })
                    },
                    emptyText: "No MCP servers"
                )
                .pickerCheckMarks()
            }
        default:
            if let teamId {
                ChatIssueSearchList(teamId: teamId, onPick: onPickIssue)
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
                    value: value(row.id)
                )
            }
            .buttonStyle(.plain)
            .disabled(disabled)
            .opacity(disabled ? 0.45 : 1)
            .accessibilityIdentifier(ComposerMenu.rowTestId(row.id))
        }
    }

    /// A submenu row's current value: the picked label.
    private func value(_ id: ComposerMenu.RowId) -> String? {
        switch id {
        case .device: device?.value
        case .model: modelValue.map(LaunchVocabulary.modelLabel)
        case .account: account?.current?.email
        case .teams: teams?.value
        case .mcpServers: mcp.flatMap { McpServers.pickedValue($0.picked) }
        default: nil
        }
    }
}

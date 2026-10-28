import ExpCore
import ExpUI
import SwiftUI

/// EXP-1249: the launch composer's ONE "+" menu on a phone — a bottom sheet
/// of the same rows web's dropdown draws (`ComposerMenu.composerMenuLayout`,
/// fixture-locked ×4): Implement issue ›, Message an agent › (NEW-3, while
/// the team has agents), Add file or image | Effort ›, Ultracode | MCP
/// servers ›, Computer use (NEW-2: prompts live in the `/` menu, never
/// here). Implement issue › and Message an agent › PUSH their page inside
/// this sheet, THE search list (`AgentIssuePickerPage`, `AgentPickerPage`)
/// with a back row; the other submenu rows close
/// this sheet and hand off to their picker (a sheet cannot present while its
/// sibling is still animating away, so the host opens the picker on this
/// sheet's dismiss); a toggle flips in place.
struct ComposerPlusMenu: View {
    let model: AgentComposerModel
    /// The team's MCP servers (empty = the row hides).
    let mcpServers: [McpServerRow]
    /// A hand-off row was picked: the host closes the sheet and opens what
    /// the row names.
    let onPick: (ComposerMenu.RowId) -> Void
    /// A page finished (an agent picked, Implement N issues): the host
    /// closes the sheet.
    let onClose: () -> Void

    /// The pushed page (Implement issue or Message an agent), nil = the rows.
    @State private var page: ComposerMenu.RowId?
    @Environment(\.motion) private var motion

    private var launch: LaunchOptionsState { model.launch }

    /// `subagentModel` stays false: `sessions.send` carries no subagent
    /// model (EXP-1291), so the Subagents row has nothing to set.
    private var conditions: ComposerMenu.Conditions {
        ComposerMenu.Conditions(
            temporary: model.selectedAgent == nil,
            subagentModel: false,
            ultracode: launch.agent == "claude",
            mcp: !mcpServers.isEmpty,
            computerUse: model.device?.canToggleComputerUse == true
        )
    }

    var body: some View {
        Group {
            if let page {
                pageView(page)
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
                ForEach(Array(ComposerMenu.composerMenuLayout(conditions, surface: .launch).enumerated()), id: \.offset) { _, entry in
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
            .padding(.bottom, 16)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(ComposerMenu.menuTestId)
    }

    @ViewBuilder
    private func menuRow(_ row: ComposerMenu.Row) -> some View {
        let label = row.label(codex: launch.agent == "codex")
        switch row.kind {
        case .toggle:
            GlassToggleRow(label, isOn: toggleBinding(row.id)) {
                rowIcon(row)
            }
            .font(.body)
            .padding(.horizontal, 12)
            .frame(minHeight: 48)
            .accessibilityIdentifier(ComposerMenu.rowTestId(row.id))
        case .submenu, .item:
            let disabled = row.id == .effort && launch.agent == "claude" && launch.ultracode
                || row.id == .addFile && (model.attachFull || model.sending)
            Button {
                if row.id == .implementIssue {
                    page = row.id
                } else {
                    onPick(row.id)
                }
            } label: {
                HStack(spacing: 12) {
                    rowLabel(row, label: label)
                    Spacer(minLength: 8)
                    if let value = value(row.id) {
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

    /// A pushed page: a back row naming the submenu over THE search list,
    /// at the large detent (the list scrolls under its input).
    private func pageView(_ id: ComposerMenu.RowId) -> some View {
        VStack(spacing: 0) {
            Button {
                page = nil
            } label: {
                HStack(spacing: 8) {
                    AppIcon(AppIcons.uiChevronLeft, size: 14)
                    Text(ComposerMenu.allRows.first { $0.id == id }?.label ?? "")
                        .font(.headline)
                    Spacer(minLength: 0)
                }
                .foregroundStyle(.white)
                .padding(.horizontal, GlassSheetTokens.headerHPadding)
                .padding(.top, GlassSheetTokens.headerTopPadding)
                .padding(.bottom, GlassSheetTokens.headerBottomPadding)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Back")
            .accessibilityIdentifier("agent-composer-submenu-back")
            AgentIssuePickerPage(model: model, onClose: onClose)
        }
        .frame(maxHeight: .infinity, alignment: .top)
        .presentationDetents([.large])
        .presentationBackground(GlassSheetTokens.background)
        .presentationCornerRadius(GlassSheetTokens.cornerRadius)
        .presentationDragIndicator(.visible)
        .presentationContentInteraction(.scrolls)
    }

    private func rowIcon(_ row: ComposerMenu.Row) -> some View {
        AppIcon(Self.icon(row.icon), size: 18, weight: .medium)
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
            .frame(width: 22)
    }

    private func rowLabel(_ row: ComposerMenu.Row, label: String) -> some View {
        HStack(spacing: 12) {
            rowIcon(row)
            Text(label)
                .font(.body)
                .foregroundStyle(.white)
        }
    }

    /// A submenu row's current value: the effort, the number of MCP servers
    /// picked.
    private func value(_ id: ComposerMenu.RowId) -> String? {
        switch id {
        case .effort:
            launch.effort == LaunchVocabulary.cliDefault ? "CLI default" : LaunchVocabulary.effortLabel(launch.effort)
        case .mcpServers:
            McpServers.pickedValue(launch.mcpServerIds)
        default:
            nil
        }
    }

    private func toggleBinding(_ id: ComposerMenu.RowId) -> Binding<Bool> {
        let launch = launch
        switch id {
        case .ultracode:
            return Binding(get: { launch.ultracode }, set: { launch.ultracode = $0 })
        default:
            // Untouched = the machine's current default; only a flip lands
            // in `launch.computerUse` (and so on the wire).
            let deviceDefault = model.device?.computerUseDefault ?? false
            return Binding(
                get: { launch.computerUse ?? deviceDefault },
                set: { launch.computerUse = $0 }
            )
        }
    }

    /// The fixture's concept ids → this client's generated icon names.
    static func icon(_ concept: String) -> String {
        switch concept {
        case "editor-issue-ref": AppIcons.editorIssueRef
        case "action-run": AppIcons.actionRun
        case "ui-attach": AppIcons.uiAttach
        case "ui-estimate": AppIcons.uiEstimate
        case "coding-subagent": AppIcons.codingSubagent
        case "action-default": AppIcons.actionDefault
        case "ui-mcp": AppIcons.uiMcp
        case "nav-computer": AppIcons.navComputer
        case "settings-agents": AppIcons.settingsAgents
        default: AppIcons.uiAdd
        }
    }
}

/// NEW-3: Message an agent › — THE Picker rows of the team's agents (the
/// agent's icon in its colour + its name, single pick; web `PickerMenuRows`,
/// no search). The pick becomes the composer's subject as ONE agent chip and
/// closes the menu.
struct AgentPickerPage: View {
    let model: AgentComposerModel
    let onClose: () -> Void

    var body: some View {
        GlassPickerContent(
            items: model.agents.map { agent in
                PickerItem(
                    value: agent.id,
                    label: agent.name,
                    icon: BoardTypeDisplay.iconName(for: agent.icon) ?? SessionTree.ListCopy.agentDefaultIcon,
                    color: Color(hex: agent.color ?? SessionTree.ListCopy.agentDefaultColor)
                )
            },
            mode: .single,
            value: Set([model.agentId].compactMap { $0 }),
            onChange: { picked in
                guard let id = picked.first else { return }
                model.pickAgent(id)
                onClose()
            }
        )
        .accessibilityIdentifier("agent-composer-agents-picker")
    }
}

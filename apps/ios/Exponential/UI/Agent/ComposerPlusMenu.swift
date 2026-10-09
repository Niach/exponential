import ExpCore
import ExpUI
import SwiftUI

/// EXP-1249: the launch composer's ONE "+" menu on a phone — a bottom sheet
/// of the same rows web's dropdown draws (`ComposerMenu.composerMenuLayout`,
/// fixture `composer-menu.json` ×4): Implement issue ›, Run action ›, Add
/// file or image | Effort ›, Subagents ›, Ultracode | MCP servers ›, Computer
/// use. A submenu row closes this sheet and hands off to its picker (a sheet
/// cannot present while its sibling is still animating away, so the host
/// opens the picker on this sheet's dismiss); a toggle flips in place.
struct ComposerPlusMenu: View {
    let model: AgentComposerModel
    /// The team's MCP servers (empty = the row hides).
    let mcpServers: [McpServerRow]
    /// A submenu / item row was picked: the host closes the sheet and opens
    /// what the row names.
    let onPick: (ComposerMenu.RowId) -> Void

    private var launch: LaunchOptionsState { model.launch }

    /// A real (non-builtin) action owns its MCP list (FEED-73).
    private var actionOwnsMcp: Bool {
        guard let action = model.selectedAction else { return false }
        return !action.id.hasPrefix("builtin:")
    }

    private var conditions: ComposerMenu.Conditions {
        ComposerMenu.Conditions(
            subagentModel: LaunchVocabulary.supportsSubagentModel(launch.agent),
            ultracode: launch.agent == "claude",
            mcp: !mcpServers.isEmpty && !actionOwnsMcp,
            computerUse: model.device?.canToggleComputerUse == true
        )
    }

    var body: some View {
        GlassSheetChrome(title: nil) {
            VStack(spacing: 0) {
                ForEach(Array(ComposerMenu.composerMenuLayout(conditions).enumerated()), id: \.offset) { _, entry in
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
                onPick(row.id)
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

    /// A submenu row's current value: the effort, the subagent model, the
    /// number of MCP servers picked.
    private func value(_ id: ComposerMenu.RowId) -> String? {
        switch id {
        case .effort:
            launch.effort == LaunchVocabulary.cliDefault ? nil : LaunchVocabulary.effortLabel(launch.effort)
        case .subagents:
            launch.subagentModel == LaunchVocabulary.cliDefault
                ? nil
                : LaunchVocabulary.subagentModelLabel(launch.subagentModel)
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
        default: AppIcons.uiAdd
        }
    }
}

/// EXP-1249: the composer's quiet suggestion rows UNDER the options line —
/// muted 13pt text behind a faint `action-default` glyph, no pill, no border;
/// `ComposerMenu.suggestionCount` of them, only with no subject and an empty
/// field.
struct ComposerSuggestionRows: View {
    let suggestions: [String]
    let onPick: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(suggestions.prefix(ComposerMenu.suggestionCount).enumerated()), id: \.offset) { _, text in
                Button { onPick(text) } label: {
                    HStack(spacing: 10) {
                        AppIcon(ComposerPlusMenu.icon(ComposerMenu.suggestionIcon), size: 13)
                            .foregroundStyle(.white.opacity(0.28))
                        Text(text)
                            .font(.system(size: 13))
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .lineLimit(1)
                            .truncationMode(.tail)
                        Spacer(minLength: 0)
                    }
                    .padding(.vertical, 6)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier(ComposerMenu.suggestionTestId)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

import ExpCore
import ExpUI
import SwiftUI

/// EXP-1249 / S-3: the launch composer's ONE "+" menu on a phone — a bottom
/// sheet of the same rows web's dropdown draws
/// (`ComposerMenu.composerMenuLayout(_, surface: .launch)`, fixture-locked
/// ×4): Photo or file · Issue › | Device › · Account › · Model › ·
/// Repository › | Effort › · Subagents › · Ultracode | MCP servers › ·
/// Computer use. The "+" is the composer's ONLY tool (no options line under
/// the card). The runner submenus (Device, Account, Model, Repository) and
/// Issue PUSH their page inside this sheet over THE existing pickers' rows,
/// a back row on top; the value column names the pick. While the agent row
/// (composer-home.json) picks an agent instead of Temporary session, the
/// runner and work rows hide (`temporary` false): a message to an agent runs
/// on that agent's definition, and Issue inserts `#IDENT` at the caret
/// (there is no implement subject on a message). Effort and MCP servers hand
/// off to their picker (a sheet cannot present while its sibling is still
/// animating away, so the host opens it on this sheet's dismiss); a toggle
/// flips in place.
struct ComposerPlusMenu: View {
    let model: AgentComposerModel
    /// The team's MCP servers (empty = the row hides).
    let mcpServers: [McpServerRow]
    /// A hand-off row was picked: the host closes the sheet and opens what
    /// the row names.
    let onPick: (ComposerMenu.RowId) -> Void
    /// A page finished (Implement N issues, an `#IDENT` inserted): the host
    /// closes the sheet.
    let onClose: () -> Void

    /// The pushed page, nil = the rows.
    @State private var page: ComposerMenu.RowId?
    @Environment(\.motion) private var motion

    private var launch: LaunchOptionsState { model.launch }

    /// `subagentModel` stays false: `sessions.send` carries no subagent
    /// model (EXP-1291), so the Subagents row has nothing to set.
    private var conditions: ComposerMenu.Conditions {
        let temporary = model.agentId == nil
        return ComposerMenu.Conditions(
            temporary: temporary,
            device: temporary && !model.candidateDevices.isEmpty,
            account: temporary && model.device != nil,
            model: temporary,
            repositories: model.repos.count > 1,
            subagentModel: false,
            ultracode: launch.agent == "claude",
            mcp: !mcpServers.isEmpty,
            computerUse: model.device?.canToggleComputerUse == true
        )
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
                ComposerMenuRowIcon(concept: row.icon)
            }
            .font(.body)
            .padding(.horizontal, 12)
            .frame(minHeight: 48)
            .accessibilityIdentifier(ComposerMenu.rowTestId(row.id))
        case .submenu, .item:
            let disabled = row.id == .effort && launch.agent == "claude" && launch.ultracode
                || row.id == .addFile && (model.attachFull || model.sending)
            Button {
                if Self.pushes(row.id) {
                    page = row.id
                } else {
                    onPick(row.id)
                }
            } label: {
                ComposerMenuRowLabel(row: row, label: label, value: value(row.id))
            }
            .buttonStyle(.plain)
            .disabled(disabled)
            .opacity(disabled ? 0.45 : 1)
            .accessibilityIdentifier(ComposerMenu.rowTestId(row.id))
        }
    }

    /// The rows whose submenu is a page of this sheet (the rest hand off).
    static func pushes(_ id: ComposerMenu.RowId) -> Bool {
        switch id {
        case .implementIssue, .device, .account, .model, .repository: true
        default: false
        }
    }

    // MARK: - Pages

    @ViewBuilder
    private func pageContent(_ id: ComposerMenu.RowId) -> some View {
        switch id {
        case .device:
            // THE device picker's rows (`DevicePicker.items`).
            GlassPickerContent(
                items: DevicePicker<EmptyView>.items(model.candidateDevices.map(DevicePickerDevice.init)),
                mode: .single,
                value: model.device.map { [$0.deviceId] } ?? [],
                onChange: { picked in picked.first.map { model.selectDevice($0) } }
            )
            .accessibilityIdentifier("agent-device-pill")
        case .account:
            // EXP-872: THE account picker's rows (brand mark, email, the
            // EXP-992 limit bars); picking a login picks its agent too.
            let options = launch.accountOptions(on: model.device)
            AccountPickerContent(
                options: options,
                value: (launch.selectedAccount(in: options) ?? options.first)?.key,
                onChange: { key in
                    guard let picked = options.first(where: { $0.key == key }) else { return }
                    model.selectAccount(picked)
                },
                mark: { AgentBrandMark.image($0) }
            )
            .accessibilityIdentifier("start-coding-agent-picker")
        case .model:
            GlassPickerContent(
                items: ModelPicker<EmptyView>.items(agent: launch.agent),
                mode: .single,
                value: [launch.model],
                onChange: { picked in picked.first.map { launch.model = $0 } }
            )
        case .repository:
            let rows = model.repos.map { RepositoryPickerRow(id: $0.id, fullName: $0.fullName, isPrivate: $0.isPrivate) }
            let byId = Dictionary(rows.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
            GlassPickerContent(
                items: RepositoryPickerRules.items(rows, noneLabel: nil),
                mode: .single,
                value: [model.chatRepoId],
                onChange: { picked in picked.first.map { model.chatRepoId = $0 } },
                search: RepositoryPickerRules.showsSearch(count: rows.count),
                searchPlaceholder: "Search repositories…",
                renderItem: { item in byId[item.value].map { AnyView(RepositoryPickerRowBody(row: $0)) } }
            )
        default:
            if model.agentId != nil, let teamId = model.teamId {
                // A message to an agent has no implement subject: THE issue
                // search list, a pick inserts `#IDENT` at the caret.
                ThreadIssueSearchList(teamId: teamId) { issue in
                    if let identifier = issue.identifier {
                        model.draftEditor.insertTextAtCaret("#\(identifier) ")
                    }
                    onClose()
                }
            } else {
                AgentIssuePickerPage(model: model, onClose: onClose)
            }
        }
    }

    /// A submenu row's current value: the picked label.
    private func value(_ id: ComposerMenu.RowId) -> String? {
        switch id {
        case .device:
            model.device.map(LaunchVocabulary.deviceName)
        case .account:
            {
                let options = launch.accountOptions(on: model.device)
                return (launch.selectedAccount(in: options) ?? options.first)?.email
            }()
        case .model:
            LaunchVocabulary.modelLabel(launch.model)
        case .repository:
            model.repos.first { $0.id == model.chatRepoId }?.fullName
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
        AppIcons.conceptOrName(concept) ?? AppIcons.uiAdd
    }
}

/// One "+" row's glyph: the fixture concept, muted, in a 22pt column.
struct ComposerMenuRowIcon: View {
    let concept: String

    var body: some View {
        AppIcon(ComposerPlusMenu.icon(concept), size: 18, weight: .medium)
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
            .frame(width: 22)
    }
}

/// One "+" item / submenu row: glyph · label · the value column (the picked
/// label) · a chevron on a submenu.
struct ComposerMenuRowLabel: View {
    let row: ComposerMenu.Row
    let label: String
    var value: String?

    var body: some View {
        HStack(spacing: 12) {
            ComposerMenuRowIcon(concept: row.icon)
            Text(label)
                .font(.body)
                .foregroundStyle(.white)
            Spacer(minLength: 8)
            if let value, !value.isEmpty {
                Text(value)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                    .truncationMode(.middle)
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
}

/// A "+" submenu PUSHED as a page of the sheet (composer-menu.json: phones
/// push submenus): a back row naming the submenu over its rows, at the large
/// detent (a list scrolls under its input).
struct ComposerMenuPage<Content: View>: View {
    let id: ComposerMenu.RowId
    let onBack: () -> Void
    @ViewBuilder let content: () -> Content

    var body: some View {
        VStack(spacing: 0) {
            Button(action: onBack) {
                HStack(spacing: 8) {
                    AppIcon(AppIcons.uiChevronLeft, size: 14)
                    Text(ComposerMenu.row(id).label)
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
            content()
        }
        .frame(maxHeight: .infinity, alignment: .top)
        .presentationDetents([.large])
        .presentationBackground(GlassSheetTokens.background)
        .presentationCornerRadius(GlassSheetTokens.cornerRadius)
        .presentationDragIndicator(.visible)
        .presentationContentInteraction(.scrolls)
    }
}

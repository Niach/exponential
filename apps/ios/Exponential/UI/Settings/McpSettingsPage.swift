import ExpCore
import ExpUI
import GRDB
import SwiftUI
import UIKit

// #VAPP-185 (fixtures/settings-mcp.json): Settings › MCP, ONE page ×4 (web
// `mcp-settings-page.tsx`): the Overview line · Servers (owner 2026-11-12:
// ONE group, the built-ins Exponential + Team apps first, then the team's
// servers, EVERY row expandable to its tools) · Access (+ Activity) ·
// Clients (Mine | Everyone). Phones have no add/edit: the team rows carry
// the member's own Connect (the web page in the browser, `?connect=<id>`) /
// Disconnect.

/// The page the Team row `mcp` opens.
struct McpSettingsPage: View {
    let teamId: String
    /// Styleguide capture only (`sg_settings-mcp`): the page over sample rows,
    /// no fetch.
    let specimen: Specimen?

    struct Specimen {
        var instanceUrl: String
        var isOwner: Bool
        var servers: [McpServerRow]
        var clients: [McpClientRow]
        var access: McpAppAccess
        /// An open team server's tools (no `mcpServers.tools` fetch).
        var serverTools: [String: [McpToolRow]] = [:]
    }

    /// An open team server's `mcpServers.tools` answer.
    enum ToolsLoad: Equatable {
        case loading
        case loaded([McpToolRow])
        case failed(String)
    }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @Environment(\.openURL) private var openURL

    @State private var team: TeamEntity?
    @State private var isOwner = false
    @State private var servers: [McpServerRow]?
    @State private var pendingServer: String?
    @State private var scope: McpSettings.Scope = .mine
    @State private var clients: McpClientsList?
    @State private var revokeTarget: McpClientRow?
    @State private var showRevoke = false
    /// #EXP-1788: a revoke is in flight (every Revoke is off, the IDE's `revoking`).
    @State private var revoking = false
    @State private var access: McpAppAccess?
    @State private var builtinQuery = ""
    /// The open Servers rows (`builtin:<id>` or a team server's id); closed
    /// by default, several may be open.
    @State private var openRows: Set<String> = []
    /// The team servers' tools, fetched on first open (kept, so reopening a
    /// row never refetches; the server caches a member's answer 5 minutes).
    @State private var serverTools: [String: ToolsLoad] = [:]

    init(teamId: String, specimen: Specimen? = nil) {
        self.teamId = teamId
        self.specimen = specimen
        if let specimen {
            _isOwner = State(initialValue: specimen.isOwner)
            _servers = State(initialValue: specimen.servers)
            _clients = State(initialValue: McpClientsList(canSeeEveryone: specimen.isOwner, clients: specimen.clients))
            _access = State(initialValue: specimen.access)
        }
    }

    private var api: McpServersApi { McpServersApi(trpc: deps.trpc) }

    private var instanceUrl: String? {
        if let specimen { return specimen.instanceUrl }
        return deps.auth.accounts.first { $0.id == accountId }?.instanceUrl
    }

    var body: some View {
        TeamSettingsPageChrome(section: .mcp) {
            VStack(alignment: .leading, spacing: 24) {
                overviewLine
                serversSection
                accessSection
                clientsSection
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(McpSettings.TestId.page)
        }
        .task(id: "\(accountId)/\(teamId)") { await loadTeam() }
        .task(id: "\(accountId)/\(teamId)/servers") { await loadServers() }
        .task(id: "\(accountId)/\(teamId)/\(scope.rawValue)") { await loadClients() }
        .task(id: "\(accountId)/\(teamId)/access") { await loadAccess() }
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(isPresented: $showRevoke) {
                    GlassAlert(
                        prompt: Prompts.RevokeMcpClient.copy(client: revokeTarget?.name ?? "this client"),
                        handlers: ["revoke": { Task { await revoke() } }]
                    )
                }
        }
    }

    // MARK: - Overview

    /// One muted line above the first group, no label (`overview`).
    private var overviewLine: some View {
        Text(McpSettings.overview)
            .font(.subheadline)
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityIdentifier(McpSettings.Section.overview.testId)
    }

    // MARK: - Servers

    /// ONE group (`serverOrder`): the built-ins first, then the team's
    /// servers (no team server = the built-ins only). FLAT rows under the
    /// band (web, desktop): no filled box.
    private var serversSection: some View {
        let hasAppTools = !(access?.apps.isEmpty ?? true)
        return section(.servers, boxed: false) {
            VStack(spacing: 0) {
                ForEach(McpSettings.serverOrder(appTools: hasAppTools, servers: servers ?? [])) { entry in
                    switch entry {
                    case .builtin(let builtin):
                        builtinRow(builtin)
                    case .server(let server):
                        serverRow(server)
                    }
                }
                if servers == nil {
                    SkeletonRows(rows: 2, rowHeight: McpSettingsPage.serverRowHeight)
                }
            }
        }
    }

    private func isOpen(_ key: String) -> Binding<Bool> {
        Binding(
            get: { openRows.contains(key) },
            set: { open in
                if open { openRows.insert(key) } else { openRows.remove(key) }
            }
        )
    }

    private func builtinUrl(_ builtin: McpSettings.Builtin) -> String {
        guard let instanceUrl else { return builtin.path }
        return McpSettings.builtinEndpoint(instanceUrl: instanceUrl, builtin)
    }

    /// A built-in server (`builtins`): the Exponential mark, the name, the
    /// Built-in pill, its URL in mono; its ONE action copies the URL. No
    /// menu, no edit.
    private func builtinRow(_ builtin: McpSettings.Builtin) -> some View {
        let url = builtinUrl(builtin)
        return VStack(spacing: 0) {
            HStack(spacing: 12) {
                McpRowDisclosure(
                    isExpanded: isOpen(builtin.key),
                    name: builtin.name,
                    pill: McpSettings.builtinPill,
                    caption: url,
                    mono: true
                ) {
                    ExpLogoMark(size: 16, color: .white.opacity(TextOpacity.secondary))
                }
                GlassPill(McpSettings.copy, mode: .action {
                    UIPasteboard.general.string = url
                    toaster.success(McpSettings.copied)
                })
                .fixedSize()
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .flatRow()
            .accessibilityIdentifier(builtin == .exponential ? McpSettings.TestId.builtin : McpSettings.TestId.appsServer)
            if openRows.contains(builtin.key) {
                VStack(spacing: 0) {
                    switch builtin {
                    case .exponential:
                        builtinTools
                    case .apps:
                        appTools
                    }
                }
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier(McpSettings.TestId.serverTools)
            }
        }
    }

    private func serverRow(_ server: McpServerRow) -> some View {
        let caption = [McpServers.location(server), McpSettings.runsOnCaption(server.deviceLabel), McpSettings.usedByCaption(server.usedBy)]
            .compactMap { $0 }
            .joined(separator: " · ")
        let connected = server.connection.status == "connected"
        return VStack(spacing: 0) {
            HStack(spacing: 12) {
                McpRowDisclosure(
                    isExpanded: isOpen(server.id),
                    name: server.name,
                    pill: server.enabledByDefault ? "Default" : nil,
                    caption: caption
                ) {
                    AppIcon(AppIcons.uiMcp, size: AppIcon.Size.small)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
                connectionAction(server)
                if connected {
                    GlassMenu {
                        GlassMenuItem("Disconnect", icon: AppIcons.uiClear, destructive: true) {
                            Task { await disconnect(server) }
                        }
                    } label: {
                        GhostIconLabel(AppIcons.uiMore)
                            .accessibilityLabel("Server menu for \(server.name)")
                    }
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .flatRow()
            .accessibilityIdentifier("mcp-server-row")
            if openRows.contains(server.id) {
                serverToolsList(server)
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier(McpSettings.TestId.serverTools)
            }
        }
    }

    // MARK: - Tools under an open row

    /// The open Exponential row: its FIRST line is the one filter field,
    /// then one sub-label per contract tool group (#EXP-1657); every band
    /// filtered away = the one empty state under the field.
    @ViewBuilder
    private var builtinTools: some View {
        let bands = McpSettings.builtinToolBands(query: builtinQuery)
        GlassSheetSearchField(
            placeholder: McpSettings.builtinFilterPlaceholder,
            text: $builtinQuery,
            accessibilityIdentifier: "mcp-builtin-tools-filter"
        )
        .padding(.leading, McpSettingsPage.toolInset)
        .padding(.trailing, 12)
        .padding(.vertical, 6)
        if bands.isEmpty {
            EmptyState(.builtinToolsNoMatch)
        } else {
            LazyVStack(spacing: 0) {
                ForEach(bands) { band in
                    toolBand(band.label, id: band.id) {
                        ForEach(band.tools) { tool in
                            toolRow(title: tool.title, blurb: tool.blurb, mark: true)
                        }
                    }
                }
            }
        }
    }

    /// The open Team apps row: one sub-label per app (alphabetical), its
    /// exposed tools in package order.
    private var appTools: some View {
        ForEach(access?.apps ?? []) { app in
            toolBand(app.name, id: app.slug) {
                ForEach(app.tools) { tool in
                    toolRow(title: tool.title, blurb: tool.blurb)
                }
            }
        }
    }

    /// An open team server (`toolsStateCases`): a FLAT list fetched on first
    /// open, or the muted line saying why there is none.
    @ViewBuilder
    private func serverToolsList(_ server: McpServerRow) -> some View {
        switch McpSettings.toolsState(transport: server.transport, status: server.connection.status) {
        case .connect:
            toolsLine(McpSettings.toolsConnect)
        case .stdio:
            toolsLine(McpSettings.toolsStdio)
        case .list:
            Group {
                switch serverTools[server.id] ?? .loading {
                case .loading:
                    SkeletonRows(rows: 2, rowHeight: McpSettingsPage.toolRowHeight, dot: false, dividers: false)
                        .padding(.leading, McpSettingsPage.toolInset)
                        .padding(.trailing, 12)
                case .failed(let reason):
                    toolsLine(McpSettings.toolsFailedLine(reason)) {
                        Button(McpSettings.retry) {
                            Task { await loadTools(server) }
                        }
                        .buttonStyle(.plain)
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white)
                        .fixedSize()
                        .accessibilityIdentifier("mcp-server-tools-retry")
                    }
                case .loaded(let tools):
                    if tools.isEmpty {
                        toolsLine(McpSettings.toolsNone)
                    } else {
                        VStack(spacing: 0) {
                            ForEach(tools) { tool in
                                toolRow(title: tool.title, blurb: tool.blurb)
                            }
                        }
                    }
                }
            }
            .task(id: server.id) {
                if serverTools[server.id] == nil { await loadTools(server) }
            }
        }
    }

    /// A sub-label inside an open row (a tool group, an app) over its tools.
    private func toolBand<Content: View>(
        _ label: String,
        id: String,
        @ViewBuilder content: () -> Content
    ) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(label)
                .font(.caption.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .lineLimit(1)
                .padding(.leading, McpSettingsPage.toolInset)
                .padding(.trailing, 12)
                .padding(.top, 8)
                .padding(.bottom, 2)
                .frame(maxWidth: .infinity, alignment: .leading)
            content()
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("mcp-tool-band-\(id)")
    }

    /// One tool under an open row: its title (medium) + its muted blurb
    /// (the built-in rows keep the Exponential mark).
    private func toolRow(title: String, blurb: String, mark: Bool = false) -> some View {
        HStack(spacing: 8) {
            if mark {
                ExpLogoMark(size: 12, color: .white.opacity(TextOpacity.tertiary))
            }
            Text(title)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white)
                .lineLimit(1)
                .layoutPriority(1)
            Text(blurb)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .lineLimit(1)
            Spacer(minLength: 0)
        }
        .padding(.leading, McpSettingsPage.toolInset)
        .padding(.trailing, 12)
        .padding(.vertical, 8)
        .accessibilityIdentifier("mcp-tool-row")
    }

    /// A muted line under an open row (connect first, a local command, a
    /// failure + its Retry, no tools).
    private func toolsLine(_ text: String) -> some View {
        toolsLine(text) { EmptyView() }
    }

    private func toolsLine<Trailing: View>(
        _ text: String,
        @ViewBuilder trailing: () -> Trailing
    ) -> some View {
        HStack(spacing: 8) {
            Text(text)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 0)
            trailing()
        }
        .padding(.leading, McpSettingsPage.toolInset)
        .padding(.trailing, 12)
        .padding(.vertical, 8)
        .accessibilityIdentifier("mcp-server-tools-line")
    }

    private func loadTools(_ server: McpServerRow) async {
        if let specimen {
            serverTools[server.id] = .loaded(specimen.serverTools[server.id] ?? [])
            return
        }
        serverTools[server.id] = .loading
        do {
            let answer = try await api.tools(accountId: accountId, serverId: server.id)
            serverTools[server.id] = answer.ok
                ? ToolsLoad.loaded(answer.tools ?? [])
                : ToolsLoad.failed(answer.error ?? "the request failed")
        } catch {
            // Closed before the answer came: the next open fetches again.
            if Task.isCancelled {
                serverTools[server.id] = nil
                return
            }
            serverTools[server.id] = .failed(error.localizedDescription)
        }
    }

    /// The viewer's ONE action (web `ConnectionAction`); a connected row
    /// shows a read-only caption, its Disconnect lives in the row's menu.
    @ViewBuilder
    private func connectionAction(_ server: McpServerRow) -> some View {
        switch server.connection.status {
        case "not_needed":
            Text("No sign-in needed")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .fixedSize()
        case "connected":
            HStack(spacing: 4) {
                AppIcon(AppIcons.uiSuccess, size: 12)
                Text("Connected").font(.caption)
            }
            .foregroundStyle(DesignTokens.Semantic.green)
            .fixedSize()
        case "expired", "error":
            // Web's outline `Button size="sm"`, its warning glyph in yellow.
            let live = pendingServer != server.id
            Button { connect(server) } label: {
                GlassSubmitLabel("Reconnect", enabled: live, small: true, outline: true) {
                    AppIcon(AppIcons.uiWarning, size: GlassPillTokens.glyphMd).foregroundStyle(.yellow)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.pressScale)
            .disabled(!live)
            .fixedSize()
        default:
            let secret = server.auth == "secret"
            // EXP-1676: web's primary `Button size="sm"`, glyph + label.
            let live = pendingServer != server.id
            Button { connect(server) } label: {
                GlassSubmitLabel(secret ? "Set key" : "Connect", enabled: live, small: true) {
                    AppIcon(secret ? AppIcons.uiPrivate : AppIcons.uiSignIn, size: GlassPillTokens.glyphMd)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.pressScale)
            .disabled(!live)
            .fixedSize()
        }
    }

    /// Connecting needs the provider's consent page (or a key form): the
    /// web MCP page in the browser runs it (`?connect=<serverId>`).
    private func connect(_ server: McpServerRow) {
        guard let instanceUrl, let slug = team?.slug else { return }
        let base = instanceUrl.hasSuffix("/") ? String(instanceUrl.dropLast()) : instanceUrl
        guard let url = URL(string: "\(base)/t/\(slug)/settings/mcp?connect=\(server.id)") else { return }
        openURL(url)
    }

    private func disconnect(_ server: McpServerRow) async {
        pendingServer = server.id
        defer { pendingServer = nil }
        do {
            try await api.disconnect(accountId: accountId, serverId: server.id)
            toaster.success("Disconnected from \(server.name)")
            await loadServers()
        } catch {
            toaster.error(FeedbackStates.Errors.title(verb: "disconnect from", thing: server.name))
        }
    }

    // MARK: - Clients

    @ViewBuilder
    private var clientsSection: some View {
        VStack(alignment: .leading, spacing: 0) {
            GlassSectionBand(McpSettings.Section.clients.label ?? "")
            if isOwner {
                GlassSegmentedControl(
                    options: McpSettings.Scope.allCases,
                    selection: scope,
                    label: \.label,
                    identifier: { "settings-mcp-scope-\($0.rawValue)" }
                ) { next in
                    scope = next
                }
                .padding(.bottom, 8)
            }
            Group {
                if let clients {
                    if clients.clients.isEmpty {
                        EmptyState(.mcpClients)
                    } else {
                        VStack(spacing: 0) {
                            ForEach(clients.clients) { client in
                                clientRow(client)
                            }
                        }
                    }
                } else {
                    SkeletonRows(rows: 2, rowHeight: McpSettingsPage.rowHeight)
                }
            }
            .glassSection()
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(McpSettings.Section.clients.testId)
    }

    private func clientRow(_ client: McpClientRow) -> some View {
        HStack(spacing: 12) {
            clientIcon(client.icon)
            VStack(alignment: .leading, spacing: 2) {
                Text(client.name)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Text(McpSettings.clientCaption(
                    scope: scope,
                    userName: Self.memberLabel(client.userName, email: client.userEmail),
                    kinds: client.kinds,
                    appsAccess: client.appsAccess,
                    lastUsed: client.lastUsedAt.map { RelativeTime.compact(wire: $0) }
                ))
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .lineLimit(1)
            }
            Spacer(minLength: 0)
            GlassPill(McpSettings.revoke, mode: .action {
                guard !revoking else { return }
                revokeTarget = client
                showRevoke = true
            }, enabled: !revoking)
            .fixedSize()
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .flatRow()
        .accessibilityIdentifier("mcp-client-row")
    }

    @ViewBuilder
    private func clientIcon(_ icon: String?) -> some View {
        if let icon, let url = URL(string: icon) {
            AsyncImage(url: url) { image in
                image.resizable().scaledToFit()
            } placeholder: {
                AppIcon(AppIcons.settingsMcp, size: AppIcon.Size.small)
            }
            .frame(width: 16, height: 16)
            .clipShape(RoundedRectangle(cornerRadius: 3))
        } else {
            AppIcon(AppIcons.settingsMcp, size: AppIcon.Size.small)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        }
    }

    private func revoke() async {
        guard let target = revokeTarget, !revoking else { return }
        revoking = true
        defer { revoking = false }
        do {
            try await api.revokeClient(
                accountId: accountId,
                teamId: teamId,
                clientId: target.clientId,
                userId: scope == .everyone ? target.userId : nil
            )
            revokeTarget = nil
            await loadClients()
        } catch {
            toaster.error(FeedbackStates.Errors.title(verb: "revoke", thing: "the client"))
        }
    }

    /// #EXP-1788: a skeleton row = the real row's height, measured on the
    /// simulator: a server row (its Default pill / Connect button) 57 pt, a
    /// client or activity row (name over caption) 54 pt.
    static let serverRowHeight: CGFloat = 57
    static let rowHeight: CGFloat = 54
    /// A tool row under an open server row (title over nothing, 8 pt pads).
    static let toolRowHeight: CGFloat = 34
    /// The tools' leading inset: under the row's name, past the chevron and
    /// the glyph (web `pl-10`).
    static let toolInset: CGFloat = 40

    /// #EXP-1788: a member without a name reads as their email (web + IDE).
    nonisolated static func memberLabel(_ name: String, email: String?) -> String {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? (email ?? name) : name
    }

    // MARK: - Access

    @ViewBuilder
    private var accessSection: some View {
        if let access, !access.apps.isEmpty {
            VStack(alignment: .leading, spacing: 0) {
                GlassSectionBand(McpSettings.Section.access.label ?? "")
                VStack(spacing: 12) {
                    ForEach(access.apps) { app in
                        VStack(spacing: 0) {
                            HStack(spacing: 12) {
                                AgentRowIcon(icon: app.icon ?? "puzzle", color: app.color ?? AppGlyph.neutralColor)
                                Text(app.name)
                                    .font(.subheadline.weight(.medium))
                                    .foregroundStyle(.white)
                                    .lineLimit(1)
                                Spacer(minLength: 8)
                                Text(McpSettings.toolCountLabel(app.toolCount))
                                    .font(.caption)
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                    .fixedSize()
                            }
                            .padding(.horizontal, 12)
                            .padding(.vertical, 10)
                            ForEach(access.rows) { member in
                                accessRow(app: app, member: member, canEdit: access.canEdit)
                            }
                        }
                        .glassSection()
                        .accessibilityElement(children: .contain)
                        .accessibilityIdentifier("mcp-access-app-\(app.slug)")
                    }
                    NavigationLink(value: AppRoute.mcpActivity(accountId: accountId, teamId: teamId)) {
                        GlassPickerRowLabel(McpSettings.activity, value: "")
                            .padding(.horizontal, 12)
                            .padding(.vertical, 12)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .glassSection()
                    .accessibilityIdentifier(McpSettings.TestId.activity)
                }
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(McpSettings.Section.access.testId)
        }
    }

    @ViewBuilder
    private func accessRow(app: McpAppAccess.App, member: McpAppAccess.Row, canEdit: Bool) -> some View {
        let level = member.level(app.id)
        let name = Self.memberLabel(member.name, email: member.email)
        let avatar = UserAvatar(
            image: member.image,
            initials: memberInitials(forDisplayName: name),
            hueKey: avatarHueKey(id: member.userId, name: member.name, email: member.email),
            size: 20
        )
        if canEdit {
            GlassPicker(
                items: McpSettings.Level.allCases.map { PickerItem(value: $0, label: $0.label) },
                mode: .single,
                value: [level],
                onChange: { picked in
                    guard let next = picked.first, next != level else { return }
                    Task { await setLevel(app: app, userId: member.userId, level: next) }
                },
                title: name
            ) {
                HStack(spacing: 12) {
                    avatar
                    GlassPickerRowLabel(name, value: level.label)
                        .font(.subheadline)
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .contentShape(Rectangle())
            }
            .accessibilityIdentifier("mcp-access-\(app.slug)-\(member.userId)")
        } else {
            HStack(spacing: 12) {
                avatar
                GlassPickerRowLabel(name, value: level.label, chevron: false)
                    .font(.subheadline)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
        }
    }

    private func setLevel(app: McpAppAccess.App, userId: String, level: McpSettings.Level) async {
        guard var next = access else { return }
        let before = next
        if let i = next.rows.firstIndex(where: { $0.userId == userId }) {
            next.rows[i].levels[app.id] = level
        }
        access = next
        do {
            try await api.setAppAccess(accountId: accountId, appId: app.id, userId: userId, level: level)
        } catch {
            access = before
            toaster.error(FeedbackStates.Errors.title(verb: "save", thing: "the access"))
        }
    }

    // MARK: - Chrome + data

    /// A section = its label band over one glass group.
    private func section<Content: View>(
        _ section: McpSettings.Section,
        boxed: Bool = true,
        @ViewBuilder content: () -> Content
    ) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            if let label = section.label {
                GlassSectionBand(label)
            }
            if boxed {
                content().glassSection()
            } else {
                content()
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(section.testId)
    }

    private func loadTeam() async {
        guard specimen == nil else { return }
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let teamId = teamId
        let userId = deps.auth.userId
        let read = try? await pool.read { db in
            (
                try TeamEntity.fetchOne(db, key: teamId),
                try TeamPageQueries.isOwner(db, teamId: teamId, userId: userId)
            )
        }
        team = read?.0
        isOwner = read?.1 ?? false
    }

    private func loadServers() async {
        guard specimen == nil else { return }
        let rows = try? await api.list(accountId: accountId, teamId: teamId)
        servers = rows ?? []
    }

    private func loadClients() async {
        if let specimen {
            clients = McpClientsList(canSeeEveryone: specimen.isOwner, clients: specimen.clients)
            return
        }
        clients = nil
        clients = (try? await api.clients(accountId: accountId, teamId: teamId, scope: scope))
            ?? McpClientsList(canSeeEveryone: false, clients: [])
    }

    private func loadAccess() async {
        guard specimen == nil else { return }
        access = try? await api.appAccess(accountId: accountId, teamId: teamId)
    }
}

// MARK: - Row disclosure

/// A Servers row's leading part (owner 2026-11-12): the disclosure header,
/// chevron LEADING (closed = right, open = down), over the glyph, the name
/// (+ its pill) and the caption; the row's actions stay outside it. The
/// SDK's Swift `DisclosureHeader` takes a title only (trailing chevron), so
/// this hosts the row's own content the way web's `DisclosureHeader` +
/// children and Compose's `DisclosureHeader(leading:)` do.
private struct McpRowDisclosure<Glyph: View>: View {
    @Binding var isExpanded: Bool
    let name: String
    let pill: String?
    let caption: String
    var mono: Bool = false
    @ViewBuilder let glyph: () -> Glyph

    var body: some View {
        Button {
            withAnimation(.easeInOut(duration: 0.15)) { isExpanded.toggle() }
        } label: {
            HStack(spacing: 8) {
                AppIcon(AppIcons.uiChevronRight, size: 12)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .rotationEffect(.degrees(isExpanded ? 90 : 0))
                HStack(spacing: 12) {
                    glyph()
                    VStack(alignment: .leading, spacing: 2) {
                        HStack(spacing: 6) {
                            Text(name)
                                .font(.subheadline.weight(.medium))
                                .foregroundStyle(.white)
                                .lineLimit(1)
                            if let pill {
                                GlassPill(pill).fixedSize()
                            }
                        }
                        if !caption.isEmpty {
                            Text(caption)
                                .font(mono ? .expMono(.caption) : .caption)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                .lineLimit(1)
                                .truncationMode(.middle)
                        }
                    }
                }
                Spacer(minLength: 0)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(isExpanded ? "Hide" : "Show") \(name) tools")
        .accessibilityValue(isExpanded ? "expanded" : "collapsed")
        .accessibilityIdentifier("mcp-server-disclosure")
    }
}

// MARK: - Specimen

/// Styleguide capture only (`-uiTestingStyleguide settings-mcp`,
/// `sg_settings-mcp`): THE MCP page as pushed off the Team page, over SAMPLE
/// rows (the capture seed has no app exposing tools and no MCP client yet).
struct McpSettingsSpecimen: View {
    private static let hour: TimeInterval = 3600
    private static func ago(_ seconds: TimeInterval) -> String {
        ISO8601DateFormatter().string(from: Date().addingTimeInterval(-seconds))
    }

    private static var specimen: McpSettingsPage.Specimen {
        McpSettingsPage.Specimen(
            instanceUrl: "https://app.exponential.at",
            isOwner: true,
            servers: [
                McpServerRow(
                    id: "linear", name: "Linear", url: "https://mcp.linear.app/sse", enabledByDefault: true,
                    connection: .init(status: "connected"), usedBy: ["Customer admin"]
                ),
                McpServerRow(
                    id: "sentry", name: "Sentry", url: "https://mcp.sentry.dev/mcp",
                    connection: .init(status: "not_connected")
                ),
            ],
            clients: [
                McpClientRow(
                    clientId: "claude", name: "Claude Code", userId: "u-alex", userName: "Alex",
                    kinds: ["apps", "builtin"], appsAccess: "full", lastUsedAt: ago(2 * hour)
                ),
                McpClientRow(
                    clientId: "chatgpt", name: "ChatGPT", userId: "u-alex", userName: "Alex",
                    kinds: ["apps"], appsAccess: "read", lastUsedAt: nil
                ),
            ],
            access: McpAppAccess(
                canEdit: true,
                apps: [
                    McpAppAccess.App(
                        id: "a-admin", slug: "customer-admin", name: "Customer admin", icon: "server", color: "#3b82f6",
                        toolCount: 3,
                        tools: [
                            McpToolRow(name: "find_customer", title: "Find customer", blurb: "Look a customer up by name or email"),
                            McpToolRow(name: "update_plan", title: "Update plan", blurb: "Move a customer to another plan"),
                            McpToolRow(name: "refund", title: "Refund", blurb: "Refund a customer's last payment"),
                        ]
                    ),
                ],
                rows: [
                    .init(userId: "u-alex", name: "Alex", levels: [:]),
                    // #EXP-1788: a member without a name reads as their email.
                    .init(userId: "u-mia", name: "", email: "mia@acme.dev", levels: ["a-admin": .read]),
                ]
            ),
            serverTools: [
                "linear": [
                    McpToolRow(name: "list_issues", title: "List issues", blurb: "Issues in a Linear team"),
                    McpToolRow(name: "create_issue", title: "Create issue", blurb: "File a new Linear issue"),
                ],
            ]
        )
    }

    var body: some View {
        // A one-deep path, so the page wears the pushed back chevron.
        NavigationStack(path: .constant(["mcp"])) {
            Color.clear
                .navigationDestination(for: String.self) { _ in
                    McpSettingsPage(teamId: "specimen-team", specimen: Self.specimen)
                        .navigationBarTitleDisplayMode(.inline)
                        .bareBackButton()
                }
        }
    }
}

// MARK: - Activity

/// Settings › MCP › Activity (`activityPage`): the `mcp_calls` rows newest
/// first: owners the team's calls, a member their own.
struct McpActivityPage: View {
    let teamId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var data: McpActivityList?

    var body: some View {
        ZStack {
            AppBackground()
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    GlassSectionBand(McpSettings.activity)
                    Group {
                        if let data {
                            if data.calls.isEmpty {
                                EmptyState(.mcpActivity)
                            } else {
                                LazyVStack(spacing: 0) {
                                    ForEach(data.calls) { call in
                                        row(call, owner: data.owner)
                                    }
                                }
                            }
                        } else {
                            SkeletonRows(rowHeight: McpSettingsPage.rowHeight)
                        }
                    }
                    .glassSection()
                }
                .padding(16)
            }
        }
        .navigationTitle(McpSettings.activity)
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .accessibilityIdentifier(McpSettings.TestId.activity)
        .task(id: "\(accountId)/\(teamId)") {
            data = (try? await McpServersApi(trpc: deps.trpc).activity(accountId: accountId, teamId: teamId))
                ?? McpActivityList(owner: false, calls: [])
        }
    }

    private func row(_ call: McpActivityCall, owner: Bool) -> some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(call.tool)
                    .font(.expMono(.subheadline))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Text(McpSettings.activityCaption(
                    owner: owner,
                    appName: call.appName,
                    userName: call.userName,
                    clientName: call.clientName,
                    when: RelativeTime.compact(wire: call.createdAt)
                ))
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .lineLimit(1)
            }
            Spacer(minLength: 0)
            GlassPill(McpSettings.statusLabel(call.status)).fixedSize()
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .accessibilityIdentifier("mcp-activity-row")
    }
}

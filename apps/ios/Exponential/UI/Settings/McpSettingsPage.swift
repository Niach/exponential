import ExpCore
import ExpUI
import GRDB
import SwiftUI
import UIKit

// #VAPP-185 (fixtures/settings-mcp.json): Settings › MCP, ONE page ×4 (web
// `mcp-settings-page.tsx`): Endpoint · Servers · Clients (Mine | Everyone) ·
// Access (+ Activity) · Built-in Exponential tools. Phones have no add/edit:
// the Servers rows carry the member's own Connect (the web page in the
// browser, `?connect=<id>`) / Disconnect.

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
    @State private var access: McpAppAccess?
    @State private var builtinQuery = ""

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
                endpointSection
                serversSection
                clientsSection
                accessSection
                builtinSection
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

    // MARK: - Endpoint

    @ViewBuilder
    private var endpointSection: some View {
        let url = instanceUrl.map(McpSettings.endpoint(instanceUrl:)) ?? McpSettings.endpointPath
        section(.endpoint) {
            HStack(spacing: 12) {
                Text(url)
                    .font(.subheadline.monospaced())
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 0)
                GlassPill(McpSettings.copy, icon: AppIcons.uiCopy, mode: .action {
                    UIPasteboard.general.string = url
                    toaster.success(McpSettings.copied)
                })
                .fixedSize()
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
        }
    }

    // MARK: - Servers

    @ViewBuilder
    private var serversSection: some View {
        section(.servers) {
            if let servers {
                if servers.isEmpty {
                    EmptyState(.mcpServers)
                } else {
                    VStack(spacing: 0) {
                        ForEach(servers) { server in
                            serverRow(server)
                        }
                    }
                }
            } else {
                SkeletonRows(rows: 2, rowHeight: 52)
            }
        }
    }

    private func serverRow(_ server: McpServerRow) -> some View {
        let caption = [McpServers.location(server), McpSettings.usedByCaption(server.usedBy)]
            .compactMap { $0 }
            .joined(separator: " · ")
        let connected = server.connection.status == "connected"
        return HStack(spacing: 12) {
            AppIcon(AppIcons.uiMcp, size: AppIcon.Size.small)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(server.name)
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                    if server.enabledByDefault {
                        GlassPill("Default").fixedSize()
                    }
                }
                if !caption.isEmpty {
                    Text(caption)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                }
            }
            Spacer(minLength: 0)
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
            GlassPill("Reconnect", icon: AppIcons.uiWarning, mode: .action { connect(server) },
                      enabled: pendingServer != server.id)
                .fixedSize()
        default:
            let secret = server.auth == "secret"
            GlassPill(secret ? "Set key" : "Connect", icon: secret ? AppIcons.uiPrivate : AppIcons.uiSignIn,
                      mode: .action { connect(server) }, enabled: pendingServer != server.id)
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
            GlassSectionBand(McpSettings.Section.clients.label)
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
                    SkeletonRows(rows: 2, rowHeight: 52)
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
                    userName: client.userName,
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
                revokeTarget = client
                showRevoke = true
            })
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
        guard let target = revokeTarget else { return }
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

    // MARK: - Access

    @ViewBuilder
    private var accessSection: some View {
        if let access, !access.apps.isEmpty {
            VStack(alignment: .leading, spacing: 0) {
                GlassSectionBand(McpSettings.Section.access.label)
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
        let avatar = UserAvatar(
            image: member.image,
            initials: memberInitials(forDisplayName: member.name),
            hueKey: member.userId,
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
                title: member.name
            ) {
                HStack(spacing: 12) {
                    avatar
                    GlassPickerRowLabel(member.name, value: level.label)
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
                GlassPickerRowLabel(member.name, value: level.label, chevron: false)
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

    // MARK: - Built-in

    @ViewBuilder
    private var builtinSection: some View {
        let shown = McpSettings.filterBuiltinTools(McpSettings.builtinTools, query: builtinQuery)
        VStack(alignment: .leading, spacing: 0) {
            GlassSectionBand(McpSettings.Section.builtin.label)
            GlassSheetSearchField(
                placeholder: McpSettings.builtinFilterPlaceholder,
                text: $builtinQuery,
                accessibilityIdentifier: "mcp-builtin-tools-filter"
            )
            .padding(.bottom, 8)
            Group {
                if shown.isEmpty {
                    EmptyState(.builtinToolsNoMatch)
                } else {
                    LazyVStack(spacing: 0) {
                        ForEach(shown) { tool in
                            HStack(spacing: 8) {
                                ExpLogoMark(size: 12, color: .white.opacity(TextOpacity.tertiary))
                                Text(tool.title)
                                    .font(.subheadline.weight(.medium))
                                    .foregroundStyle(.white)
                                    .lineLimit(1)
                                    .fixedSize()
                                Text(tool.blurb)
                                    .font(.caption)
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                    .lineLimit(1)
                                Spacer(minLength: 0)
                            }
                            .padding(.horizontal, 12)
                            .padding(.vertical, 8)
                        }
                    }
                }
            }
            .glassSection()
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(McpSettings.Section.builtin.testId)
    }

    // MARK: - Chrome + data

    /// A section = its label band over one glass group.
    private func section<Content: View>(
        _ section: McpSettings.Section,
        @ViewBuilder content: () -> Content
    ) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            GlassSectionBand(section.label)
            content().glassSection()
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
                    connection: .init(status: "connected"), usedBy: ["m5 admin"]
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
                apps: [McpAppAccess.App(id: "m5", slug: "m5-admin", name: "m5 admin", icon: "server", color: "#3b82f6", toolCount: 3)],
                rows: [
                    .init(userId: "u-alex", name: "Alex", levels: [:]),
                    .init(userId: "u-mia", name: "Mia", levels: ["m5": .read]),
                ]
            )
        )
    }

    var body: some View {
        // A one-deep path, so the page wears the pushed back chevron.
        NavigationStack(path: .constant(["mcp"])) {
            Color.clear
                .navigationDestination(for: String.self) { _ in
                    McpSettingsPage(teamId: "specimen-team", specimen: Self.specimen)
                        .navigationBarTitleDisplayMode(.inline)
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
                            SkeletonRows(rowHeight: 52)
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
                    .font(.subheadline.monospaced())
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

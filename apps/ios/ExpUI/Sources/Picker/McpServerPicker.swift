import ExpCore
import SwiftUI

// EXP-792/1249 contract — the MCP servers picker: the team's servers this run
// gets, a MULTI pick, each by the `ui-mcp` concept + name. A server the
// caller has not connected (or whose sign-in expired) stays listed but greyed
// with "Connect first" / "Reconnect first" as its note — connecting happens
// in Settings on web or the IDE. The composer's "+" menu opens it (web
// `McpServerPicker`, Android `McpServerPicker.kt`).
public struct McpServerPicker<Trigger: View>: View {
    public let servers: [McpServerRow]
    public let value: [String]
    public let onChange: ([String]) -> Void
    public let title: String
    public let open: Binding<Bool>?
    public let hideTrigger: Bool
    public let onDismiss: (() -> Void)?
    private let trigger: () -> Trigger

    public init(
        servers: [McpServerRow],
        value: [String],
        onChange: @escaping ([String]) -> Void,
        title: String = "MCP servers",
        open: Binding<Bool>? = nil,
        hideTrigger: Bool = false,
        onDismiss: (() -> Void)? = nil,
        @ViewBuilder trigger: @escaping () -> Trigger
    ) {
        self.servers = servers
        self.value = value
        self.onChange = onChange
        self.title = title
        self.open = open
        self.hideTrigger = hideTrigger
        self.onDismiss = onDismiss
        self.trigger = trigger
    }

    /// One row per server: not-ready ones disabled with their note, ready
    /// ones described by where they live.
    nonisolated public static func items(_ servers: [McpServerRow]) -> [PickerItem<String>] {
        servers.map { server in
            let note = McpServers.notReadyLabel(server)
            return PickerItem(
                value: server.id,
                label: server.name,
                icon: AppIcons.uiMcp,
                description: note ?? McpServers.location(server),
                disabled: note != nil
            )
        }
    }

    public var body: some View {
        GlassPicker(
            items: Self.items(servers),
            mode: .multi,
            value: Set(value),
            onChange: { picked in
                // Keep the team's order, not the set's.
                onChange(servers.map(\.id).filter { picked.contains($0) })
            },
            emptyText: "No MCP servers",
            title: title,
            open: open,
            hideTrigger: hideTrigger,
            onDismiss: onDismiss,
            sheetIdentifier: "mcp-server-picker",
            trigger: trigger
        )
    }
}

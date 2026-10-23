import ExpCore
import ExpUI
import Foundation
import GRDB
import SwiftUI
#if canImport(ExponentialUI)
import ExponentialUI
#endif

// VAPP-110: the iOS app as an Exponential UI HOST (the web's
// `lib/exponential-ui-host.tsx`, the desktop's `exponential_ui_host.rs`).
// Everything goes through the SDK's public host API (`ExponentialHost` +
// `HostSurface`), like a third party's host would:
//   - bindings: the `exp:` scheme over the synced GRDB store
//     (`exp:devices`, `exp:issues?board=…&limit=…`), derived by
//     `AppHostSources` exactly like the web host;
//   - functions: `harness.toast`, `harness.openIssue`, `harness.openDevice`,
//     `harness.mcp` (a tools/call on the instance's /api/mcp as the viewer);
//   - extensions: the app extension (`packages/ui/exponential-ui/extension.json`,
//     bundled) with the `IconDisc` + `LiveDot` painters, and the built-in
//     Devices package (`templates/devices.json`, bundled);
//   - policy: `harness.mcp` asks first (THE consent card, the
//     `exponential-ui-consent` prompt), urls through the URL policy (an
//     instance link opens in-app like a markdown link, anything else in the
//     Safari sheet).

/// The consent card's store: one pending `ask` call at a time, answered by
/// the person (Allow / Deny). A second ask denies the first.
@MainActor
@Observable
final class AppConsentGate {
    /// The tool the pending call runs (`harness.mcp`'s tool, else the
    /// function's own name); nil = nothing pending.
    private(set) var pendingTool: String?
    @ObservationIgnored private var resolve: ((Bool) -> Void)?

    nonisolated init() {}

    func ask(tool: String) async -> Bool {
        resolve?(false)
        return await withCheckedContinuation { continuation in
            pendingTool = tool
            resolve = { continuation.resume(returning: $0) }
        }
    }

    func answer(_ ok: Bool) {
        let pending = resolve
        resolve = nil
        pendingTool = nil
        pending?(ok)
    }
}

/// THE consent card over the host's surface (the app's own `GlassAlert`,
/// never a system dialog): Deny focused, Allow the explicit default.
struct AppConsentCard: ViewModifier {
    let gate: AppConsentGate

    func body(content: Content) -> some View {
        content.glassAlert(
            isPresented: Binding(
                get: { gate.pendingTool != nil },
                set: { if !$0 { gate.answer(false) } }
            ),
            onDismiss: { gate.answer(false) }
        ) {
            GlassAlert(
                prompt: Prompts.ExponentialUiConsent.copy(tool: gate.pendingTool ?? ""),
                handlers: [
                    "deny": { gate.answer(false) },
                    "allow": { gate.answer(true) },
                ]
            )
        }
    }
}

/// The bundled app-extension catalog and Devices package (bundled from
/// `packages/ui/exponential-ui/` by Project.swift, byte for byte what the web
/// and desktop hosts install).
enum AppHostResources {
    static let extensionJSON: String? = text("extension")
    static let devicesPackageJSON: String? = text("devices")

    private static func text(_ name: String) -> String? {
        guard let url = Bundle.main.url(forResource: name, withExtension: "json") else { return nil }
        return try? String(contentsOf: url, encoding: .utf8)
    }
}

#if canImport(ExponentialUI)

/// The theme every app surface paints with (the web's `DEFAULT_THEME_ID`).
private let appHostThemeId = "exponential"

/// Online-ness ages out on a clock, not on a delta (the Devices page's tick).
private let livenessTick: Duration = .seconds(30)

/// One app host per preview: the SDK host plus the store observations feeding
/// its `exp:` source. Keep it alive as long as its surface is on screen;
/// `close()` cancels everything.
@MainActor
final class ExpAppHost {
    let host: ExponentialHost
    let consent: AppConsentGate

    init(
        packages: [String],
        accountId: String,
        deps: AppDependencies,
        consent: AppConsentGate,
        onOpenIssue: @escaping (String) -> Void
    ) {
        self.consent = consent
        let base = deps.auth.instanceBaseURL(forAccountId: accountId)
        let origin = base.map { $0.absoluteString.hasSuffix("/") ? String($0.absoluteString.dropLast()) : $0.absoluteString }
        let pool = try? deps.db.pool(forAccountId: accountId)

        var extensions: [HostExtension] = []
        if let json = AppHostResources.extensionJSON {
            extensions.append(HostExtension(json: json, painters: [
                "IconDisc": IconDiscPainter(),
                "LiveDot": LiveDotPainter(),
            ]))
        }
        let installed = [AppHostResources.devicesPackageJSON].compactMap { $0 } + packages

        let schemes = ["https"] + (base?.scheme == "http" ? ["http"] : [])
        host = ExponentialHost(HostOptions(
            functions: [
                "harness.toast": { args, _ in
                    let text = args["message"]?.displayText ?? ""
                    if args["tone"]?.string == "error" {
                        Toaster.shared.error(text)
                    } else {
                        Toaster.shared.info(text)
                    }
                    return nil
                },
                "harness.openIssue": { args, _ in
                    let identifier = args["identifier"]?.displayText ?? ""
                    if let issueId = AppHostStore.issueId(identifier: identifier, pool: pool) {
                        onOpenIssue(issueId)
                    } else {
                        Toaster.shared.error(
                            FeedbackStates.Errors.title(verb: "open", thing: identifier),
                            description: "It is not synced here."
                        )
                    }
                    return nil
                },
                // The web host's default: the device's name as a toast.
                "harness.openDevice": { args, _ in
                    let label = args["label"]?.displayText ?? ""
                    Toaster.shared.info(label.isEmpty ? (args["id"]?.displayText ?? "") : label)
                    return nil
                },
                "harness.mcp": { args, _ in
                    try await AppHostMcp.call(
                        tool: args["tool"]?.displayText ?? "",
                        arguments: args["arguments"]?.object ?? [:],
                        accountId: accountId,
                        base: base,
                        httpClient: deps.httpClient
                    )
                },
            ],
            sources: [
                "exp": { source, emit in
                    switch source.name {
                    case "devices": return AppHostStore.devices(pool: pool, emit: emit)
                    case "issues":
                        return AppHostStore.issues(
                            pool: pool, board: source.params["board"], limit: source.params["limit"], emit: emit
                        )
                    default:
                        emit(.null)
                        return nil
                    }
                },
            ],
            extensions: extensions,
            packages: installed,
            policy: HostPolicy(
                functions: FunctionPolicy(ask: ["harness.mcp"]),
                onFunctionCall: { [consent] call in
                    let tool = call.name == "harness.mcp" ? (call.args["tool"]?.displayText ?? "") : call.name
                    return await consent.ask(tool: tool)
                },
                urls: UrlPolicy(baseUrl: origin),
                openUrl: { url in
                    // An instance link opens in-app (the markdown links' rule),
                    // anything else in the Safari sheet: never UIApplication.open.
                    if let action = AgentMarkdownText.linkAction(
                        url, base: base, origin: origin ?? "", accountId: accountId, deps: deps
                    ) {
                        action()
                    }
                },
                media: MediaOptions(baseUrl: origin, schemes: schemes)
            ),
            theme: ThemeHandle.builtin(appHostThemeId),
            mode: .dark,
            plugin: AppHostPlugin()
        ))
    }

    /// Paint `templateId` of `packageId` into `surfaceId` (`applyTemplate`).
    func apply(surfaceId: String, packageId: String, templateId: String) {
        let message: JSONValue = .object([
            "version": .string("v0.9"),
            "applyTemplate": .object([
                "surfaceId": .string(surfaceId),
                "templateId": .string(templateId),
                "packageId": .string(packageId),
            ]),
        ])
        host.receive(message)
    }

    func close() {
        host.close()
        consent.answer(false)
    }
}

/// The painter callbacks the host does not own: icons by CONCEPT through the
/// app's registry (then raw registry names, the web host's map).
@MainActor
private final class AppHostPlugin: HostPlugin {
    func icon(_ name: String, size: CGFloat) -> AnyView? {
        guard let resolved = AppIcons.conceptOrName(name) else { return nil }
        return AnyView(AppIcon(resolved, size: size))
    }
}

/// `IconDisc`: a 32pt circle in the tone colour at 15 % with the icon
/// concept centred in the tone colour (web `IconDiscPainter`).
@MainActor
private final class IconDiscPainter: ExtensionPainter {
    func measure(_ leaf: ExtensionLeaf, wrap: CGFloat?) -> CGSize? { CGSize(width: 32, height: 32) }

    func paint(_ context: ExtensionContext) throws -> AnyView {
        let key: String = switch context.props["tone"]?.string {
        case "success": "success"
        case "danger": "destructive"
        case "muted": "mutedForeground"
        default: "primary"
        }
        let tone = context.theme?.color(key, mode: context.mode)
            ?? context.theme?.color("primary", mode: context.mode)
            ?? DesignTokens.Palette.primary
        let icon = context.props["icon"]?.string.flatMap(AppIcons.conceptOrName)
        return AnyView(
            ZStack {
                Circle().fill(tone.opacity(0.15))
                if let icon {
                    AppIcon(icon, size: 16).foregroundStyle(tone)
                }
            }
            .frame(width: 32, height: 32)
        )
    }
}

/// `LiveDot`: the 8pt status dot, in the colours the app paints its device
/// and session dots with (the desktop's `LiveDotPainter`).
@MainActor
private final class LiveDotPainter: ExtensionPainter {
    private static let size: CGFloat = 8

    func measure(_ leaf: ExtensionLeaf, wrap: CGFloat?) -> CGSize? { CGSize(width: Self.size, height: Self.size) }

    func paint(_ context: ExtensionContext) throws -> AnyView {
        let muted = context.theme?.color("mutedForeground", mode: context.mode) ?? .white.opacity(TextOpacity.tertiary)
        let tone: Color = switch context.props["tone"]?.string {
        case "live": DesignTokens.Semantic.green
        case "attention": DesignTokens.Semantic.yellow
        case "done": DesignTokens.Semantic.blue
        case "unread": context.theme?.color("primary", mode: context.mode) ?? muted
        case "idle": muted.opacity(0.4)
        default: muted
        }
        return AnyView(
            Circle()
                .fill(tone)
                .frame(width: Self.size, height: Self.size)
                .frame(width: context.size.width, height: context.size.height)
        )
    }
}

/// The `exp:` source over GRDB: one ValueObservation per bound source,
/// re-derived on every store change (devices also on the liveness tick); a
/// value equal to the last one is not re-emitted.
@MainActor
enum AppHostStore {
    static func devices(pool: DatabasePool?, emit: @escaping (JSONValue?) -> Void) -> (() -> Void)? {
        guard let pool else {
            emit(JSONValue.parse(AppHostSources.json(AppHostSources.devicesValue([], now: Date()))))
            return nil
        }
        var rows: [DeviceEntity] = []
        var last: String?
        let push = {
            let inputs = rows.map { row in
                AppHostSources.DeviceInput(
                    id: row.id, label: row.label, kind: row.kind, platform: row.platform,
                    version: row.version, icon: DeviceIconDisplay.iconName(for: row), lastSeenAt: row.lastSeenAt
                )
            }
            let json = AppHostSources.json(AppHostSources.devicesValue(inputs, now: Date()))
            guard json != last else { return }
            last = json
            emit(JSONValue.parse(json))
        }
        let observe = Task { @MainActor in
            do {
                for try await next in ValueObservation.tracking({ try DeviceEntity.fetchAll($0) }).values(in: pool) {
                    rows = next
                    push()
                }
            } catch {}
        }
        let tick = Task { @MainActor in
            while !Task.isCancelled {
                try? await Task.sleep(for: livenessTick)
                guard !Task.isCancelled else { return }
                push()
            }
        }
        return {
            observe.cancel()
            tick.cancel()
        }
    }

    static func issues(
        pool: DatabasePool?, board: String?, limit: String?, emit: @escaping (JSONValue?) -> Void
    ) -> (() -> Void)? {
        guard let pool else {
            emit(JSONValue.parse(AppHostSources.json(AppHostSources.issuesValue([], board: board, limit: limit))))
            return nil
        }
        let observation = ValueObservation.tracking { db -> [IssueEntity] in
            var request = IssueEntity.all()
            if let board, !board.isEmpty { request = request.filter(Column("board_id") == board) }
            return try request.fetchAll(db)
        }
        var last: String?
        let observe = Task { @MainActor in
            do {
                for try await rows in observation.values(in: pool) {
                    let inputs = rows.map { row in
                        AppHostSources.IssueInput(
                            id: row.id, identifier: row.identifier ?? "", title: row.title,
                            status: row.status, boardId: row.boardId, sortOrder: row.sortOrder ?? 0
                        )
                    }
                    let json = AppHostSources.json(AppHostSources.issuesValue(inputs, board: board, limit: limit))
                    guard json != last else { continue }
                    last = json
                    emit(JSONValue.parse(json))
                }
            } catch {}
        }
        return { observe.cancel() }
    }

    /// A synced issue's id by identifier (nil = not synced here).
    static func issueId(identifier: String, pool: DatabasePool?) -> String? {
        guard let pool, !identifier.isEmpty else { return nil }
        return try? pool.read { db in
            try String.fetchOne(
                db,
                sql: #"SELECT "id" FROM "issues" WHERE upper("identifier") = upper(?) LIMIT 1"#,
                arguments: [identifier]
            )
        }
    }
}

/// `harness.mcp`: a JSON-RPC `tools/call` on the instance's own MCP endpoint
/// as the signed-in viewer (the consent card has already said yes); the
/// answer's `result`, read from a JSON or an SSE body (the web's `callMcp`).
enum AppHostMcp {
    struct CallError: LocalizedError {
        let message: String
        var errorDescription: String? { message }
    }

    @MainActor
    static func call(
        tool: String, arguments: [String: JSONValue], accountId: String, base: URL?, httpClient: HTTPClient
    ) async throws -> Any? {
        guard let base, let url = URL(string: "/api/mcp", relativeTo: base)?.absoluteURL else {
            throw CallError(message: "not signed in")
        }
        let id = 1
        let body: JSONValue = .object([
            "jsonrpc": .string("2.0"),
            "id": .number(Double(id)),
            "method": .string("tools/call"),
            "params": .object(["name": .string(tool), "arguments": .object(arguments)]),
        ])
        var request = httpClient.request(url, accountId: accountId, method: "POST", body: Data(body.json.utf8))
        request.setValue("application/json, text/event-stream", forHTTPHeaderField: "Accept")
        request.timeoutInterval = 60
        let (data, response) = try await httpClient.session.data(for: request)
        if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
            throw CallError(message: "MCP \(http.statusCode)")
        }
        guard let message = answer(String(decoding: data, as: UTF8.self), id: id) else {
            throw CallError(message: "MCP: no answer")
        }
        if let error = message["error"], !error.isNull {
            throw CallError(message: error["message"]?.string ?? "MCP error")
        }
        return message["result"]?.any
    }

    /// The message answering `id` in a JSON or SSE body.
    static func answer(_ body: String, id: Int) -> JSONValue? {
        func pick(_ text: String) -> JSONValue? {
            let parsed = JSONValue.parse(text)
            let messages = parsed.array ?? [parsed]
            return messages.first { $0["id"]?.number == Double(id) }
        }
        let trimmed = body.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.hasPrefix("{") || trimmed.hasPrefix("[") { return pick(trimmed) }
        var data: [String] = []
        for raw in body.components(separatedBy: "\n") {
            let line = raw.hasSuffix("\r") ? String(raw.dropLast()) : raw
            if line.hasPrefix("data:") {
                let rest = line.dropFirst(5)
                data.append(String(rest.hasPrefix(" ") ? rest.dropFirst() : rest))
            } else if line.isEmpty, !data.isEmpty {
                if let message = pick(data.joined(separator: "\n")) { return message }
                data.removeAll()
            }
        }
        return data.isEmpty ? nil : pick(data.joined(separator: "\n"))
    }
}

#endif

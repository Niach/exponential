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
//     (`exp:devices`, derived by `AppHostSources` exactly like the web
//     host; VAPP-119 `exp:issues` / `boards` / `members` / `sessions` /
//     `comments` / `attachments` over the team's rows (`AppSources`, the
//     fixture's derivation); VAPP-122 `exp:records?kind=…` over the app's
//     own rows (`AppRecords`), VAPP-161 also the format's neutral
//     `records:<kind>?…` (`AppRecords.recordsSourceParams`);
//   - functions: `harness.toast`, `harness.openIssue`, `harness.openDevice`,
//     `harness.mcp` (a tools/call on the instance's /api/mcp as the viewer),
//     `harness.records.upsert` / `.delete` (tRPC `apps.records.*`, no
//     consent card: the app's own rows; a refusal = the failed-action toast;
//     VAPP-161 the neutral `records.upsert` / `.delete` = the same closures);
//   - extensions: the app extension (`packages/ui/exponential-ui/extension.json`,
//     bundled) with the `IconDisc` + `LiveDot` painters (Exponential ships
//     no built-in apps: only the team's packages install);
//   - VAPP-120 typed actions: every `exponential.<noun>.<op>` of
//     `AppActions.all` = a `harness.mcp`-style tools/call with `{op, …args}`
//     (no op for the verb tool `exponential.sessions.send`);
//   - policy: `harness.mcp` and `exponential.*` ask first (THE consent card,
//     the `exponential-ui-consent` prompt; a typed action worded for its
//     operation by `AppActions.actionConsent`), except a typed action the
//     hosting app's synced row grants (`apps.granted_tools`): no card; urls through the URL policy (an
//     instance link opens in-app like a markdown link, anything else in the
//     Safari sheet); media only through `Apps.appMediaAllowed` (VAPP-157:
//     a data: image/audio/video or the instance's own attachment).

/// VAPP-107 (the consent prompt's `slot`): what the card shows of a call:
/// the tool it names, its arguments as compact JSON (a `harness.mcp` call's
/// MCP `arguments`, else the function's own args; an empty object = none)
/// and whether it sounds destructive. The web `consentWords` and desktop
/// `consent_words` derive the same three.
struct AppConsentWords: Equatable {
    let tool: String
    let args: String?
    let destructive: Bool
    /// VAPP-120: a typed action's title (`actionConsent`), replacing the
    /// generic prompt's; nil = the generic copy.
    var title: String? = nil

    /// Past this many characters the arguments show as a preview (desktop's
    /// rule; the web adds a Show all disclosure).
    static let previewChars = 160

    var argsPreview: String? {
        guard let args else { return nil }
        return args.count > Self.previewChars ? String(args.prefix(Self.previewChars)) + "…" : args
    }
}

/// The consent card's store: one pending `ask` call at a time, answered by
/// the person (Allow / Deny). A second ask denies the first.
@MainActor
@Observable
final class AppConsentGate {
    /// The pending call's words; nil = nothing pending.
    private(set) var pending: AppConsentWords?
    @ObservationIgnored private var resolve: ((Bool) -> Void)?

    nonisolated init() {}

    func ask(_ words: AppConsentWords) async -> Bool {
        resolve?(false)
        return await withCheckedContinuation { continuation in
            pending = words
            resolve = { continuation.resume(returning: $0) }
        }
    }

    func answer(_ ok: Bool) {
        let done = resolve
        resolve = nil
        pending = nil
        done?(ok)
    }
}

/// THE consent card over the host's surface (the app's own `GlassAlert`,
/// never a system dialog): Deny focused, Allow the explicit default (the
/// destructive entry when the call sounds destructive), the call's
/// arguments in the CodeBlock under the body.
struct AppConsentCard: ViewModifier {
    let gate: AppConsentGate

    func body(content: Content) -> some View {
        content.glassAlert(
            isPresented: Binding(
                get: { gate.pending != nil },
                set: { if !$0 { gate.answer(false) } }
            ),
            onDismiss: { gate.answer(false) }
        ) {
            GlassAlert(
                prompt: Prompts.exponentialUiConsent(
                    tool: gate.pending?.tool ?? "",
                    destructive: gate.pending?.destructive ?? false,
                    title: gate.pending?.title
                ),
                handlers: [
                    "deny": { gate.answer(false) },
                    "allow": { gate.answer(true) },
                ]
            ) {
                if let args = gate.pending?.argsPreview {
                    CodeBlockView(args, title: nil, scrollToEnd: false)
                        .accessibilityIdentifier("exponential-ui-consent-arguments")
                }
            }
        }
    }
}

/// The bundled app-extension catalog (bundled from
/// `packages/ui/exponential-ui/` by Project.swift, byte for byte what the web
/// and desktop hosts install).
enum AppHostResources {
    static let extensionJSON: String? = text("extension")

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

    /// The consent card's words for a host call (see [AppConsentWords]).
    nonisolated static func consentWords(name: String, args: [String: JSONValue]) -> AppConsentWords {
        // VAPP-120: a typed action is worded for its operation (the
        // contract's display rows).
        if let typed = AppActions.actionConsent(name, args: args.compactMapValues { CardJSON(any: $0.any) }) {
            return AppConsentWords(tool: typed.tool, args: typed.args, destructive: typed.destructive, title: typed.title)
        }
        let mcp = name == "harness.mcp"
        let tool = mcp ? (args["tool"]?.displayText ?? "") : name
        let raw: [String: JSONValue]? = mcp ? args["arguments"]?.object : args
        let json = raw.map { JSONValue.object($0).json }
        let op = raw?["op"]?.displayText
        return AppConsentWords(
            tool: tool,
            args: (json == nil || json == "{}") ? nil : json,
            destructive: Prompts.destructiveTool(tool, op: op)
        )
    }
    let consent: AppConsentGate

    /// VAPP-120: every typed action (`AppActions.all`) as a tools/call on the
    /// instance's MCP as the viewer: `{op, …args}` for a noun's op (the
    /// name's op wins over an `op` arg: the card and the grant name THIS
    /// operation), the args alone for the verb tool.
    static func actionFunctions(accountId: String, base: URL?, httpClient: HTTPClient) -> [String: HostFunction] {
        var functions: [String: HostFunction] = [:]
        for name in AppActions.all {
            guard let target = AppActions.parseActionName(name) else { continue }
            functions[name] = { args, _ in
                try await AppHostMcp.call(
                    tool: target.tool,
                    arguments: AppActions.arguments(args, op: target.op) { .string($0) },
                    accountId: accountId, base: base, httpClient: httpClient
                )
            }
        }
        return functions
    }

    init(
        packages: [String],
        appId: String?,
        teamId: String?,
        me: String?,
        accountId: String,
        deps: AppDependencies,
        consent: AppConsentGate,
        grants: Bool = true,
        onOpenIssue: @escaping (String) -> Void
    ) {
        self.consent = consent
        let base = deps.auth.instanceBaseURL(forAccountId: accountId)
        let origin = base.map { $0.absoluteString.hasSuffix("/") ? String($0.absoluteString.dropLast()) : $0.absoluteString }
        let ownHost = base?.host.map { [$0] }
        let pool = try? deps.db.pool(forAccountId: accountId)

        var extensions: [HostExtension] = []
        if let json = AppHostResources.extensionJSON {
            extensions.append(HostExtension(json: json, painters: [
                "IconDisc": IconDiscPainter(),
                "LiveDot": LiveDotPainter(),
            ]))
        }
        let installed = packages
        let scope = AppHostStore.RecordsScope(appId: appId, teamId: teamId, me: me)
        let appsApi = deps.appsApi

        // VAPP-157: `data` for the fixture's data image/audio/video (the
        // plugin's `resolveUrl` already narrowed every src to the media rule).
        let schemes = ["https"] + (base?.scheme == "http" ? ["http"] : []) + ["data"]
        let actions = Self.actionFunctions(accountId: accountId, base: base, httpClient: deps.httpClient)
        // VAPP-122: the app's own rows (no consent card); a refusal (the
        // kind's schema, the caps, the plan quota) = the failed-action toast,
        // and the call fails. VAPP-161: the format's neutral `records.upsert`
        // / `.delete` are the same closures.
        let recordsUpsert: HostFunction = { args, _ in
            do {
                guard let appId = scope.appId else { throw AppHostMcp.CallError(message: "No app to keep records for") }
                let data = args["data"]?.object.flatMap { object in
                    CardJSON(any: try? JSONSerialization.jsonObject(with: Data(JSONValue.object(object).json.utf8)))?.objectValue
                } ?? [:]
                let result = try await appsApi.recordsUpsert(
                    accountId: accountId,
                    appId: appId,
                    kind: args["kind"]?.displayText ?? "",
                    data: data,
                    recordId: args["id"]?.string
                )
                return ["id": result.id]
            } catch {
                Toaster.shared.error(
                    FeedbackStates.Errors.title(verb: "save", thing: "the record"),
                    description: error.trpcUserMessage
                )
                throw error
            }
        }
        let recordsDelete: HostFunction = { args, _ in
            do {
                guard let appId = scope.appId else { throw AppHostMcp.CallError(message: "No app to keep records for") }
                try await appsApi.recordsDelete(
                    accountId: accountId, appId: appId, recordId: args["id"]?.displayText ?? ""
                )
                return nil
            } catch {
                Toaster.shared.error(
                    FeedbackStates.Errors.title(verb: "delete", thing: "the record"),
                    description: error.trpcUserMessage
                )
                throw error
            }
        }
        host = ExponentialHost(HostOptions(
            functions: actions.merging([
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
                "harness.records.upsert": recordsUpsert,
                "harness.records.delete": recordsDelete,
                "records.upsert": recordsUpsert,
                "records.delete": recordsDelete,
            ]) { _, own in own },
            sources: [
                "exp": { source, emit in
                    switch source.name {
                    case "devices": return AppHostStore.devices(pool: pool, emit: emit)
                    case let name where AppSources.kinds.contains(name):
                        return AppHostStore.source(pool: pool, scope: scope, name: name, params: source.params, emit: emit)
                    case "records":
                        return AppHostStore.records(pool: pool, scope: scope, params: source.params, emit: emit)
                    default:
                        emit(.null)
                        return nil
                    }
                },
                // VAPP-161: the format's neutral `records:<kind>?…` =
                // `exp:records?kind=<kind>&…` (the name wins).
                "records": { source, emit in
                    guard let params = AppRecords.recordsSourceParams(
                        scheme: source.scheme, name: source.name, params: source.params
                    ) else {
                        emit(.null)
                        return nil
                    }
                    return AppHostStore.records(pool: pool, scope: scope, params: params, emit: emit)
                },
            ],
            extensions: extensions,
            packages: installed,
            policy: HostPolicy(
                functions: FunctionPolicy(ask: ["harness.mcp", "exponential.*"]),
                onFunctionCall: { [consent] call in
                    // VAPP-120: the app's grant, read off its live synced row
                    // at call time: a granted typed action runs without the
                    // card (`harness.mcp` always asks).
                    if AppActions.parseActionName(call.name) != nil,
                       AppActions.grantAllows(grants ? AppHostStore.grantedTools(appId: scope.appId, pool: pool) : [], call.name,
                                              args: call.args.compactMapValues { CardJSON(any: $0.any) }) {
                        return true
                    }
                    return await consent.ask(ExpAppHost.consentWords(name: call.name, args: call.args))
                },
                // VAPP-107: private hosts are denied unless opted in; the
                // app's OWN instance is (the localhost dev setup), nothing
                // else. A public origin listed here changes nothing.
                urls: UrlPolicy(allowPrivate: ownHost, baseUrl: origin),
                openUrl: { url in
                    // An instance link opens in-app (the markdown links' rule),
                    // anything else in the Safari sheet: never UIApplication.open.
                    if let action = AgentMarkdownText.linkAction(
                        url, base: base, origin: origin ?? "", accountId: accountId, deps: deps
                    ) {
                        action()
                    }
                },
                media: MediaOptions(baseUrl: origin, schemes: schemes, allowPrivate: ownHost)
            ),
            theme: ThemeHandle.builtin(appHostThemeId),
            mode: .dark,
            plugin: AppHostPlugin(origin: origin ?? "")
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
    /// The instance origin (`scheme://host[:port]`, no trailing slash).
    let origin: String

    init(origin: String) {
        self.origin = origin
    }

    /// VAPP-157: the app host NARROWS the SDK's media policy with
    /// `Apps.appMediaAllowed` (fixture `mediaPolicy`), checked on the RAW
    /// src: a denied src resolves to "" and loads nothing (the SDK's denied
    /// rendering); an allowed one still passes `MediaOptions` after.
    func resolveUrl(_ src: String) -> String {
        Apps.appMediaAllowed(src, origin: origin) ? src : ""
    }

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

    /// The tables a VAPP-119 source reads (its observation tracks only them).
    private struct SourceNeeds {
        var boards = false
        var issues = false
        var members = false
        var users = false
        var sessions = false
        var comments = false
        var attachments = false

        init(_ name: String) {
            switch name {
            case "issues": boards = true; issues = true; users = true
            case "boards": boards = true
            case "members": members = true; users = true
            case "sessions": sessions = true
            case "comments": boards = true; issues = true; comments = true; users = true
            case "attachments": boards = true; issues = true; attachments = true; users = true
            default: break
            }
        }
    }

    /// One team's synced rows as `AppSources` reads them (only the tables
    /// `needs` names), timestamps in the web host's ISO form.
    nonisolated private static func sourceStore(_ db: Database, teamId: String, needs: SourceNeeds) throws -> AppSources.Store {
        let iso = AppRecords.isoTimestamp
        var store = AppSources.Store()
        var boardIds: [String] = []
        if needs.boards {
            let boards = try BoardEntity.filter(Column("team_id") == teamId).fetchAll(db)
            boardIds = boards.map(\.id)
            // Trashed / archived boards never sync (the boards shape excludes them).
            store.boards = boards.map {
                .init(id: $0.id, slug: $0.slug, name: $0.name, prefix: $0.prefix, icon: $0.icon, color: $0.color,
                      sortOrder: $0.sortOrder ?? 0, deletedAt: nil)
            }
        }
        if needs.issues, !boardIds.isEmpty {
            store.issues = try IssueEntity.filter(boardIds.contains(Column("board_id"))).fetchAll(db).map {
                .init(id: $0.id, boardId: $0.boardId, identifier: $0.identifier ?? "", title: $0.title,
                      status: $0.status, priority: $0.priority, assigneeId: $0.assigneeId, creatorId: $0.creatorId,
                      dueDate: $0.dueDate, sortOrder: $0.sortOrder ?? 0,
                      createdAt: iso($0.createdAt), updatedAt: iso($0.updatedAt))
            }
        }
        if needs.members {
            store.members = try TeamMemberEntity.filter(Column("team_id") == teamId).fetchAll(db).map {
                .init(userId: $0.userId, role: $0.role)
            }
        }
        if needs.users {
            store.users = try UserEntity.fetchAll(db).map {
                .init(id: $0.id, name: $0.name ?? "", email: $0.email, image: $0.image)
            }
        }
        if needs.sessions {
            store.sessions = try SessionEntity.filter(Column("team_id") == teamId).fetchAll(db).map {
                .init(id: $0.id, parentId: $0.parentId, agentId: $0.agentId, appId: $0.appId, userId: $0.userId,
                      status: $0.status, busy: $0.busy, needsInput: $0.needsInput, title: $0.title,
                      createdAt: iso($0.createdAt), updatedAt: iso($0.updatedAt))
            }
        }
        if needs.comments {
            store.comments = try CommentEntity.filter(Column("team_id") == teamId).fetchAll(db).map {
                .init(id: $0.id, issueId: $0.issueId, authorId: $0.authorId, parentId: $0.parentId,
                      source: $0.source ?? "user", body: $0.body ?? "", createdAt: iso($0.createdAt))
            }
        }
        if needs.attachments {
            store.attachments = try AttachmentEntity.filter(Column("team_id") == teamId).fetchAll(db).map {
                .init(id: $0.id, issueId: $0.issueId, commentId: $0.commentId, uploaderId: $0.uploaderId,
                      filename: $0.filename, contentType: $0.contentType, sizeBytes: Double($0.sizeBytes),
                      width: $0.width.map(Double.init), height: $0.height.map(Double.init), createdAt: iso($0.createdAt))
            }
        }
        return store
    }

    /// VAPP-119 `exp:issues` / `boards` / `members` / `sessions` / `comments`
    /// / `attachments`: the fixture's value (`AppSources.sourceValue`) over
    /// the scope team's synced rows, on every change of the tables the source
    /// reads; a value equal to the last one is not re-emitted.
    static func source(
        pool: DatabasePool?, scope: RecordsScope, name: String, params: [String: String], emit: @escaping (JSONValue?) -> Void
    ) -> (() -> Void)? {
        var last: String?
        let push = { (store: AppSources.Store) in
            let json = AppSources.sourceValue(name: name, params: params, store: store, me: scope.me)?.compactString ?? "null"
            guard json != last else { return }
            last = json
            emit(JSONValue.parse(json))
        }
        guard let pool, let teamId = scope.teamId else {
            push(AppSources.Store())
            return nil
        }
        let needs = SourceNeeds(name)
        let observation = ValueObservation.tracking { db in
            try sourceStore(db, teamId: teamId, needs: needs)
        }
        let observe = Task { @MainActor in
            do {
                for try await store in observation.values(in: pool) {
                    push(store)
                }
            } catch {}
        }
        return { observe.cancel() }
    }

    /// VAPP-122: the app whose records `exp:records` reads (the hosting app,
    /// or `app=<slug>`: another app of the same team) and the viewer.
    struct RecordsScope: Sendable {
        let appId: String?
        let teamId: String?
        let me: String?
    }

    /// What one records read sees: the app's rows and their authors' names.
    private struct RecordsSnapshot: Sendable {
        var rows: [AppRecordEntity] = []
        var names: [String: String] = [:]
    }

    /// `exp:records?kind=…`: the fixture's value (`AppRecords.recordsValue`)
    /// over the synced `app_records` rows of the app, names off `users`, on
    /// every change of the records, the users or the apps (an `app=` slug
    /// may sync later); a value equal to the last one is not re-emitted.
    static func records(
        pool: DatabasePool?, scope: RecordsScope, params: [String: String], emit: @escaping (JSONValue?) -> Void
    ) -> (() -> Void)? {
        let query = AppRecords.parseRecordsQuery(params)
        var last: String?
        let push = { (snapshot: RecordsSnapshot) in
            let value = AppRecords.recordsValue(
                rows: snapshot.rows.map(AppRecords.Row.init), query: query, me: scope.me, now: Date(), names: snapshot.names
            )
            let json = value?.compactString ?? "null"
            guard json != last else { return }
            last = json
            emit(JSONValue.parse(json))
        }
        guard let pool else {
            push(RecordsSnapshot())
            return nil
        }
        let observation = ValueObservation.tracking { db -> RecordsSnapshot in
            var appId = scope.appId
            if let slug = query.app {
                appId = try String.fetchOne(
                    db,
                    sql: #"SELECT "id" FROM "apps" WHERE "slug" = ? AND "team_id" = ? AND "deleted_at" IS NULL LIMIT 1"#,
                    arguments: [slug, scope.teamId ?? ""]
                )
            }
            guard let appId else { return RecordsSnapshot() }
            let rows = try AppRecordEntity.filter(Column("app_id") == appId).fetchAll(db)
            var names: [String: String] = [:]
            let authors = Array(Set(rows.compactMap(\.createdBy)))
            if !authors.isEmpty {
                for user in try UserEntity.filter(keys: authors).fetchAll(db) {
                    names[user.id] = user.name ?? ""
                }
            }
            return RecordsSnapshot(rows: rows, names: names)
        }
        let observe = Task { @MainActor in
            do {
                for try await snapshot in observation.values(in: pool) {
                    push(snapshot)
                }
            } catch {}
        }
        return { observe.cancel() }
    }

    /// VAPP-120: the hosting app's grants off its synced `apps` row (none
    /// without an app or a store).
    static func grantedTools(appId: String?, pool: DatabasePool?) -> [String] {
        guard let pool, let appId else { return [] }
        return (try? pool.read { db in try AppEntity.fetchOne(db, key: appId)?.grantedTools }) ?? []
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

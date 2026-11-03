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
//     fixture's derivation); VAPP-122 the format's `records:<kind>?…` over
//     the app's own rows (`AppRecords`; VAPP-189: the SDK's `RecordsQuery`,
//     eUI's one evaluator);
//   - functions: `harness.toast`, `harness.openIssue`, `harness.openDevice`,
//     `harness.mcp` (a tools/call on the instance's /api/mcp as the viewer),
//     `records.upsert` / `.delete` (tRPC `apps.records.*`, no consent card:
//     the app's own rows; a refusal = the failed-action toast;
//     VAPP-194 `records.batch` = a package action's writes as ONE
//     `apps.records.batch`, its failure = `onActionFailed`'s toast);
//   - #VAPP-230 upstream reads: `upstream:<server>/<tool>?…` through the
//     server broker (`apps.upstreamRead`, re-read every 60 s while bound;
//     ok emits the value, a refusal or offline emits nothing), the
//     "Connect <server>" banner when the viewer is not connected
//     (`AppUpstreamAnswers`, `AppUpstream.bannerServers`);
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

/// #VAPP-230: every live upstream binding's last answer, for the banners
/// (the web's `UpstreamAnswers`): one per server answering
/// `upstreamDisconnected`, sorted (`AppUpstream.bannerServers`).
@MainActor
@Observable
final class AppUpstreamAnswers {
    /// The servers the viewer must connect (one banner each).
    private(set) var servers: [String] = []
    @ObservationIgnored private var answers: [Int: AppUpstream.Answer] = [:]
    @ObservationIgnored private var ids = 0

    /// A new binding's key.
    func nextId() -> Int {
        ids += 1
        return ids
    }

    /// A binding's latest answer; nil = it unbound.
    func set(_ id: Int, _ answer: AppUpstream.Answer?) {
        answers[id] = answer
        let next = AppUpstream.bannerServers(Array(answers.values))
        if next != servers { servers = next }
    }
}

/// VAPP-235: what the first replay of a host call's own entry did (the
/// call awaits it only when its entry was the only pending one).
enum AppRecordOutboxAttempt: Sendable {
    /// The server took it: the online answer.
    case done(AppRecordReplayed)
    /// No network / a retry code: the call answers from its entry.
    case retry
    /// `NOT_FOUND` on a single update or delete: removed silently.
    case dropped
    /// The server refused it (the entry is gone): the call fails.
    case refused(message: String)
}

/// VAPP-235 (eApps O1, app-records.json `outbox`, docs/spec/apps.md §14):
/// the OFFLINE OUTBOX of the app hosts' record writes. Every
/// `records.upsert` / `.delete` / `.batch` call becomes ONE entry in the
/// account's local `app_record_outbox` table (it survives a restart and a
/// resync); one flusher per account replays the pending entries FIFO, ONE
/// request at a time, each with its `writeId` (the server's idempotency
/// key), and STOPS on a retry (no network, a 5xx, signed out). It runs on
/// every enqueue, when the network comes back (`NetworkPathWatcher`) and
/// at launch. A later refusal removes the entry (its optimistic rows roll
/// back) and toasts `Couldn't <verb> the record`; dropped = removed
/// silently; done = `acked` until the synced rows reflect it (the records
/// binding prunes it, `AppRecords.outboxPrune`).
@MainActor
final class AppRecordOutbox {
    private let db: DatabaseManager
    private let api: AppsApi
    private let auth: AuthRepository
    /// Accounts with a flush running, and those asking for another pass.
    private var flushing: Set<String> = []
    private var again: Set<String> = []
    /// write id → the host call awaiting its entry's first replay.
    private var waiters: [String: (accountId: String, resume: (AppRecordOutboxAttempt) -> Void)] = [:]

    nonisolated init(db: DatabaseManager, api: AppsApi, auth: AuthRepository) {
        self.db = db
        self.api = api
        self.auth = auth
    }

    /// Queue one host call: build its entry (client ids for the write and
    /// every create), persist it, then, when it is the only pending entry,
    /// await its first replay (`attempt`); otherwise answer at once (nil).
    func write(
        accountId: String, appId: String, call: AppRecords.OutboxCall, args: [String: CardJSON]
    ) async throws -> (entry: AppRecords.OutboxEntry, attempt: AppRecordOutboxAttempt?) {
        let pool = try db.pool(forAccountId: accountId)
        let creates = AppRecords.outboxCreates(call: call, args: args)
        let newIds = (0..<creates).map { _ in UUID().uuidString.lowercased() }
        let entry = AppRecords.OutboxEntry(
            writeId: UUID().uuidString.lowercased(),
            appId: appId,
            call: call,
            writes: AppRecords.outboxWrites(call: call, args: args, newIds: newIds),
            at: AppRecords.outboxNow()
        )
        let alone = try await pool.write { db -> Bool in
            let pending = try AppRecordOutboxEntity.filter(Column("state") == "pending").fetchCount(db)
            let seq = try Int64.fetchOne(db, sql: #"SELECT COALESCE(MAX("seq"), 0) FROM "app_record_outbox""#) ?? 0
            try AppRecordOutboxEntity(entry, seq: seq + 1).insert(db)
            return pending == 0
        }
        guard alone else {
            flush(accountId: accountId)
            return (entry, nil)
        }
        let attempt = await withCheckedContinuation { continuation in
            waiters[entry.writeId] = (accountId, { continuation.resume(returning: $0) })
            flush(accountId: accountId)
        }
        return (entry, attempt)
    }

    /// Flush every signed-in account (launch, the network coming back).
    func flushAll() {
        for accountId in auth.authenticatedAccountIds.sorted() {
            flush(accountId: accountId)
        }
    }

    /// Start the account's flusher (one at a time; a call while it runs
    /// asks for another pass).
    func flush(accountId: String) {
        guard !flushing.contains(accountId) else {
            again.insert(accountId)
            return
        }
        flushing.insert(accountId)
        Task { @MainActor in
            repeat {
                self.again.remove(accountId)
                await self.drain(accountId: accountId)
            } while self.again.contains(accountId)
            self.flushing.remove(accountId)
            // Anything still waiting (no store) answers from its entry.
            for (writeId, waiter) in self.waiters.filter({ $0.value.accountId == accountId }) {
                self.waiters[writeId] = nil
                waiter.resume(.retry)
            }
        }
    }

    /// Replay the account's pending entries oldest first until none is
    /// left or one must retry.
    private func drain(accountId: String) async {
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        while true {
            let head = try? await pool.read { db in
                try AppRecordOutboxEntity
                    .filter(Column("state") == "pending")
                    .order(Column("seq"))
                    .fetchOne(db)
            }
            guard let row = head else { return }
            let writeId = row.writeId
            guard let entry = row.entry else {
                // An unreadable row can never replay: it leaves.
                await Self.remove([writeId], pool: pool)
                continue
            }
            var replayed: AppRecordReplayed?
            let result: AppRecords.OutboxResult
            var message = ""
            do {
                replayed = try await api.recordsReplay(accountId: accountId, entry: entry)
                result = .ok
            } catch {
                result = AppRecords.outboxResult(error)
                message = error.trpcUserMessage
            }
            let outcome = AppRecords.outboxOutcome(entry, result: result)
            switch outcome {
            case .done:
                let now = AppRecords.outboxNow()
                _ = try? await pool.write { db in
                    try db.execute(
                        sql: #"UPDATE "app_record_outbox" SET "state" = 'acked', "acked_at" = ? WHERE "write_id" = ?"#,
                        arguments: [now, writeId]
                    )
                }
            case .retry:
                break
            case .dropped, .refused:
                await Self.remove([writeId], pool: pool)
            }
            let waiter = waiters.removeValue(forKey: writeId)
            if let waiter {
                switch outcome {
                case .done: waiter.resume(replayed.map { AppRecordOutboxAttempt.done($0) } ?? .retry)
                case .retry: waiter.resume(.retry)
                case .dropped: waiter.resume(.dropped)
                case .refused: waiter.resume(.refused(message: message))
                }
            } else if outcome == .refused {
                // A later replay's refusal: the failed-action toast.
                Toaster.shared.error(
                    FeedbackStates.Errors.title(verb: AppRecords.outboxVerb(entry), thing: "the record"),
                    description: message
                )
            }
            if outcome == .retry { return }
        }
    }

    /// Remove entries (a refusal, a drop, a pruned ack).
    nonisolated static func remove(_ writeIds: [String], pool: DatabasePool) async {
        guard !writeIds.isEmpty else { return }
        _ = try? await pool.write { db in
            for writeId in writeIds {
                try db.execute(sql: #"DELETE FROM "app_record_outbox" WHERE "write_id" = ?"#, arguments: [writeId])
            }
        }
    }

    /// The running app's header glyph: `outboxPendingLabel` of the account's
    /// entries for the app, on every outbox change (nil = no glyph).
    static func pendingLabels(accountId: String, appId: String, db: DatabaseManager, _ update: @escaping @MainActor (String?) -> Void) async {
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in
            try AppRecordOutboxEntity.filter(Column("app_id") == appId).order(Column("seq")).fetchAll(db)
        }
        do {
            for try await rows in observation.values(in: pool) {
                update(AppRecords.outboxPendingLabel(rows.compactMap(\.entry), appId: appId))
            }
        } catch {}
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
    /// #VAPP-230: the live upstream reads' last answers (the banners).
    let upstreams: AppUpstreamAnswers

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
        // EXP-1657: the aliases too, so a package published before the
        // `team_config` split keeps working.
        for name in AppActions.all + AppActions.aliases.keys.sorted() {
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
        readOnly: Bool = false,
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
        let scope = AppHostStore.RecordsScope(appId: appId, teamId: teamId, me: me, records: packages.first.map(JSONValue.parse)?["records"])
        let appsApi = deps.appsApi
        let upstreams = AppUpstreamAnswers()
        self.upstreams = upstreams

        // VAPP-157: `data` for the fixture's data image/audio/video (the
        // plugin's `resolveUrl` already narrowed every src to the media rule).
        let schemes = ["https"] + (base?.scheme == "http" ? ["http"] : []) + ["data"]
        let actions = Self.actionFunctions(accountId: accountId, base: base, httpClient: deps.httpClient)
        // VAPP-122: the app's own rows (no consent card); a refusal (the
        // kind's schema, the caps, the plan quota) = the failed-action toast,
        // and the call fails (`records.upsert` / `.delete`).
        // VAPP-235 (app-records.json `outbox`): every call is ONE outbox
        // entry (client ids for the write and every create), persisted
        // first; the only pending entry awaits its first replay (done = the
        // online answer, refused = the toast + the call fails, retry = the
        // entry's answer), behind others it answers from the entry at once.
        let outbox = deps.recordOutbox
        let recordsUpsert: HostFunction = { args, _ in
            guard let appId = scope.appId else { throw AppHostMcp.CallError(message: "No app to keep records for") }
            let queued = try await outbox.write(
                accountId: accountId, appId: appId, call: .upsert, args: args.compactMapValues { CardJSON(any: $0.any) }
            )
            switch queued.attempt {
            case let .done(.upserted(result))?:
                return ["id": result.id]
            case let .refused(message)?:
                Toaster.shared.error(
                    FeedbackStates.Errors.title(verb: AppRecords.outboxVerb(queued.entry), thing: "the record"),
                    description: message
                )
                throw AppHostMcp.CallError(message: message)
            default:
                return queued.entry.answer?.any
            }
        }
        let recordsDelete: HostFunction = { args, _ in
            guard let appId = scope.appId else { throw AppHostMcp.CallError(message: "No app to keep records for") }
            let queued = try await outbox.write(
                accountId: accountId, appId: appId, call: .delete, args: args.compactMapValues { CardJSON(any: $0.any) }
            )
            if case let .refused(message)? = queued.attempt {
                Toaster.shared.error(
                    FeedbackStates.Errors.title(verb: AppRecords.outboxVerb(queued.entry), thing: "the record"),
                    description: message
                )
                throw AppHostMcp.CallError(message: message)
            }
            return nil
        }
        // VAPP-194: a package action's writes as ONE transaction (the server
        // checks them again); a refusal lands nothing and is the action's
        // failure: `onActionFailed` shows the failed-action toast (not here,
        // so it shows once). VAPP-235: ONE outbox entry (all or nothing).
        let recordsBatch: HostFunction = { args, _ in
            guard let appId = scope.appId else { throw AppHostMcp.CallError(message: "No app to keep records for") }
            let queued = try await outbox.write(
                accountId: accountId, appId: appId, call: .batch, args: args.compactMapValues { CardJSON(any: $0.any) }
            )
            switch queued.attempt {
            case let .done(.batched(result))?:
                // VAPP-215: a package action's writes toast Undo for 10 s (app-records.json `history`).
                AppRecordUndo.toast(
                    .batch, appId: appId, eventIds: result.eventIds ?? [], via: "app",
                    accountId: accountId, api: appsApi, toaster: Toaster.shared
                )
                return ["ids": result.ids]
            case let .refused(message)?:
                throw AppHostMcp.CallError(message: message)
            default:
                return queued.entry.answer?.any
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
                "records.upsert": recordsUpsert,
                "records.delete": recordsDelete,
                "records.batch": recordsBatch,
            ]) { _, own in own },
            sources: [
                "exp": { source, emit in
                    switch source.name {
                    case "devices": return AppHostStore.devices(pool: pool, emit: emit)
                    case let name where AppSources.kinds.contains(name):
                        return AppHostStore.source(pool: pool, scope: scope, name: name, params: source.params, emit: emit)
                    default:
                        emit(.null)
                        return nil
                    }
                },
                // VAPP-161: `records:<kind>?…` (the source's name = the kind).
                "records": { source, emit in
                    guard let query = RecordsQuery.parse(source: source) else {
                        emit(.null)
                        return nil
                    }
                    return AppHostStore.records(pool: pool, scope: scope, query: query, emit: emit)
                },
                // #VAPP-230: `upstream:<server>/<tool>?…` (the broker's answer).
                AppUpstream.scheme: { source, emit in
                    ExpAppHost.upstream(
                        source: source, emit: emit, appId: scope.appId, accountId: accountId,
                        api: appsApi, answers: upstreams
                    )
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
            // VAPP-200: at app access `read` every surface is read-only.
            settings: readOnly ? SurfaceSettings(mode: .dark, readOnly: true) : nil,
            plugin: AppHostPlugin(origin: origin ?? ""),
            // VAPP-194: a package action runs as the viewer; one that landed
            // nothing = the failed-action toast. VAPP-201 (team-zone.json):
            // dates in the TEAM zone off the synced team row, else the device's.
            me: me,
            zone: TimeZoneOffset(TeamZone.hostTimeZone(AppHostStore.teamTimeZone(teamId: teamId, pool: pool))),
            onActionFailed: { failure in
                Toaster.shared.error(
                    FeedbackStates.Errors.title(verb: "save", thing: "the record"),
                    description: failure.message
                )
            }
        ))
    }

    /// #VAPP-230: one upstream binding: the broker's answer for the viewer,
    /// re-read every `cacheSeconds` while bound. Ok emits the value; a
    /// refusal or offline emits NOTHING (the path keeps the template's
    /// data: the binding's default); every answer feeds the banners.
    static func upstream(
        source: BindingSource,
        emit: @escaping (JSONValue?) -> Void,
        appId: String?,
        accountId: String,
        api: AppsApi,
        answers: AppUpstreamAnswers
    ) -> (() -> Void)? {
        guard let appId else { return nil }
        // The SDK hands a plain (non-Sendable) emit; the read loop runs on the
        // main actor, where the host calls it anyway.
        let send = UpstreamEmit(call: emit)
        let id = answers.nextId()
        let uri = source.uri
        let reads = Task { @MainActor in
            while !Task.isCancelled {
                do {
                    let answer = try await api.upstreamRead(accountId: accountId, appId: appId, source: uri)
                    guard !Task.isCancelled else { return }
                    answers.set(id, answer)
                    if case let .ok(value) = answer {
                        send.call(JSONValue.parse(value.compactString))
                    }
                } catch {
                    // Offline or refused: the binding keeps its default.
                }
                try? await Task.sleep(for: .seconds(AppUpstream.cacheSeconds))
            }
        }
        // The host unbinds on the main actor (the SDK host is MainActor).
        return {
            reads.cancel()
            MainActor.assumeIsolated { answers.set(id, nil) }
        }
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

    /// VAPP-122: the app whose records `records:<kind>` reads (the hosting app,
    /// or `app=<slug>`: another app of the same team) and the viewer.
    struct RecordsScope: Sendable {
        let appId: String?
        let teamId: String?
        let me: String?
        /// VAPP-214: the hosting package's `records` (its kinds' `computed`
        /// fields apply on read).
        var records: JSONValue? = nil
    }

    /// What one records read sees: the app's rows and their authors' names.
    private struct RecordsSnapshot: Sendable {
        var rows: [AppRecordEntity] = []
        var names: [String: String] = [:]
        /// VAPP-219 (R5): identifier → the summary an expanded issue ref inlines.
        var issues: [String: JSONValue] = [:]
        /// VAPP-235: this account's outbox entries for the app, oldest first
        /// (laid over `rows`: `AppRecords.outboxRows`).
        var entries: [AppRecords.OutboxEntry] = []
        /// VAPP-222 (R6): my team role (an owner takes every edge).
        var role: String?
    }

    /// `records:<kind>?…`: eUI's one evaluator (VAPP-189, the SDK's
    /// `RecordsQuery` over the core; fixtures/records-query.json) over the
    /// synced `app_records` rows of the app, names off `users`, on every
    /// change of the records, the users or the apps (an `app=` slug may sync
    /// later); a value equal to the last one is not re-emitted.
    static func records(
        pool: DatabasePool?, scope: RecordsScope, query: RecordsQuery, emit: @escaping (JSONValue?) -> Void
    ) -> (() -> Void)? {
        var last: String?
        // Dogfood G: the month view's today + default month in the TEAM zone (the day today() reads).
        let zone = TeamZone.hostTimeZone(AppHostStore.teamTimeZone(teamId: scope.teamId, pool: pool))
        // VAPP-219 (R5): the hosting app's refs; members expand by name, issues by identifier.
        let refs = query.app == nil ? RecordsQuery.refs(records: scope.records, kind: query.from) : .null
        let expanded = (query.structured["expand"]?.array ?? []).compactMap(\.string)
        let memberRefs = expanded.filter { refs[$0]?.string == "member" }
        let issueRefs = expanded.filter { refs[$0]?.string == "issue" }
        // VAPP-222 (R6): the hosting app's state machine: each row's legal next states.
        let transitions = query.app == nil ? RecordsQuery.transitions(records: scope.records, kind: query.from) : .null
        let push = { (snapshot: RecordsSnapshot) in
            // VAPP-235: the outbox laid over the synced rows (pending AND
            // acked, oldest first); acked entries the rows reflect (or past
            // the grace) leave the store.
            let synced = snapshot.rows.map { AppRecords.Row($0) }
            let kept = AppRecords.outboxPrune(snapshot.entries, rows: synced, now: AppRecords.outboxNow())
            if kept.count < snapshot.entries.count, let pool {
                let keptIds = Set(kept.map(\.writeId))
                let gone = snapshot.entries.map(\.writeId).filter { !keptIds.contains($0) }
                Task { await AppRecordOutbox.remove(gone, pool: pool) }
            }
            let rows = AppRecords.outboxRows(synced, entries: snapshot.entries, me: scope.me).map { row in
                RecordRow(
                    id: row.id,
                    kind: row.kind,
                    data: JSONValue.parse(CardJSON.object(row.data).compactString),
                    createdBy: row.createdBy,
                    authorId: row.authorId,
                    createdAt: row.createdAt,
                    updatedAt: row.updatedAt
                )
            }
            // VAPP-214: the hosting app's computed fields (another app's rows carry none).
            let computed = query.app == nil ? RecordsQuery.computed(records: scope.records, kind: query.from) : [:]
            let now = Date()
            let value = query.value(rows: rows, me: scope.me, now: now, today: TeamZone.day(now, in: zone), names: snapshot.names, computed: computed, refs: refs, issues: snapshot.issues, transitions: transitions, role: snapshot.role)
            let json = value.json
            guard json != last else { return }
            last = json
            emit(value)
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
            // VAPP-235: the account's queued writes for the app (the database
            // is per account), so a queued write re-emits the binding.
            let entries = try AppRecordOutboxEntity
                .filter(Column("app_id") == appId)
                .order(Column("seq"))
                .fetchAll(db)
                .compactMap(\.entry)
            var names: [String: String] = [:]
            // VAPP-187: `author` = the AUTHOR's name (`authorId ?? createdBy`);
            // VAPP-235: a queued create's author is the viewer.
            var wanted = Set(rows.compactMap { $0.authorId ?? $0.createdBy })
            if !entries.isEmpty, let me = scope.me { wanted.insert(me) }
            var identifiers: Set<String> = []
            if !memberRefs.isEmpty || !issueRefs.isEmpty {
                for row in rows where row.kind == query.from {
                    let data = JSONValue.parse(row.data)
                    for f in memberRefs { if let id = data[f]?.string, !id.isEmpty { wanted.insert(id) } }
                    for f in issueRefs { if let id = data[f]?.string, !id.isEmpty { identifiers.insert(id) } }
                }
            }
            let authors = Array(wanted)
            if !authors.isEmpty {
                for user in try UserEntity.filter(keys: authors).fetchAll(db) {
                    names[user.id] = user.name ?? ""
                }
            }
            var issues: [String: JSONValue] = [:]
            if !identifiers.isEmpty, let teamId = scope.teamId {
                let boardIds = try String.fetchAll(db, sql: #"SELECT "id" FROM "boards" WHERE "team_id" = ?"#, arguments: [teamId])
                for issue in try IssueEntity.filter(boardIds.contains(Column("board_id"))).filter(identifiers.contains(Column("identifier"))).fetchAll(db) {
                    guard let identifier = issue.identifier else { continue }
                    issues[identifier] = .object(["id": .string(issue.id), "identifier": .string(identifier), "title": .string(issue.title), "status": .string(issue.status)])
                }
            }
            var role: String?
            if transitions != .null, let me = scope.me, let teamId = scope.teamId {
                role = try TeamMemberEntity.filter(Column("team_id") == teamId && Column("user_id") == me).fetchOne(db)?.role
            }
            return RecordsSnapshot(rows: rows, names: names, issues: issues, entries: entries, role: role)
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
    /// VAPP-201: the synced team's `time_zone` (nil = none, or not synced).
    static func teamTimeZone(teamId: String?, pool: DatabasePool?) -> String? {
        guard let pool, let teamId else { return nil }
        return try? pool.read { db in try TeamEntity.fetchOne(db, key: teamId)?.timeZone }
    }

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

/// #VAPP-230: the binding's emit, carried into the main-actor read loop of
/// an `upstream:` source (the SDK's emit is not Sendable; it is only ever
/// called on the main actor).
private struct UpstreamEmit: @unchecked Sendable {
    let call: (JSONValue?) -> Void
}

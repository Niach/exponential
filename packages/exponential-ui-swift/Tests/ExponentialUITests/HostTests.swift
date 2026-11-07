import XCTest
import ExponentialUICore
@testable import ExponentialUI

/// The host API (VAPP-91) through `ExponentialHost`: router ops on real
/// surface models, sources + cancel, the function gate + consent + package
/// narrowing, the URL policy, media requests, the in-memory transport round
/// trip, actions as A2UI client messages, catalog negotiation.
@MainActor
final class HostTests: XCTestCase {
    let core = "https://ui.exponential.at/catalogs/core/v1"

    func msg(_ kind: String, _ body: JSONValue) -> String {
        JSONValue.object(["version": .string("v0.9"), kind: body]).json
    }

    func create(_ id: String, catalog: String? = nil) -> String {
        msg("createSurface", .object(["surfaceId": .string(id), "catalogId": .string(catalog ?? core)]))
    }

    func components(_ id: String, _ list: [JSONValue]) -> String {
        msg("updateComponents", .object(["surfaceId": .string(id), "components": .array(list)]))
    }

    /// Lets the host's detached sends / function tasks run.
    func settle() async {
        for _ in 0..<5 { await Task.yield() }
        try? await Task.sleep(for: .milliseconds(20))
    }

    /// Round 9 §6.5: a function the host lacks goes to the surface's agent and
    /// the press gets the answer (fixtures a2ui10-functions.json `sequences`).
    func testAFunctionTheHostLacksGoesToTheAgent() async throws {
        let transport = MemoryTransport()
        let host = ExponentialHost(HostOptions(transport: transport, agent: true))
        host.connect()
        host.receive(JSONValue.object(["version": .string("1.0"), "createSurface": .object(["surfaceId": .string("s1"), "catalogId": .string(core), "sendDataModel": .bool(true)])]))
        host.surface("s1")?.setData(path: "/name", value: .string("Ada"))
        let press = Task { await host.callFunction(FunctionCallInfo(surfaceId: "s1", componentId: "b", name: "lookup", args: ["q": .string("hi")])) }
        await settle()
        // `sendDataModel`: the metadata travels beside the message.
        let wrapped = try XCTUnwrap(transport.sentMessages.last)
        XCTAssertEqual(wrapped["metadata"]?["a2uiRendererDataModel"]?["surfaces"]?["s1"]?["name"], .string("Ada"))
        let sent = try XCTUnwrap(wrapped["message"]?["callAgentFunction"])
        XCTAssertEqual(sent["functionCallId"]?.string, "s1:1")
        XCTAssertEqual(sent["callFunction"]?["@call"]?.string, "lookup")
        XCTAssertEqual(sent["callFunction"]?["args"]?["q"], .string("hi"))
        host.receive(JSONValue.object(["version": .string("1.0"), "agentFunctionResponse": .object(["functionCallId": .string("s1:1"), "value": .number(42)])]))
        let outcome = await press.value
        XCTAssertEqual(outcome.decision, .allow)
        XCTAssertEqual(outcome.result as? JSONValue, .number(42))
        XCTAssertNil(outcome.error)
        // A press never shares a call.
        let again = Task { await host.callFunction(FunctionCallInfo(surfaceId: "s1", componentId: "b", name: "lookup", args: ["q": .string("hi")])) }
        await settle()
        XCTAssertEqual(transport.sentMessages.last?["message"]?["callAgentFunction"]?["functionCallId"]?.string, "s1:2")
        host.receive(JSONValue.object(["version": .string("1.0"), "agentFunctionResponse": .object(["functionCallId": .string("s1:2"), "error": .object(["code": .string("UNKNOWN_FUNCTION"), "message": .string("no")])])]))
        let refused = await again.value
        XCTAssertEqual(refused.error, "no")
        await settle()
        // The failure names the call, no surface: nothing to sync beside it.
        XCTAssertEqual(transport.sentMessages.last?["error"]?["code"]?.string, "EXPRESSION_ERROR")
    }

    /// Round 9 §6.8: an action's function runs first; its event fires only
    /// when the function did not fail.
    func testAFailedFunctionHoldsItsEvent() async throws {
        struct Boom: Error {}
        let transport = MemoryTransport()
        var order: [String] = []
        var release: CheckedContinuation<Void, Never>?
        var hold = false
        let host = ExponentialHost(HostOptions(
            transport: transport,
            functions: [
                "harness_ok": { _, _ in
                    if hold { await withCheckedContinuation { release = $0 } }
                    order.append("ok")
                    return nil
                },
                "harness_boom": { _, _ in throw Boom() },
                "harness_wipe": { _, _ in order.append("wipe"); return nil },
            ],
            policy: HostPolicy(functions: FunctionPolicy(deny: ["harness_wipe"]))
        ))
        host.connect()
        let names = ["harness_ok", "harness_boom", "harness_wipe", "harness_nope"]
        host.receive(JSONValue.object(["version": .string("1.0"), "createSurface": .object(["surfaceId": .string("s1"), "catalogId": .string(core)])]))
        let buttons: [JSONValue] = names.map { name in
            .object(["id": .string(name), "component": .string("Button"), "label": .string("x"), "on": .object(["press": .object(["functionCall": .object(["@call": .string(name)]), "event": .object(["name": .string("done_\(name)")])])])])
        }
        host.receive(JSONValue.object(["version": .string("1.0"), "updateComponents": .object(["surfaceId": .string("s1"), "components": .array([.object(["id": .string("root"), "component": .string("Box"), "children": .array(names.map { .string($0) })])] + buttons)])]))
        let m = try XCTUnwrap(host.surface("s1"))
        m.fixedMeasure = true
        m.setViewport(width: 390, height: 800)
        func events() -> [String] { transport.sentMessages.compactMap { $0["action"]?["name"]?.string } }

        // The event waits for the function.
        hold = true
        m.press(id: "harness_ok")
        await settle()
        XCTAssertEqual(events(), [])
        hold = false
        release?.resume()
        await settle()
        XCTAssertEqual(order, ["ok"])
        XCTAssertEqual(events(), ["done_harness_ok"])
        // A throw, a denial, an unknown name: the error, no event.
        m.press(id: "harness_boom")
        await settle()
        XCTAssertEqual(transport.sentMessages.last?["error"]?["code"]?.string, "EXECUTION_ERROR")
        m.press(id: "harness_wipe")
        await settle()
        XCTAssertEqual(transport.sentMessages.last?["error"]?["code"]?.string, "INVALID_FUNCTION_CALL")
        m.press(id: "harness_nope")
        await settle()
        XCTAssertEqual(transport.sentMessages.last?["error"]?["code"]?.string, "UNKNOWN_FUNCTION")
        XCTAssertEqual(order, ["ok"])
        XCTAssertEqual(events(), ["done_harness_ok"])
    }

    /// Round 9 §6.5: a surface without an agent (here a host's own) answers
    /// UNKNOWN_FUNCTION at the gate; a layout with nothing to report asks the
    /// hub nothing.
    func testNoAgentNoPressAndNoNeedlessFlush() async throws {
        let transport = MemoryTransport()
        let host = ExponentialHost(HostOptions(transport: transport, agent: true))
        host.connect()
        host.receive(JSONValue.object(["version": .string("1.0"), "createSurface": .object(["surfaceId": .string("own"), "catalogId": .string(core)])]), trusted: true)
        host.receive(JSONValue.object(["version": .string("1.0"), "updateComponents": .object(["surfaceId": .string("own"), "components": .array([.object(["id": .string("root"), "component": .string("Text"), "text": .string("hi")])])])]), trusted: true)
        let m = try XCTUnwrap(host.surface("own"))
        m.fixedMeasure = true
        m.setViewport(width: 390, height: 800)
        XCTAssertFalse(m.surface.needsFlush())
        let outcome = await host.callFunction(FunctionCallInfo(surfaceId: "own", componentId: "b", name: "lookup", args: [:]))
        XCTAssertEqual(outcome.decision, .notFound)
        await settle()
        XCTAssertEqual(transport.sentMessages.last?["error"]?["code"]?.string, "UNKNOWN_FUNCTION")
        XCTAssertEqual(transport.sentMessages.last?["error"]?["message"]?.string, "no function lookup")
        XCTAssertFalse(transport.sentMessages.contains { $0["callAgentFunction"] != nil })
    }

    /// Round 9 §6.5: a reactive position's call leaves after the layout, once,
    /// and its answer binds the surface again.
    func testAReactiveCallGoesToTheAgentAndItsAnswerRebinds() async throws {
        let transport = MemoryTransport()
        let host = ExponentialHost(HostOptions(transport: transport, agent: true))
        host.connect()
        host.receive(#"{"version":"1.0","createSurface":{"surfaceId":"s1","catalogId":"\#(core)"}}"#)
        host.receive(#"{"version":"1.0","updateComponents":{"surfaceId":"s1","components":[{"id":"root","component":"Text","text":{"@call":"lookup","args":{"q":"hi"}}}]}}"#)
        let model = try XCTUnwrap(host.surface("s1"))
        model.fixedMeasure = true
        model.setViewport(width: 300, height: 400)
        await settle()
        XCTAssertEqual(transport.sentMessages.count, 1, "\(transport.sent)")
        XCTAssertEqual(transport.sentMessages.first?["callAgentFunction"]?["functionCallId"]?.string, "s1:1")
        host.receive(#"{"version":"1.0","agentFunctionResponse":{"functionCallId":"s1:1","value":"Ada"}}"#)
        XCTAssertEqual(model.nodes.first?.props["text"], .string("Ada"))
        await settle()
        XCTAssertEqual(transport.sentMessages.count, 1)
    }

    func testRouterOpsThroughAHost() throws {
        let host = ExponentialHost()
        let ops = host.receive(create("s1"))
        XCTAssertEqual(ops.first?["op"]?.string, "create")
        XCTAssertEqual(host.surfaceIds, ["s1"])
        let model = try XCTUnwrap(host.surface("s1"))
        XCTAssertEqual(model.options.catalogId, core)
        host.receive(components("s1", [.object(["id": .string("root"), "component": .string("Text"), "text": .object(["path": .string("/name")])])]))
        let epoch = model.surface.writeEpoch()
        host.receive(msg("updateDataModel", .object(["surfaceId": .string("s1"), "path": .string("/name"), "value": .string("Ada")])))
        XCTAssertEqual(model.data["name"], .string("Ada"))
        // #VAPP-288: host data is not a programmatic write.
        XCTAssertEqual(model.surface.writeEpoch(), epoch)
        model.setViewport(width: 300, height: 400)
        XCTAssertEqual(model.nodes.first?.props["text"], .string("Ada"))
        // No `value` = remove the path.
        host.receive(msg("updateDataModel", .object(["surfaceId": .string("s1"), "path": .string("/name")])))
        XCTAssertNil(model.data["name"])
        // `""` = the whole model.
        host.receive(msg("updateDataModel", .object(["surfaceId": .string("s1"), "path": .string(""), "value": .object(["name": .string("Grace")])])))
        XCTAssertEqual(model.data["name"], .string("Grace"))
        // Round 9 §1.4: a live id is not created twice.
        host.receive(create("s1"))
        XCTAssertTrue(host.surface("s1") === model)
        host.receive(msg("deleteSurface", .object(["surfaceId": .string("s1")])))
        XCTAssertNil(host.surface("s1"))
        XCTAssertEqual(host.surfaceIds, [])
    }

    func testTheMemoryTransportRoundTrip() async throws {
        let transport = MemoryTransport()
        let host = ExponentialHost(HostOptions(transport: transport))
        XCTAssertTrue(host.hasTransport)
        transport.feed(create("s1"))
        XCTAssertEqual(host.status, .closed)
        host.connect()
        XCTAssertEqual(host.status, .open)
        XCTAssertNotNil(host.surface("s1"), "queued messages arrive at start")
        transport.feedJsonl("\(components("s1", [.object(["id": .string("root"), "component": .string("Text"), "text": .string("Hi")])]))\n\n{bad\n")
        let model = try XCTUnwrap(host.surface("s1"))
        model.setViewport(width: 300, height: 400)
        XCTAssertEqual(model.nodes.first?.props["text"], .string("Hi"))
        // A message to a ghost surface answers with SURFACE_NOT_FOUND.
        transport.feed(components("ghost", [.object(["id": .string("root"), "component": .string("Text"), "text": .string("Hi")])]))
        await settle()
        XCTAssertEqual(transport.sentMessages.last?["error"]?["code"], .string("SURFACE_NOT_FOUND"))
        host.close()
        XCTAssertEqual(host.surfaceIds, [])
    }

    func testAnActionBecomesTheA2UIClientMessage() async throws {
        let transport = MemoryTransport()
        let recorder = RecordingHost()
        let host = ExponentialHost(HostOptions(transport: transport, plugin: recorder))
        host.connect()
        host.receive(create("s1"))
        host.receive(components("s1", [.object(["id": .string("root"), "component": .string("Stack"), "children": .array([.string("go")])]), .object(["id": .string("go"), "component": .string("Button"), "label": .string("Go"), "on": .object(["press": .object(["event": .object(["name": .string("refresh"), "context": .object(["note": .object(["path": .string("/note")])])])])])])]))
        host.receive(msg("updateDataModel", .object(["surfaceId": .string("s1"), "path": .string("/note"), "value": .string("hello")])))
        let model = try XCTUnwrap(host.surface("s1"))
        model.setViewport(width: 300, height: 400)
        model.press(id: "go")
        await settle()
        let sent = try XCTUnwrap(transport.sentMessages.last)
        XCTAssertEqual(sent["version"], .string("1.0"))
        let action = try XCTUnwrap(sent["action"])
        XCTAssertEqual(action["name"], .string("refresh"))
        XCTAssertEqual(action["surfaceId"], .string("s1"))
        XCTAssertEqual(action["sourceComponentId"], .string("go"))
        XCTAssertEqual(action["context"], .object(["note": .string("hello")]))
        XCTAssertNotNil(action["timestamp"]?.string)
        // The base plugin still observes it.
        XCTAssertEqual(recorder.actions.map(\.name), ["refresh"])
    }

    /// VAPP-240: a surface's components read back raw (a `{path}` binding), and a
    /// file request names its surface.
    func testASurfaceReadsItsComponentsRaw() async throws {
        let host = ExponentialHost(HostOptions())
        host.receive(create("s1"))
        host.receive(msg("updateComponents", .object(["surfaceId": .string("s1"), "components": .array([
            .object(["id": .string("root"), "component": .string("FileUpload"), "files": .object(["path": .string("/draft/files")])]),
        ])])))
        let model = try XCTUnwrap(host.surface("s1"))
        XCTAssertEqual(model.components.first?["files"]?["@path"]?.string, "/draft/files")
        XCTAssertEqual(FilePickRequest(surfaceId: "s1", componentId: "root", accept: nil, multiple: false).surfaceId, "s1")
    }

    func testSourcesBindAndCancel() async throws {
        let transport = MemoryTransport()
        var emits: [String: (JSONValue?) -> Void] = [:]
        var cancelled: [String] = []
        let host = ExponentialHost(HostOptions(transport: transport, sources: [
            "EXP": { source, emit in
                emits[source.name] = emit
                XCTAssertEqual(source.params, ["team": "t1"])
                return { cancelled.append(source.name) }
            },
        ]))
        host.connect()
        host.receive(create("s1"))
        host.receive(msg("bindDataModel", .object(["surfaceId": .string("s1"), "path": .string("/devices"), "source": .string("exp:devices?team=t1")])), trusted: true)
        let emit = try XCTUnwrap(emits["devices"])
        emit(.array([.string("mac")]))
        XCTAssertEqual(host.surface("s1")?.data["devices"], .array([.string("mac")]))
        emit(nil)
        XCTAssertNil(host.surface("s1")?.data["devices"])
        // No resolver for the scheme: VALIDATION_FAILED back to the server.
        host.receive(msg("bindDataModel", .object(["surfaceId": .string("s1"), "path": .string("/x"), "source": .string("other:thing")])), trusted: true)
        await settle()
        let err = try XCTUnwrap(transport.sentMessages.last?["error"])
        XCTAssertEqual(err["code"], .string("VALIDATION_FAILED"))
        XCTAssertEqual(err["message"], .string("no resolver for the source scheme other"))
        XCTAssertEqual(err["path"], .string("/x"))
        // Delete cancels the surface's subscriptions; a re-create too.
        host.receive(msg("deleteSurface", .object(["surfaceId": .string("s1")])))
        XCTAssertEqual(cancelled, ["devices"])
    }

    /// VAPP-122: the `bind` action re-follows a path from an action (the
    /// source with its params appended): the earlier subscription at that
    /// path is cancelled first, another path's stays.
    func testBindActionRefollowsAPath() async throws {
        var bound: [String] = []
        var cancelled: [String] = []
        let resolver: SourceResolver = { source, emit in
            bound.append(source.uri)
            emit(.string(source.params["month"] ?? ""))
            return { cancelled.append(source.uri) }
        }
        let host = ExponentialHost(HostOptions(sources: ["exp": resolver, "records": resolver]))
        host.receive(create("s1"))
        host.receive(msg("bindDataModel", .object(["surfaceId": .string("s1"), "path": .string("/other"), "source": .string("exp:devices")])), trusted: true)
        func bind(_ month: String, on surface: String = "s1") async -> FunctionOutcome {
            await host.callFunction(SurfaceFunctionCall(surfaceId: surface, componentId: "c", name: "bind", args: [
                "path": .string("/records"),
                "source": .string("records:entry"),
                "params": .object(["month": .string(month), "skip": .null]),
            ]))
        }
        var out = await bind("2026-10")
        XCTAssertEqual(out.decision, .allow)
        XCTAssertEqual(host.surface("s1")?.data["records"], .string("2026-10"))
        out = await bind("2026-11")
        XCTAssertEqual(out.decision, .allow)
        XCTAssertEqual(bound, ["exp:devices", "records:entry?month=2026-10", "records:entry?month=2026-11"])
        XCTAssertEqual(cancelled, ["records:entry?month=2026-10"])
        XCTAssertEqual(host.surface("s1")?.data["records"], .string("2026-11"))
        // No such surface, or no source: nothing ran.
        out = await bind("2026-12", on: "nope")
        XCTAssertEqual(out.decision, .deny)
        out = await host.callFunction(SurfaceFunctionCall(surfaceId: "s1", componentId: "c", name: "bind", args: ["path": .string("/x")]))
        XCTAssertEqual(out.decision, .deny)
        host.receive(msg("deleteSurface", .object(["surfaceId": .string("s1")])))
        XCTAssertEqual(Set(cancelled), ["records:entry?month=2026-10", "records:entry?month=2026-11", "exp:devices"])
    }

    /// `sourceWithParams` replays host-policy.json `sourceParams`.
    func testSourceWithParamsReplaysTheFixture() throws {
        let cases = try XCTUnwrap(try Fixtures.json("host-policy.json")["sourceParams"]?.array)
        XCTAssertFalse(cases.isEmpty)
        XCTAssertEqual(BindingSource.sourceWithParams("records:entry", params: .object(["month": .string("2026-10")])), "records:entry?month=2026-10")
        XCTAssertEqual(BindingSource.sourceWithParams("exp:x", params: nil), "exp:x")
        for item in cases {
            let source = try XCTUnwrap(item["source"]?.string)
            XCTAssertEqual(BindingSource.sourceWithParams(source, params: item["params"]), item["expected"]?.string, source)
        }
    }

    /// Round 9 §12: a formatVersion 5 package is installed UPGRADED, and the
    /// gate narrows with the upgraded `functions` (`records.*` →
    /// `records_*`), or every flat call its template makes is denied (an
    /// app's record form on the phones, #VAPP-272).
    func testAFormat5PackageNarrowsWithItsUpgradedFunctions() throws {
        let package = JSONValue.object([
            "formatVersion": .number(5), "id": .string("acme.v5"), "name": .string("V5"), "version": .string("1.0.0"),
            "catalogId": .string(core), "functions": .array([.string("records.*")]),
            "templates": .object(["main": .object(["components": .array([
                .object(["id": .string("root"), "component": .string("Text"), "text": .string("hi")]),
            ])])]),
        ])
        let host = ExponentialHost(HostOptions(
            transport: MemoryTransport(),
            functions: ["records_upsert": { _, _ in nil }, "harness_toast": { _, _ in nil }],
            packages: [package.json]
        ))
        host.connect()
        host.receive(msg("applyTemplate", .object(["surfaceId": .string("w1"), "templateId": .string("main"), "packageId": .string("acme.v5")])), trusted: true)
        XCTAssertNotNil(host.surface("w1"))
        XCTAssertEqual(host.decide(surfaceId: "w1", name: "records_upsert"), .allow)
        XCTAssertEqual(host.decide(surfaceId: "w1", name: "harness_toast"), .deny)
    }

    func testFunctionGateConsentAndPackageNarrowing() async throws {
        let transport = MemoryTransport()
        var ran: [String] = []
        var asked: [String] = []
        var consent = false
        let package = try Fixtures.json("host-router.json")["packages"]!["acme.devices"]!
        let host = ExponentialHost(HostOptions(
            transport: transport,
            functions: [
                "harness_toast": { args, _ in ran.append("toast:\(args["text"]?.string ?? "")"); return nil },
                "harness_wipe": { _, _ in ran.append("wipe"); return nil },
                "app_save": { _, _ in ran.append("save"); return true },
            ],
            sources: ["exp": { _, _ in nil }],
            packages: [package.json],
            policy: HostPolicy(functions: FunctionPolicy(ask: ["app_*"], deny: ["harness_wipe"]), onFunctionCall: { call in asked.append(call.name); return consent })
        ))
        host.connect()
        host.receive(create("s1"))
        func call(_ name: String, on surface: String = "s1", args: Props = [:]) async -> FunctionOutcome {
            await host.callFunction(SurfaceFunctionCall(surfaceId: surface, componentId: "c", name: name, args: args))
        }
        var out = await call("harness_toast", args: ["text": .string("hi")])
        XCTAssertEqual(out.decision, .allow)
        XCTAssertEqual(ran, ["toast:hi"])
        out = await call("harness_wipe")
        XCTAssertEqual(out.decision, .deny)
        await settle()
        XCTAssertEqual(transport.sentMessages.last?["error"]?["code"], .string("INVALID_FUNCTION_CALL"))
        XCTAssertEqual(transport.sentMessages.last?["error"]?["message"], .string("harness_wipe was not allowed"))
        out = await call("app_save")
        XCTAssertEqual(out.decision, .deny, "ask without consent = deny")
        XCTAssertEqual(asked, ["app_save"])
        consent = true
        out = await call("app_save")
        XCTAssertEqual(out.decision, .allow)
        XCTAssertEqual(out.result as? Bool, true)
        out = await call("nope")
        XCTAssertEqual(out.decision, .notFound)
        await settle()
        XCTAssertEqual(transport.sentMessages.last?["error"]?["code"], .string("UNKNOWN_FUNCTION"))
        XCTAssertEqual(transport.sentMessages.last?["error"]?["message"], .string("no function nope"))
        // A template surface: its package's `functions` narrow the gate.
        let ops = host.receive(msg("applyTemplate", .object(["surfaceId": .string("d1"), "templateId": .string("list"), "packageId": .string("acme.devices")])), trusted: true)
        XCTAssertEqual(ops.map { $0["op"]?.string ?? "" }, ["create", "data", "components", "bind"])
        XCTAssertEqual(host.surface("d1")?.data["title"], .string("Devices"))
        XCTAssertEqual(host.decide(surfaceId: "d1", name: "harness_toast"), .allow)
        XCTAssertEqual(host.decide(surfaceId: "d1", name: "app_save"), .deny)
        XCTAssertEqual(host.decide(surfaceId: "s1", name: "app_save"), .ask)
        ran = []
        out = await call("app_save", on: "d1")
        XCTAssertEqual(out.decision, .deny)
        XCTAssertEqual(ran, [])
    }

    /// Round 9 §1.4: a re-open mints, applies, and only then deletes the old surface.
    func testAReopenedSurfacePaintsUnderAMintedId() async throws {
        let transport = MemoryTransport()
        let package = try Fixtures.json("host-router.json")["packages"]!["acme.devices"]!
        let host = ExponentialHost(HostOptions(transport: transport, packages: [package.json]))
        host.connect()
        func apply(_ id: String) {
            host.receive(msg("applyTemplate", .object(["surfaceId": .string(id), "templateId": .string("list"), "packageId": .string("acme.devices")])), trusted: true)
        }
        func delete(_ id: String) { host.receive(msg("deleteSurface", .object(["surfaceId": .string(id)]))) }
        let first = host.mintSurfaceId("app")
        apply(first)
        XCTAssertNotNil(host.surface(first))
        let second = host.mintSurfaceId("app")
        XCTAssertNotEqual(first, second)
        apply(second)
        XCTAssertNotNil(host.surface(first), "the old surface paints until the new one exists")
        delete(first)
        XCTAssertNil(host.surface(first))
        XCTAssertNotNil(host.surface(second))
        await settle()
        XCTAssertFalse(transport.sentMessages.contains { $0["error"]?["code"] == .string("INTEGRITY_ERROR") })
        // The fixed id the hosts used to re-apply is refused for good.
        apply(first)
        await settle()
        XCTAssertEqual(transport.sentMessages.last?["error"]?["code"], .string("INTEGRITY_ERROR"))
        XCTAssertNil(host.surface(first))
    }

    func testAFunctionCallActionReachesTheGate() async throws {
        let transport = MemoryTransport()
        var got: [Props] = []
        let host = ExponentialHost(HostOptions(transport: transport, functions: ["harness_toast": { args, _ in got.append(args); return nil }]))
        host.connect()
        host.receive(create("s1"))
        host.receive(components("s1", [.object(["id": .string("root"), "component": .string("Stack"), "children": .array([.string("b")])]), .object(["id": .string("b"), "component": .string("Button"), "label": .string("Toast"), "on": .object(["press": .object(["functionCall": .object(["call": .string("harness_toast"), "args": .object(["text": .object(["path": .string("/t")])])])])])])]))
        host.receive(msg("updateDataModel", .object(["surfaceId": .string("s1"), "path": .string("/t"), "value": .string("saved")])))
        let model = try XCTUnwrap(host.surface("s1"))
        model.setViewport(width: 300, height: 400)
        model.press(id: "b")
        await settle()
        XCTAssertEqual(got, [["text": .string("saved")]])
    }

    func testUrlPolicyAndMediaRequests() throws {
        var opened: [URL] = []
        let host = ExponentialHost(HostOptions(policy: HostPolicy(
            urls: UrlPolicy(hosts: ["*.exponential.at"]),
            openUrl: { opened.append($0) },
            media: MediaOptions(baseUrl: "https://app.exponential.at", rules: [.init(prefix: "https://app.exponential.at/api/attachments/", headers: ["authorization": "Bearer expu_test"])])
        )))
        XCTAssertTrue(host.openURL("https://ui.exponential.at/docs"))
        XCTAssertFalse(host.openURL("https://evil.example"))
        XCTAssertFalse(host.openURL("javascript:alert(1)"))
        // Relative urls resolve against the media base.
        XCTAssertTrue(host.openURL("/t/acme"))
        XCTAssertEqual(opened.map(\.absoluteString), ["https://ui.exponential.at/docs", "https://app.exponential.at/t/acme"])
        let r = try XCTUnwrap(host.mediaRequest("/api/attachments/a1?poster=1"))
        XCTAssertEqual(r.url?.absoluteString, "https://app.exponential.at/api/attachments/a1?poster=1")
        XCTAssertEqual(r.value(forHTTPHeaderField: "authorization"), "Bearer expu_test")
        let other = try XCTUnwrap(host.mediaRequest("https://cdn.example/x.png"))
        XCTAssertNil(other.value(forHTTPHeaderField: "authorization"))
        // The painter's loader goes through the host.
        XCTAssertEqual(host.plugin.mediaRequest("/api/attachments/a2")?.value(forHTTPHeaderField: "authorization"), "Bearer expu_test")
    }

    func testCatalogNegotiationAndUnsupportedCatalog() async throws {
        let transport = MemoryTransport()
        let ext = try Fixtures.json("catalog-extension.json")["extension"]!
        let host = ExponentialHost(HostOptions(transport: transport, extensions: [HostExtension(json: ext.json)]))
        host.connect()
        let extId = ext["id"]!.string!
        XCTAssertEqual(host.supportedCatalogIds, supportedCatalogIdsFor(extensionIds: [extId]))
        XCTAssertEqual(host.supportedCatalogIds.last, extId)
        XCTAssertEqual(host.rendererCapabilities["1.0"]?["supportedCatalogIds"]?.array?.count, host.supportedCatalogIds.count)
        host.receive(create("e1", catalog: extId))
        XCTAssertNotNil(host.surface("e1"))
        XCTAssertNil(host.unsupportedCatalog)
        host.receive(create("s9", catalog: "https://ui.exponential.at/catalogs/core/v2"))
        XCTAssertNil(host.surface("s9"))
        XCTAssertEqual(host.unsupportedCatalog, "https://ui.exponential.at/catalogs/core/v2")
        await settle()
        XCTAssertEqual(transport.sentMessages.last?["error"]?["code"], .string("UNSUPPORTED_CATALOG"))
    }
}

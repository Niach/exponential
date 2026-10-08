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

    func testRouterOpsThroughAHost() throws {
        let host = ExponentialHost()
        let ops = host.receive(create("s1"))
        XCTAssertEqual(ops.first?["op"]?.string, "create")
        XCTAssertEqual(host.surfaceIds, ["s1"])
        let model = try XCTUnwrap(host.surface("s1"))
        XCTAssertEqual(model.options.catalogId, core)
        host.receive(components("s1", [.object(["id": .string("root"), "component": .string("Text"), "text": .object(["path": .string("/name")])])]))
        host.receive(msg("updateDataModel", .object(["surfaceId": .string("s1"), "path": .string("/name"), "value": .string("Ada")])))
        XCTAssertEqual(model.data["name"], .string("Ada"))
        model.setViewport(width: 300, height: 400)
        XCTAssertEqual(model.nodes.first?.props["text"], .string("Ada"))
        // No `value` = remove the path.
        host.receive(msg("updateDataModel", .object(["surfaceId": .string("s1"), "path": .string("/name")])))
        XCTAssertNil(model.data["name"])
        // `""` = the whole model.
        host.receive(msg("updateDataModel", .object(["surfaceId": .string("s1"), "path": .string(""), "value": .object(["name": .string("Grace")])])))
        XCTAssertEqual(model.data["name"], .string("Grace"))
        // A second create replaces the model with an empty one.
        host.receive(create("s1"))
        XCTAssertFalse(host.surface("s1") === model)
        XCTAssertEqual(host.surface("s1")?.nodes.count, 0)
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
        transport.feed(components("ghost", []))
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
        XCTAssertEqual(sent["version"], .string("v0.9"))
        let action = try XCTUnwrap(sent["action"])
        XCTAssertEqual(action["name"], .string("refresh"))
        XCTAssertEqual(action["surfaceId"], .string("s1"))
        XCTAssertEqual(action["sourceComponentId"], .string("go"))
        XCTAssertEqual(action["context"], .object(["note": .string("hello")]))
        XCTAssertNotNil(action["timestamp"]?.string)
        // The base plugin still observes it.
        XCTAssertEqual(recorder.actions.map(\.name), ["refresh"])
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
        host.receive(msg("bindDataModel", .object(["surfaceId": .string("s1"), "path": .string("/devices"), "source": .string("exp:devices?team=t1")])))
        let emit = try XCTUnwrap(emits["devices"])
        emit(.array([.string("mac")]))
        XCTAssertEqual(host.surface("s1")?.data["devices"], .array([.string("mac")]))
        emit(nil)
        XCTAssertNil(host.surface("s1")?.data["devices"])
        // No resolver for the scheme: VALIDATION_FAILED back to the server.
        host.receive(msg("bindDataModel", .object(["surfaceId": .string("s1"), "path": .string("/x"), "source": .string("other:thing")])))
        await settle()
        let err = try XCTUnwrap(transport.sentMessages.last?["error"])
        XCTAssertEqual(err["code"], .string("VALIDATION_FAILED"))
        XCTAssertEqual(err["message"], .string("no resolver for the source scheme other"))
        XCTAssertEqual(err["path"], .string("/x"))
        // Delete cancels the surface's subscriptions; a re-create too.
        host.receive(msg("deleteSurface", .object(["surfaceId": .string("s1")])))
        XCTAssertEqual(cancelled, ["devices"])
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
                "harness.toast": { args, _ in ran.append("toast:\(args["text"]?.string ?? "")"); return nil },
                "harness.wipe": { _, _ in ran.append("wipe"); return nil },
                "app.save": { _, _ in ran.append("save"); return true },
            ],
            sources: ["exp": { _, _ in nil }],
            packages: [package.json],
            policy: HostPolicy(functions: FunctionPolicy(ask: ["app.*"], deny: ["harness.wipe"]), onFunctionCall: { call in asked.append(call.name); return consent })
        ))
        host.connect()
        host.receive(create("s1"))
        func call(_ name: String, on surface: String = "s1", args: Props = [:]) async -> FunctionOutcome {
            await host.callFunction(SurfaceFunctionCall(surfaceId: surface, componentId: "c", name: name, args: args))
        }
        var out = await call("harness.toast", args: ["text": .string("hi")])
        XCTAssertEqual(out.decision, .allow)
        XCTAssertEqual(ran, ["toast:hi"])
        out = await call("harness.wipe")
        XCTAssertEqual(out.decision, .deny)
        await settle()
        XCTAssertEqual(transport.sentMessages.last?["error"]?["code"], .string("FUNCTION_DENIED"))
        XCTAssertEqual(transport.sentMessages.last?["error"]?["message"], .string("harness.wipe was not allowed"))
        out = await call("app.save")
        XCTAssertEqual(out.decision, .deny, "ask without consent = deny")
        XCTAssertEqual(asked, ["app.save"])
        consent = true
        out = await call("app.save")
        XCTAssertEqual(out.decision, .allow)
        XCTAssertEqual(out.result as? Bool, true)
        out = await call("nope")
        XCTAssertEqual(out.decision, .notFound)
        await settle()
        XCTAssertEqual(transport.sentMessages.last?["error"]?["code"], .string("FUNCTION_NOT_FOUND"))
        XCTAssertEqual(transport.sentMessages.last?["error"]?["message"], .string("no function nope"))
        // A template surface: its package's `functions` narrow the gate.
        let ops = host.receive(msg("applyTemplate", .object(["surfaceId": .string("d1"), "templateId": .string("list"), "packageId": .string("acme.devices")])))
        XCTAssertEqual(ops.map { $0["op"]?.string ?? "" }, ["create", "components", "data", "bind"])
        XCTAssertEqual(host.surface("d1")?.data["title"], .string("Devices"))
        XCTAssertEqual(host.decide(surfaceId: "d1", name: "harness.toast"), .allow)
        XCTAssertEqual(host.decide(surfaceId: "d1", name: "app.save"), .deny)
        XCTAssertEqual(host.decide(surfaceId: "s1", name: "app.save"), .ask)
        ran = []
        out = await call("app.save", on: "d1")
        XCTAssertEqual(out.decision, .deny)
        XCTAssertEqual(ran, [])
    }

    func testAFunctionCallActionReachesTheGate() async throws {
        let transport = MemoryTransport()
        var got: [Props] = []
        let host = ExponentialHost(HostOptions(transport: transport, functions: ["harness.toast": { args, _ in got.append(args); return nil }]))
        host.connect()
        host.receive(create("s1"))
        host.receive(components("s1", [.object(["id": .string("root"), "component": .string("Stack"), "children": .array([.string("b")])]), .object(["id": .string("b"), "component": .string("Button"), "label": .string("Toast"), "on": .object(["press": .object(["functionCall": .object(["call": .string("harness.toast"), "args": .object(["text": .object(["path": .string("/t")])])])])])])]))
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
        XCTAssertEqual(host.clientCapabilities["v0.9"]?["supportedCatalogIds"]?.array?.count, host.supportedCatalogIds.count)
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

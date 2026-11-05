import XCTest
import ExponentialUICore
@testable import ExponentialUI

/// VAPP-194: `fixtures/actions.json` (`effects`, `check`, `validate`, `run`,
/// `batch`) through `Actions` (the core's one action runner over the FFI),
/// plus the host's all-or-nothing commit of `actions.<name>`. The TS
/// reference, the Rust core, the FFI facade and Compose's `ActionsTest`
/// replay the same cases.
@MainActor
final class ActionsTests: XCTestCase {
    final class FixedZone: HostZone, @unchecked Sendable {
        let offset: Int32
        init(_ offset: Int32) { self.offset = offset }
        func offsetMinutes(epochMs: Double) -> Int32 { offset }
    }

    struct Refused: LocalizedError {
        let message: String
        var errorDescription: String? { message }
    }

    func same(_ got: JSONValue, _ want: JSONValue, _ name: String) {
        XCTAssertTrue(jsonEqual(a: got.json, b: want.json), "\(name): got \(got.json.prefix(300)), want \(want.json.prefix(300))")
    }

    func want(_ c: JSONValue, key: String) -> JSONValue {
        if let e = c["error"] { return .object(["error": e]) }
        return .object([key: c[key] ?? .null])
    }

    /// A validate case's package: the base with its `with` keys replaced
    /// (`addActions` = actions added to the base's).
    static func validateCasePackage(_ base: JSONValue, _ with: JSONValue?) -> JSONValue {
        var pkg = base.object ?? [:]
        for (k, v) in with?.object ?? [:] where k != "addActions" { pkg[k] = v }
        if let add = with?["addActions"]?.object {
            var actions = base["actions"]?.object ?? [:]
            for (k, v) in add { actions[k] = v }
            pkg["actions"] = .object(actions)
        }
        return .object(pkg)
    }

    func testEffects() throws {
        let f = try Fixtures.json("actions.json")
        let cases = f["effects"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"]?.string ?? ""
            XCTAssertEqual(Actions.effect(name).rawValue, c["effect"]?.string, "effect: \(name)")
        }
    }

    func testCheck() throws {
        let f = try Fixtures.json("actions.json")
        for c in f["check"]?.array ?? [] {
            let got = try checkExpressionJson(expr: c["expr"]?.string ?? "", typesJson: "{}")
            same(JSONValue.parse(got), want(c, key: "type"), "check: \(c["name"]?.string ?? "")")
        }
    }

    func testValidate() throws {
        let f = try Fixtures.json("actions.json")
        let base = f["package"] ?? .null
        for c in f["validate"]?.array ?? [] {
            let pkg = Self.validateCasePackage(base, c["with"])
            let got = try validatePackageJson(packageJson: pkg.json, catalogIds: nil)
            same(JSONValue.parse(got), c["issues"] ?? .array([]), "validate: \(c["name"]?.string ?? "")")
        }
    }

    func testRun() throws {
        let f = try Fixtures.json("actions.json")
        let base = f["package"] ?? .null
        let now = ExpressionsTests.instant(f["env"]?["now"]?.string ?? "")
        let zone = FixedZone(Int32(f["env"]?["offset"]?.number ?? 0))
        let cases = f["run"]?.array ?? []
        XCTAssertGreaterThan(cases.count, 10)
        for c in cases {
            let got = Actions.run(
                base, action: c["action"]?.string ?? "", params: c["params"], data: c["data"], me: c["me"]?.string,
                nowMs: now, zone: zone, maxSteps: c["maxSteps"]?.number.map { UInt32($0) }
            )
            let json: JSONValue
            switch got {
            case let .plan(plan): json = .object(["plan": plan.json])
            case let .error(e): json = .object(["error": .string(e)])
            }
            same(json, want(c, key: "plan"), "run: \(c["name"]?.string ?? "")")
        }
    }

    func testBatch() throws {
        let b = try XCTUnwrap(try Fixtures.json("actions.json")["batch"])
        for c in b["cases"]?.array ?? [] {
            let got = Actions.applyBatch(
                rows: b["rows"]?.array ?? [], writes: c["writes"]?.array ?? [], records: b["records"] ?? .null,
                me: b["me"]?.string, now: b["now"]?.string ?? "", newIds: (b["newIds"]?.array ?? []).compactMap(\.string)
            )
            let json: JSONValue
            switch got {
            case let .applied(rows, ids): json = .object(["rows": .array(rows), "ids": .array(ids.map(JSONValue.string))])
            case let .error(e): json = .object(["error": .string(e)])
            }
            let expected: JSONValue = c["error"].map { .object(["error": $0]) }
                ?? .object(["rows": c["rows"] ?? .null, "ids": c["ids"] ?? .null])
            same(json, expected, "batch: \(c["name"]?.string ?? "")")
        }
    }

    // MARK: - a package action on a host

    @MainActor
    final class Surface {
        let host: ExponentialHost
        var rows: [JSONValue]
        var batches = 0
        var toasts: [String] = []
        var failed: [String] = []
        var consent = true

        init(_ f: JSONValue) {
            let b = f["batch"] ?? .null
            let pkg = f["package"] ?? .null
            rows = b["rows"]?.array ?? []
            let me = b["me"]?.string
            let stamp = b["now"]?.string ?? ""
            let records = pkg["records"] ?? .null
            weak var box: Surface?
            let host = ExponentialHost(HostOptions(
                functions: [
                    Actions.batchFunction: { args, _ in
                        guard let s = box else { return nil }
                        let fresh = (0..<10).map { "new\(s.batches)-\($0)" }
                        switch Actions.applyBatch(rows: s.rows, writes: args["writes"]?.array ?? [], records: records, me: me, now: stamp, newIds: fresh) {
                        case let .error(e): throw Refused(message: e)
                        case let .applied(rows, ids):
                            s.rows = rows
                            s.batches += 1
                            return ["ids": ids]
                        }
                    },
                    "harness.toast": { args, _ in
                        box?.toasts.append(args["message"]?.displayText ?? "")
                        return nil
                    },
                ],
                packages: [pkg.json],
                policy: HostPolicy(functions: FunctionPolicy(ask: ["harness.toast"]), onFunctionCall: { _ in box?.consent ?? false }),
                me: me,
                now: { Date(timeIntervalSince1970: ExpressionsTests.instant(f["env"]?["now"]?.string ?? "") / 1000) },
                zone: FixedZone(Int32(f["env"]?["offset"]?.number ?? 0)),
                onActionFailed: { failure in box?.failed.append(failure.message) }
            ))
            self.host = host
            box = self
            host.receive(JSONValue.object([
                "version": .string("v0.9"),
                "applyTemplate": .object(["surfaceId": .string("s"), "packageId": pkg["id"] ?? .null, "templateId": .string("month")]),
            ]))
        }

        var formHours: JSONValue? { host.surface("s")?.data["form"]?["hours"] }

        func call(_ name: String, _ args: Props) async -> FunctionOutcome {
            await host.callFunction(SurfaceFunctionCall(surfaceId: "s", componentId: "log", name: name, args: args))
        }
    }

    func testThePlanLandsOneBatchTheSetsThenTheCalls() async throws {
        let s = Surface(try Fixtures.json("actions.json"))
        XCTAssertNotNil(s.host.surface("s"))
        let out = await s.call("actions.log", ["date": .string("2026-11-03"), "hours": .number(2)])
        XCTAssertNil(out.error)
        XCTAssertEqual(s.batches, 1)
        XCTAssertTrue(s.rows.contains { $0["data"] == .object(["date": .string("2026-11-03"), "hours": .number(2)]) })
        XCTAssertEqual(s.formHours, .number(0))
        XCTAssertEqual(s.toasts, ["Logged 2:00"])
    }

    func testAFalseRequireLandsNothingAndIsTheFailuresMessage() async throws {
        let s = Surface(try Fixtures.json("actions.json"))
        let before = s.rows
        let out = await s.call("actions.log", ["date": .string("2026-11-03"), "hours": .number(0)])
        XCTAssertEqual(out.error, "Log more than 0 hours")
        XCTAssertEqual(s.failed, ["Log more than 0 hours"])
        XCTAssertEqual(s.rows, before)
        XCTAssertEqual(s.formHours, .number(7.5))
    }

    func testARefusedWriteLandsNothingTheBatchIsOneUnit() async throws {
        let s = Surface(try Fixtures.json("actions.json"))
        let before = s.rows
        let out = await s.call("actions.swap", ["a": .string("e1"), "b": .string("missing")])
        XCTAssertEqual(out.error, "write 1: record missing not found")
        XCTAssertEqual(s.rows, before)
    }

    func testAHostCallThePersonRefusesLandsNothing() async throws {
        let s = Surface(try Fixtures.json("actions.json"))
        s.consent = false
        let before = s.rows
        let out = await s.call("actions.log", ["date": .string("2026-11-03"), "hours": .number(2)])
        XCTAssertEqual(out.error, "harness.toast was not allowed")
        XCTAssertEqual(s.rows, before)
        XCTAssertEqual(s.batches, 0)
        XCTAssertEqual(s.formHours, .number(7.5))
    }

    /// VAPP-205: `hostCalls`: a `call` step reaches the same built-ins a press does.
    func testAnActionStepRunsBindAndOpenUrl() async throws {
        let f = try XCTUnwrap(try Fixtures.json("actions.json")["hostCalls"])
        let pkg = f["package"] ?? .null
        for c in f["cases"]?.array ?? [] {
            let name = c["name"]?.string ?? ""
            var opened: [String] = []
            var bound: [String: String]?
            let host = ExponentialHost(HostOptions(
                sources: ["records": { source, emit in
                    bound = source.params
                    emit(.array([.string("bound")]))
                    return nil
                }],
                packages: [pkg.json],
                policy: HostPolicy(openUrl: { opened.append($0.absoluteString) })
            ))
            host.receive(JSONValue.object([
                "version": .string("v0.9"),
                "applyTemplate": .object(["surfaceId": .string("s"), "packageId": pkg["id"] ?? .null, "templateId": .string("main")]),
            ]))
            let args = c["args"]?.object ?? [:]
            let out = await host.callFunction(SurfaceFunctionCall(surfaceId: "s", componentId: "b", name: "actions.\(c["action"]?.string ?? "")", args: args))
            XCTAssertEqual(out.error, c["error"]?.string, name)
            XCTAssertEqual(opened, (c["opened"]?.array ?? []).compactMap(\.string), name)
            XCTAssertEqual(bound?["day"], c["bound"]?["day"]?.string, name)
            if c["bound"] != nil { XCTAssertEqual(host.surface("s")?.data["rows"], .array([.string("bound")]), name) }
        }
    }
}

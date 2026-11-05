import XCTest
import ExponentialUICore
@testable import ExponentialUI

/// VAPP-191: `fixtures/expressions.json` (`eval`, `check`, `packages`)
/// through `Expressions` (the core's one evaluator over the FFI). The TS
/// reference, the Rust core, the FFI facade and Compose's `ExpressionsTest`
/// replay the same cases; the team zone is a `HostZone` built from the
/// fixture's transitions (and checked against Foundation's IANA zone).
final class ExpressionsTests: XCTestCase {
    final class FixtureZone: HostZone, @unchecked Sendable {
        let base: Int32
        let steps: [(Double, Int32)]
        init(_ z: JSONValue?) {
            base = Int32(z?["offset"]?.number ?? 0)
            steps = (z?["transitions"]?.array ?? []).map { t in (ExpressionsTests.instant(t.array?[0].string ?? ""), Int32(t.array?[1].number ?? 0)) }
        }
        func offsetMinutes(epochMs: Double) -> Int32 { steps.last { epochMs >= $0.0 }?.1 ?? base }
    }

    static func instant(_ s: String) -> Double {
        let f = ISO8601DateFormatter()
        return (f.date(from: s)?.timeIntervalSince1970 ?? 0) * 1000
    }

    func same(_ got: JSONValue, _ want: JSONValue, _ name: String) {
        XCTAssertTrue(jsonEqual(a: got.json, b: want.json), "\(name): got \(got.json.prefix(300)), want \(want.json.prefix(300))")
    }

    func outcome(_ o: Expressions.Outcome, key: String) -> JSONValue {
        switch o {
        case .value(let v): return .object([key: v])
        case .error(let e): return .object(["error": .string(e)])
        }
    }

    func want(_ c: JSONValue, key: String) -> JSONValue {
        if let e = c["error"] { return .object(["error": e]) }
        return .object([key: c[key] ?? .null])
    }

    /// VAPP-206: `fixtures/host-router.json` `hostParams`.
    func testHostParams() throws {
        let f = try Fixtures.json("host-router.json")
        for c in f["hostParams"]?.array ?? [] {
            let got = HostParams.values(nowMs: Self.instant(c["now"]?.string ?? ""), zone: FixtureZone(c))
            same(got, c["expected"] ?? .null, c["name"]?.string ?? "")
        }
    }

    func testEval() throws {
        let f = try Fixtures.json("expressions.json")
        let vars = f["vars"]?.object ?? [:]
        let cases = f["eval"]?.array ?? []
        XCTAssertGreaterThan(cases.count, 150)
        for c in cases {
            let now = Self.instant(c["now"]?.string ?? f["env"]?["now"]?.string ?? "")
            let zone = FixtureZone(f["zones"]?[c["zone"]?.string ?? f["env"]?["zone"]?.string ?? ""])
            let got = Expressions.evaluate(c["expr"]?.string ?? "", vars: vars, nowMs: now, zone: zone, maxSteps: c["maxSteps"]?.number.map { UInt32($0) })
            same(outcome(got, key: "value"), want(c, key: "value"), "eval: \(c["name"]?.string ?? "")")
        }
    }

    func testCheck() throws {
        let f = try Fixtures.json("expressions.json")
        let types = f["checkTypes"]?.object ?? [:]
        for c in f["check"]?.array ?? [] {
            same(outcome(Expressions.check(c["expr"]?.string ?? "", types: types), key: "type"), want(c, key: "type"), "check: \(c["name"]?.string ?? "")")
        }
    }

    func testPackages() throws {
        let f = try Fixtures.json("expressions.json")
        for c in f["packages"]?.array ?? [] {
            let got = try validatePackageJson(packageJson: (c["package"] ?? .null).json, catalogIds: nil)
            same(JSONValue.parse(got), c["issues"] ?? .array([]), "package: \(c["name"]?.string ?? "")")
        }
    }

    func testTheFixtureZoneIsFoundations() throws {
        let f = try Fixtures.json("expressions.json")
        let vienna = f["zones"]?["vienna"]
        let ours = FixtureZone(vienna)
        let theirs = TimeZoneOffset(TimeZone(identifier: vienna?["name"]?.string ?? "")!)
        for t in vienna?["transitions"]?.array ?? [] {
            let ms = Self.instant(t.array?[0].string ?? "")
            for at in [ms - 1, ms, ms + 1] { XCTAssertEqual(ours.offsetMinutes(epochMs: at), theirs.offsetMinutes(epochMs: at)) }
        }
    }
}

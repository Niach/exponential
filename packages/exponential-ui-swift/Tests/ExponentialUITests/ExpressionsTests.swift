import XCTest
import ExponentialUICore
@testable import ExponentialUI

/// VAPP-191: `fixtures/expressions.json` through `Expressions` (the core's
/// one evaluator over the FFI). The TS reference, the Rust core and
/// Compose's `ExpressionsTest` replay the same cases.
final class ExpressionsTests: XCTestCase {
    func ms(_ iso: String) -> Double {
        let f = ISO8601DateFormatter()
        f.formatOptions = iso.contains(".") ? [.withInternetDateTime, .withFractionalSeconds] : [.withInternetDateTime]
        return (f.date(from: iso)?.timeIntervalSince1970 ?? 0) * 1000
    }

    func clock(_ c: JSONValue, _ f: JSONValue) -> (Double, Int) {
        let clock = c["clock"] ?? f["clock"] ?? .null
        return (ms(clock["now"]?.string ?? ""), Int(clock["offsetMinutes"]?.number ?? 0))
    }

    func same(_ got: JSONValue, _ want: JSONValue?, _ name: String) {
        let want = want ?? .null
        XCTAssertTrue(jsonEqual(a: got.json, b: want.json), "\(name): differs at \(jsonDiff(a: got.json, b: want.json)): got \(got.json.prefix(400))")
    }

    func testEvalAndCostCases() throws {
        let f = try Fixtures.json("expressions.json")
        var n = 0
        for key in ["evalCases", "costCases"] {
            for c in f[key]?.array ?? [] {
                n += 1
                let (now, offset) = clock(c, f)
                let got: JSONValue
                switch Expressions.eval(c["source"]?.string ?? "", vars: c["vars"] ?? f["vars"] ?? .object([:]), nowMs: now, offsetMinutes: offset) {
                case .value(let v): got = .object(["value": v])
                case .error(let e): got = .object(["error": .string(e)])
                }
                same(got, c["expected"], "\(key) \(c["name"]?.string ?? "")")
            }
        }
        XCTAssertGreaterThan(n, 200)
    }

    func testCheckCases() throws {
        let f = try Fixtures.json("expressions.json")
        for c in f["checkCases"]?.array ?? [] {
            let types = (c["types"]?.object ?? [:]).compactMapValues(\.string)
            let got = Expressions.check(c["source"]?.string ?? "", types: types, kinds: f["kinds"] ?? .object([:]))
            same(.object(["type": .string(got.type), "issues": .array(got.issues.map { .string($0) })]), c["expected"], c["name"]?.string ?? "")
        }
    }

    func testCallCases() throws {
        let f = try Fixtures.json("expressions.json")
        for c in f["callCases"]?.array ?? [] {
            let (now, offset) = clock(c, f)
            let got = Expressions.call(c["call"]?.string ?? "", args: c["args"] ?? .object([:]), nowMs: now, offsetMinutes: offset)
            same(got.map { .object(["value": $0]) } ?? .object([:]), c["expected"], c["name"]?.string ?? "")
        }
    }

    func testTheClockAndZone() {
        let now = Date(timeIntervalSince1970: ms("2026-11-02T23:30:00Z") / 1000)
        XCTAssertEqual(Expressions.eval("today()", now: now, timeZone: TimeZone(identifier: "Europe/Vienna")), .value(.string("2026-11-03")))
        XCTAssertEqual(Expressions.eval("today()", now: now), .value(.string("2026-11-02")))
    }
}

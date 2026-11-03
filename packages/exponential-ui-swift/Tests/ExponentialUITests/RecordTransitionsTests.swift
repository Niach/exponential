import XCTest
import ExponentialUICore
@testable import ExponentialUI

/// eApps R6 (VAPP-222): `fixtures/record-transitions.json` through the SDK
/// (the core over the FFI). The TS reference and the Rust core replay the
/// same cases.
final class RecordTransitionsTests: XCTestCase {
    private func kind(_ f: JSONValue, _ c: JSONValue) -> JSONValue {
        if let t = c.object?["transitions"] { return .object(["transitions": t]) }
        return f["kind"] ?? .null
    }

    private func nonNull(_ v: JSONValue?) -> JSONValue? {
        guard let v, v != .null else { return nil }
        return v
    }

    private func same(_ got: JSONValue, _ want: JSONValue?, _ name: String) {
        let w = want ?? .null
        XCTAssertTrue(jsonEqual(a: got.json, b: w.json), "\(name): got \(got.json.prefix(300)), want \(w.json.prefix(300))")
    }

    func testFixture() throws {
        let f = try Fixtures.json("record-transitions.json")
        let validate = f["validateCases"]?.array ?? []
        XCTAssertFalse(validate.isEmpty)
        for c in validate {
            let name = c["kindName"]?.string ?? ""
            same(JSONValue.array(RecordTransitions.validate(kind: c["kind"] ?? .null, at: "/records/\(name)", kindName: name)), c["expected"], c["name"]?.string ?? "")
        }
        for c in f["refusalCases"]?.array ?? [] {
            let got = RecordTransitions.refusal(kind: kind(f, c), before: nonNull(c["before"]), after: c["after"] ?? .null, viewer: c["viewer"] ?? .null, row: nonNull(c["row"]))
            same(got ?? .null, c["expected"], c["name"]?.string ?? "")
        }
        for c in f["fromCases"]?.array ?? [] {
            let got = RecordTransitions.from(kind: kind(f, c), record: nonNull(c["record"]), viewer: nonNull(c["viewer"]), row: nonNull(c["row"]))
            same(JSONValue.array(got.map(JSONValue.string)), c["expected"], c["name"]?.string ?? "")
        }
    }
}

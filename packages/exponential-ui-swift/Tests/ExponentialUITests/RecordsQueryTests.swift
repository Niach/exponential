import XCTest
import ExponentialUICore
@testable import ExponentialUI

/// VAPP-189: `fixtures/records-query.json` through `RecordsQuery` (the
/// core's one evaluator over the FFI). The TS reference, the Rust core and
/// Compose's `RecordsQueryTest` replay the same cases.
final class RecordsQueryTests: XCTestCase {
    func strings(_ v: JSONValue?) -> [String: String] {
        (v?.object ?? [:]).compactMapValues(\.string)
    }

    func same(_ got: JSONValue, _ want: JSONValue?, _ name: String) {
        let want = want ?? .null
        XCTAssertTrue(jsonEqual(a: got.json, b: want.json), "\(name): differs at \(jsonDiff(a: got.json, b: want.json)): got \(got.json.prefix(400))")
    }

    func rows(_ v: JSONValue?) -> [RecordRow] {
        (v?.array ?? []).map { r in
            RecordRow(
                id: r["id"]?.string ?? "",
                kind: r["kind"]?.string ?? "",
                data: r["data"] ?? .object([:]),
                createdBy: r["createdBy"]?.string,
                authorId: r["authorId"]?.string,
                createdAt: r["createdAt"]?.string ?? "",
                updatedAt: r["updatedAt"]?.string ?? ""
            )
        }
    }

    func testParseCases() throws {
        let f = try Fixtures.json("records-query.json")
        let cases = f["parseCases"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let query = RecordsQuery.parse(params: strings(c["params"]), kind: c["kind"]?.string)
            same(query.structured, c["expected"], c["name"]?.string ?? "")
        }
    }

    func testSourceCases() throws {
        let f = try Fixtures.json("records-query.json")
        for c in f["sourceCases"]?.array ?? [] {
            let source = c["source"]?.string ?? ""
            same(RecordsQuery.parse(source: source)?.structured ?? .null, c["expected"], c["name"]?.string ?? "")
            // The parsed-source path agrees with the string path.
            let parsed = BindingSource.parse(source).flatMap { RecordsQuery.parse(source: $0) }
            same(parsed?.structured ?? .null, c["expected"], "parsed \(c["name"]?.string ?? "")")
        }
    }

    func testValueCases() throws {
        let f = try Fixtures.json("records-query.json")
        let cases = f["valueCases"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let query = RecordsQuery.parse(params: strings(c["params"]), kind: c["kind"]?.string)
            let value = query.value(rows: rows(c["rows"]), me: c["me"]?.string, now: c["now"]?.string ?? "", names: strings(c["names"]))
            same(value, c["expected"], c["name"]?.string ?? "")
        }
    }

    func testTheQueryReadsItsKindAndApp() {
        let query = RecordsQuery.parse(params: ["kind": "entry", "app": "working-hours"])
        XCTAssertEqual(query.from, "entry")
        XCTAssertEqual(query.app, "working-hours")
        XCTAssertNil(RecordsQuery.parse(params: [:]).from)
        XCTAssertEqual(RecordsQuery.parse(params: [:]).value(rows: [], me: nil, now: "2026-10-24T09:00:00Z", names: [:]), .null)
    }
}

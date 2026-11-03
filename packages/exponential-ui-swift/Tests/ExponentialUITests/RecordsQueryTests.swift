import XCTest
import ExponentialUICore
@testable import ExponentialUI

/// VAPP-189: `fixtures/records-query.json` through `RecordsQuery` (the
/// core's one evaluator over the FFI; VAPP-190: jsonCases, bucketCases,
/// JSON valueCases). The TS reference, the Rust core and
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
            let query = c["query"].map { RecordsQuery.parse(json: $0.json, kind: c["kind"]?.string).query } ?? RecordsQuery.parse(params: strings(c["params"]), kind: c["kind"]?.string)
            let value = query.value(rows: rows(c["rows"]), me: c["me"]?.string, now: c["now"]?.string ?? "", today: c["today"]?.string, names: strings(c["names"]), computed: strings(c["computed"]))
            same(value, c["expected"], c["name"]?.string ?? "")
        }
    }

    func testJsonCases() throws {
        let f = try Fixtures.json("records-query.json")
        let cases = f["jsonCases"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let parsed = RecordsQuery.parse(json: (c["query"] ?? .null).json, kind: c["kind"]?.string)
            same(.object(["query": parsed.query.structured, "issues": .array(parsed.issues)]), c["expected"], c["name"]?.string ?? "")
        }
    }

    func testBucketCases() throws {
        let f = try Fixtures.json("records-query.json")
        let cases = f["bucketCases"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            XCTAssertEqual(RecordsQuery.dateBucket(c["date"]?.string ?? "", by: c["bucket"]?.string ?? ""), c["expected"]?.string, "\(c["date"]?.string ?? "") \(c["bucket"]?.string ?? "")")
        }
    }

    func testTheQueryReadsItsKindAndApp() {
        let query = RecordsQuery.parse(params: ["app": "working-hours"], kind: "entry")
        XCTAssertEqual(query.from, "entry")
        XCTAssertEqual(query.app, "working-hours")
        XCTAssertEqual(RecordsQuery.parse(source: "records:entry?app=working-hours"), query)
        // `kind` is no reserved param: only the source's name is the kind.
        XCTAssertNil(RecordsQuery.parse(params: ["kind": "entry"]).from)
        XCTAssertNil(RecordsQuery.parse(params: [:]).from)
        XCTAssertEqual(RecordsQuery.parse(params: [:]).value(rows: [], me: nil, now: "2026-10-24T09:00:00Z", names: [:]), .null)
    }
}

import Foundation
import XCTest
@testable import ExpCore

// VAPP-122: app records, replayed ×4 against
// packages/domain-contract/fixtures/app-records.json (TS `app-records.test.ts`,
// desktop `domain::app_records`, Android `AppRecordsTest`): `queryCases`,
// `valueCases` and `countLabelCases`, plus the query constants.
final class AppRecordsTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/app-records.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func cases(_ key: String) throws -> [[String: Any]] {
        let list = try XCTUnwrap(try fixture()[key] as? [[String: Any]])
        XCTAssertFalse(list.isEmpty, key)
        return list
    }

    private func optional(_ value: String?) -> Any { value ?? NSNull() }

    func testConstantsMatchTheFixture() throws {
        let fixture = try fixture()
        let query = try XCTUnwrap(fixture["query"] as? [String: Any])
        XCTAssertEqual(Set(try XCTUnwrap(query["reserved"] as? [String])), AppRecords.reserved)
        XCTAssertEqual(query["sortDefault"] as? String, AppRecords.sortDefault)
        XCTAssertEqual(query["orderDefault"] as? String, AppRecords.orderDefault)
        XCTAssertEqual(query["limitDefault"] as? Int, AppRecords.limitDefault)
        XCTAssertEqual(query["limitMax"] as? Int, AppRecords.limitMax)
        XCTAssertEqual(query["operators"] as? [String], AppRecords.operators)
        XCTAssertEqual(fixture["rowKeys"] as? [String], AppRecords.rowKeys)
        XCTAssertEqual(fixture["weekdays"] as? [String], AppRecords.weekdays)
        XCTAssertEqual(fixture["months"] as? [String], AppRecords.months)
    }

    func testQueryCases() throws {
        for item in try cases("queryCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            let params = try XCTUnwrap(item["params"] as? [String: String])
            let q = AppRecords.parseRecordsQuery(params)
            let actual: [String: Any] = [
                "kind": optional(q.kind), "app": optional(q.app), "mine": q.mine, "user": optional(q.user),
                "sort": q.sort, "order": q.order, "limit": q.limit,
                "filters": q.filters.map { ["field": $0.field, "op": $0.op, "value": $0.value] },
                "sum": optional(q.sum), "by": optional(q.by), "month": optional(q.month),
            ]
            XCTAssertEqual(actual as NSDictionary, try XCTUnwrap(item["expected"] as? NSDictionary), name)
        }
    }

    func testValueCases() throws {
        let iso = ISO8601DateFormatter()
        for item in try cases("valueCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            let params = try XCTUnwrap(item["params"] as? [String: String])
            let rows = try XCTUnwrap(item["rows"] as? [[String: Any]]).map { row in
                AppRecords.Row(
                    id: row["id"] as? String ?? "",
                    kind: row["kind"] as? String ?? "",
                    data: CardJSON(any: row["data"])?.objectValue ?? [:],
                    createdBy: row["createdBy"] as? String,
                    createdAt: row["createdAt"] as? String ?? "",
                    updatedAt: row["updatedAt"] as? String ?? ""
                )
            }
            let now = try XCTUnwrap(iso.date(from: try XCTUnwrap(item["now"] as? String)))
            let value = AppRecords.recordsValue(
                rows: rows,
                query: AppRecords.parseRecordsQuery(params),
                me: item["me"] as? String,
                now: now,
                names: item["names"] as? [String: String] ?? [:]
            )
            let expected = CardJSON(any: item["expected"]) ?? .null
            XCTAssertEqual(value ?? .null, expected, name)
            // The text the host dedupes on parses back to the same value.
            if let value {
                let reparsed = try JSONSerialization.jsonObject(with: Data(value.compactString.utf8))
                XCTAssertEqual(CardJSON(any: reparsed), expected, name)
            }
        }
    }

    func testCountLabelCases() throws {
        for item in try cases("countLabelCases") {
            let count = try XCTUnwrap(item["count"] as? Int)
            XCTAssertEqual(
                AppRecords.recordsCountLabel(count: count, max: item["max"] as? Int),
                item["label"] as? String
            )
        }
    }

    func testMonthHelpers() {
        XCTAssertEqual(AppRecords.shiftMonth("2026-01", by: -1), "2025-12")
        XCTAssertEqual(AppRecords.shiftMonth("2026-12", by: 1), "2027-01")
        XCTAssertEqual(AppRecords.monthLabel("2026-10"), "October 2026")
        XCTAssertEqual(AppRecords.monthDays("2028-02").count, 29)
        XCTAssertEqual(AppRecords.monthDays("2026-10").first?.weekday, "Thu")
    }
}

import Foundation
import XCTest
@testable import ExpCore

// VAPP-122: app records, replayed ×4 against
// packages/domain-contract/fixtures/app-records.json (TS `app-records.test.ts`,
// desktop `domain::app_records`, Android `AppRecordsTest`): `queryCases`,
// `sourceCases`, `valueCases` and `countLabelCases`, plus the query constants;
// VAPP-178 the Data tab's `dataTab.*Cases` and its copy / columns /
// placeholders / testIds.
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

    private func queryObject(_ q: AppRecords.Query) -> [String: Any] {
        [
            "kind": optional(q.kind), "app": optional(q.app), "mine": q.mine, "user": optional(q.user),
            "sort": q.sort, "order": q.order, "limit": q.limit,
            "filters": q.filters.map { ["field": $0.field, "op": $0.op, "value": $0.value] },
            "sum": optional(q.sum), "by": optional(q.by), "month": optional(q.month),
        ]
    }

    func testQueryCases() throws {
        for item in try cases("queryCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            let params = try XCTUnwrap(item["params"] as? [String: String])
            let actual = queryObject(AppRecords.parseRecordsQuery(params))
            XCTAssertEqual(actual as NSDictionary, try XCTUnwrap(item["expected"] as? NSDictionary), name)
        }
    }

    /// The SDK's parseSource, inlined (ExpCore does not depend on the SDK).
    private func parseSource(_ uri: String) throws -> (scheme: String, name: String, params: [String: String]) {
        let colon = try XCTUnwrap(uri.firstIndex(of: ":"))
        let rest = uri[uri.index(after: colon)...]
        let name = rest.split(separator: "?", maxSplits: 1, omittingEmptySubsequences: false).first.map(String.init) ?? ""
        var params: [String: String] = [:]
        for item in URLComponents(string: "x:?" + (rest.split(separator: "?", maxSplits: 1).dropFirst().first ?? ""))?.queryItems ?? [] {
            params[item.name] = item.value ?? ""
        }
        return (uri[..<colon].lowercased(), name, params)
    }

    func testSourceCases() throws {
        for item in try cases("sourceCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            let source = try parseSource(try XCTUnwrap(item["source"] as? String))
            let params = AppRecords.recordsSourceParams(scheme: source.scheme, name: source.name, params: source.params)
            if let expected = item["expected"] as? NSDictionary {
                let q = try XCTUnwrap(params.map(AppRecords.parseRecordsQuery), name)
                XCTAssertEqual(queryObject(q) as NSDictionary, expected, name)
            } else {
                XCTAssertTrue(item["expected"] is NSNull, name)
                XCTAssertNil(params, name)
            }
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

    /// A synced row's timestamps reach the binding as the web host's
    /// `toISOString` (the same ×4), whatever wire form Electric sent.
    func testRowTimestampsAreIsoMillis() {
        XCTAssertEqual(AppRecords.isoTimestamp("2026-10-05 18:00:00+00"), "2026-10-05T18:00:00.000Z")
        XCTAssertEqual(AppRecords.isoTimestamp("2026-10-05 18:00:00.123456+00"), "2026-10-05T18:00:00.123Z")
        XCTAssertEqual(AppRecords.isoTimestamp("2026-10-05T18:00:00Z"), "2026-10-05T18:00:00.000Z")
        XCTAssertEqual(AppRecords.isoTimestamp("garbage"), "garbage")
        let entity = AppRecordEntity(
            id: "r1", appId: "a1", teamId: "t1", kind: "entry", data: "{\"hours\":3}",
            createdBy: "u1", createdAt: "2026-10-05 18:00:00+00", updatedAt: "2026-10-06 07:30:00.5+00"
        )
        let row = AppRecords.Row(entity)
        XCTAssertEqual(row.createdAt, "2026-10-05T18:00:00.000Z")
        XCTAssertEqual(row.updatedAt, "2026-10-06T07:30:00.500Z")
    }

    // MARK: - The Data tab (VAPP-178)

    private func dataTab() throws -> [String: Any] {
        try XCTUnwrap(try fixture()["dataTab"] as? [String: Any])
    }

    private func dataCases(_ key: String) throws -> [[String: Any]] {
        let list = try XCTUnwrap(try dataTab()[key] as? [[String: Any]], key)
        XCTAssertFalse(list.isEmpty, key)
        return list
    }

    private func json(_ value: Any?) -> CardJSON {
        CardJSON(any: value) ?? .null
    }

    func testDataTabConstantsMatchTheFixture() throws {
        let tab = try dataTab()
        typealias C = AppRecords.DataTab.Copy
        XCTAssertEqual(tab["copy"] as? [String: String], [
            "kindOption": C.kindOption, "addRecord": C.addRecord, "edit": C.edit, "delete": C.delete,
            "deleteTitle": C.deleteTitle, "deleteBody": C.deleteBody, "by": C.by, "updated": C.updated,
            "empty": C.empty, "noKinds": C.noKinds, "emptyTitle": C.emptyTitle, "newTitle": C.newTitle,
            "editTitle": C.editTitle, "save": C.save, "cancel": C.cancel, "yes": C.yes, "no": C.no,
            "none": C.none, "optional": C.optional, "listHint": C.listHint,
        ])
        let columns = try XCTUnwrap(tab["columns"] as? [String: Any])
        XCTAssertEqual(columns["byWidth"] as? Double, AppRecords.DataTab.Columns.byWidth)
        XCTAssertEqual(columns["updatedWidth"] as? Double, AppRecords.DataTab.Columns.updatedWidth)
        XCTAssertEqual(columns["menuWidth"] as? Double, AppRecords.DataTab.Columns.menuWidth)
        XCTAssertEqual(columns["minWidth"] as? Double, AppRecords.DataTab.Columns.minWidth)
        XCTAssertEqual(columns["gap"] as? Double, AppRecords.DataTab.Columns.gap)
        XCTAssertEqual(columns["typeWidths"] as? [String: Double], AppRecords.DataTab.Columns.typeWidths)
        XCTAssertEqual(columns.count, 6)
        XCTAssertEqual(tab["placeholders"] as? [String: String], AppRecords.DataTab.placeholders)
        typealias T = AppRecords.DataTab.TestIds
        XCTAssertEqual(tab["testIds"] as? [String: String], [
            "kind": T.kind, "add": T.add, "row": T.row, "menu": T.menu, "form": T.form, "save": T.save, "error": T.error,
        ])
        XCTAssertEqual(C.kindOption("entry", count: 2), "entry · 2")
        XCTAssertEqual(C.newTitle("entry"), "New entry")
        XCTAssertEqual(C.editTitle("entry"), "Edit entry")
    }

    func testDataKindCases() throws {
        for item in try dataCases("kindCases") {
            let manifest: CardJSON? = item["manifest"] is NSNull ? nil : json(item["manifest"])
            let rowKinds = try XCTUnwrap(item["rows"] as? [[String: Any]]).map { $0["kind"] as? String ?? "" }
            let expected = try XCTUnwrap(item["kinds"] as? [[String: Any]]).map {
                AppRecords.DataKind(kind: $0["kind"] as? String ?? "", count: $0["count"] as? Int ?? -1)
            }
            XCTAssertEqual(AppRecords.dataKinds(manifest: manifest, rowKinds: rowKinds), expected, item["$why"] as? String ?? "")
        }
    }

    func testDataColumnCases() throws {
        for item in try dataCases("columnCases") {
            XCTAssertEqual(AppRecords.dataColumns(json(item["schema"])), item["columns"] as? [String])
        }
    }

    func testDataRowsCases() throws {
        for item in try dataCases("rowsCases") {
            let rows = try XCTUnwrap(item["rows"] as? [[String: Any]]).map { row in
                AppRecords.Row(
                    id: row["id"] as? String ?? "",
                    kind: row["kind"] as? String ?? "",
                    data: [:],
                    createdBy: nil,
                    createdAt: row["updatedAt"] as? String ?? "",
                    updatedAt: row["updatedAt"] as? String ?? ""
                )
            }
            let kind = try XCTUnwrap(item["kind"] as? String)
            XCTAssertEqual(AppRecords.dataRows(rows, kind: kind).map(\.id), item["order"] as? [String])
        }
    }

    func testDataCellCases() throws {
        for item in try dataCases("cellCases") {
            // A missing `value` = nil (the field is absent), null = .null.
            let value: CardJSON? = item.keys.contains("value") ? json(item["value"]) : nil
            XCTAssertEqual(AppRecords.cellText(value), item["text"] as? String, "\(item["value"] ?? "missing")")
        }
    }

    private func fieldObject(_ field: AppRecords.DataFormField) -> NSDictionary {
        var object: [String: Any] = ["name": field.name, "control": field.control.rawValue, "required": field.required]
        if let placeholder = field.placeholder { object["placeholder"] = placeholder }
        if let options = field.options { object["options"] = options }
        if let itemType = field.itemType { object["itemType"] = itemType }
        return object as NSDictionary
    }

    func testDataFormCases() throws {
        for item in try dataCases("formCases") {
            let fields = AppRecords.formFields(json(item["schema"])).map(fieldObject)
            let expected = try XCTUnwrap(item["fields"] as? [NSDictionary])
            XCTAssertEqual(fields, expected)
        }
    }

    func testDataInputCases() throws {
        for item in try dataCases("inputCases") {
            let control = try XCTUnwrap(AppRecords.DataControl(rawValue: try XCTUnwrap(item["control"] as? String)))
            let input = AppRecords.formInput(control, json(item["value"]))
            let expected: AppRecords.DataInput
            if let text = item["input"] as? String {
                expected = .text(text)
            } else {
                let flag = try XCTUnwrap(item["input"] as? Bool)
                expected = .flag(flag)
            }
            XCTAssertEqual(input, expected, "\(control) \(item["value"] ?? "nil")")
        }
    }

    func testDataParseCases() throws {
        for item in try dataCases("parseCases") {
            var inputs: [String: AppRecords.DataInput] = [:]
            for (name, raw) in try XCTUnwrap(item["inputs"] as? [String: Any]) {
                if let text = raw as? String {
                    inputs[name] = .text(text)
                } else {
                    let flag = try XCTUnwrap(raw as? Bool)
                    inputs[name] = .flag(flag)
                }
            }
            let data = AppRecords.formData(
                json(item["schema"]), inputs: inputs, editing: try XCTUnwrap(item["editing"] as? Bool)
            )
            XCTAssertEqual(CardJSON.object(data), json(item["data"]), item["$why"] as? String ?? "")
        }
    }
}

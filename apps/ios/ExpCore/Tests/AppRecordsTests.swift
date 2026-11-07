import Foundation
import XCTest
@testable import ExpCore

// VAPP-122: app records, replayed ×4 against
// packages/domain-contract/fixtures/app-records.json (TS `app-records.test.ts`,
// desktop `domain::app_records`, Android `AppRecordsTest`): `countLabelCases`
// and the row keys (VAPP-189: the query is eUI's, the SDK's
// `RecordsQueryTests` replays fixtures/records-query.json);
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

    func testConstantsMatchTheFixture() throws {
        XCTAssertEqual(try fixture()["rowKeys"] as? [String], AppRecords.rowKeys)
    }

    // VAPP-187: `ownership` — the policy, the refusal and `canChangeCases`
    // (`author` / `role` / `viewer` null = nil).
    func testOwnershipConstantsMatchTheFixture() throws {
        let ownership = try XCTUnwrap(try fixture()["ownership"] as? [String: Any])
        XCTAssertEqual(ownership["policy"] as? [String: String], AppRecords.Ownership.policy)
        XCTAssertEqual(ownership["refusal"] as? String, AppRecords.Ownership.refusal)
    }

    // eApps R3: packages/exponential-ui fixtures/record-policy.json (eUI's
    // rule, hand-ported in `AppRecords.Policy`).
    func testRecordPolicyFixture() throws {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("packages/exponential-ui/fixtures/record-policy.json")
        let f = try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any])
        XCTAssertEqual(f["ops"] as? [String], AppRecords.Policy.ops)
        XCTAssertEqual(f["levels"] as? [String], AppRecords.Policy.levels)
        XCTAssertEqual(f["access"] as? [String], AppRecords.Policy.access)
        XCTAssertEqual(f["defaults"] as? [String: String], AppRecords.Policy.defaults)
        let copy = try XCTUnwrap(f["copy"] as? [String: Any])
        XCTAssertEqual(copy["none"] as? String, AppRecords.Policy.Copy.none)
        XCTAssertEqual(copy["readOnly"] as? String, AppRecords.Policy.Copy.readOnly)
        XCTAssertEqual(copy["author"] as? [String: String], AppRecords.Policy.Copy.author)
        XCTAssertEqual(copy["owners"] as? [String: String], AppRecords.Policy.Copy.owners)
        for item in try XCTUnwrap(f["validateCases"] as? [[String: Any]]) {
            let kind = CardJSON(any: ["policy": item["policy"] ?? NSNull()])
            XCTAssertEqual(AppRecords.Policy.effective(kind), item["effective"] as? [String: String], item["name"] as? String ?? "")
        }
        for item in try XCTUnwrap(f["allowCases"] as? [[String: Any]]) {
            let name = try XCTUnwrap(item["name"] as? String)
            let op = try XCTUnwrap(item["op"] as? String)
            let policy = AppRecords.Policy.effective(item["policy"].flatMap { CardJSON(any: ["policy": $0]) })
            let row = op == "create" ? nil : AppRecords.Row(id: "r", kind: "k", data: [:], createdBy: item["createdBy"] as? String, authorId: item["author"] as? String, createdAt: "", updatedAt: "")
            let got = AppRecords.Policy.refusal(policy, op: op, viewer: item["viewer"] as? String, role: item["role"] as? String, access: item["access"] as? String, row: row)
            XCTAssertEqual(got, item["refusal"] as? String, name)
        }
    }

    func testCanChangeCases() throws {
        let ownership = try XCTUnwrap(try fixture()["ownership"] as? [String: Any])
        let list = try XCTUnwrap(ownership["canChangeCases"] as? [[String: Any]])
        XCTAssertFalse(list.isEmpty)
        for item in list {
            let name = try XCTUnwrap(item["name"] as? String)
            let row = AppRecords.Row(
                id: "r1", kind: "entry", data: [:],
                createdBy: item["createdBy"] as? String,
                authorId: item["author"] as? String,
                createdAt: "", updatedAt: ""
            )
            let expected = try XCTUnwrap(item["expected"] as? [String: Bool], name)
            for op in ["update", "delete"] {
                XCTAssertEqual(
                    AppRecords.canChangeRecord(row, viewer: item["viewer"] as? String, role: item["role"] as? String, op: op),
                    try XCTUnwrap(expected[op], name),
                    "\(name) (\(op))"
                )
            }
        }
    }

    // VAPP-188: `onBehalf` — the refusals, `writeAsCases` and `importCases`
    // (`author` / `role` / `viewer` / `expected` null = nil).
    func testOnBehalfConstantsMatchTheFixture() throws {
        let onBehalf = try XCTUnwrap(try fixture()["onBehalf"] as? [String: Any])
        XCTAssertEqual(onBehalf["refusal"] as? String, AppRecords.OnBehalf.refusal)
        XCTAssertEqual(onBehalf["notMember"] as? String, AppRecords.OnBehalf.notMember)
    }

    private func onBehalfCases(_ key: String) throws -> [[String: Any]] {
        let onBehalf = try XCTUnwrap(try fixture()["onBehalf"] as? [String: Any])
        let list = try XCTUnwrap(onBehalf[key] as? [[String: Any]], key)
        XCTAssertFalse(list.isEmpty, key)
        return list
    }

    func testWriteAsCases() throws {
        for item in try onBehalfCases("writeAsCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            let expected = try XCTUnwrap(AppRecords.WriteAsVerdict(rawValue: try XCTUnwrap(item["expected"] as? String)), name)
            XCTAssertEqual(
                AppRecords.writeAsVerdict(
                    viewer: item["viewer"] as? String,
                    role: item["role"] as? String,
                    author: item["author"] as? String,
                    members: try XCTUnwrap(item["members"] as? [String], name)
                ),
                expected,
                name
            )
        }
    }

    func testImportCases() throws {
        for item in try onBehalfCases("importCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            XCTAssertEqual(
                AppRecords.importAuthor(
                    importer: try XCTUnwrap(item["importer"] as? String, name),
                    role: item["role"] as? String,
                    author: item["author"] as? String,
                    members: try XCTUnwrap(item["members"] as? [String], name)
                ),
                item["expected"] as? String,
                name
            )
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
            "author": C.author, "byFor": C.byFor, "file": C.file, "files": C.files,
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
            "history": T.history, "historyRow": T.historyRow, "restore": T.restore,
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

    // VAPP-221: an attachment field (a string or a list of strings with
    // `format: "attachment"`) gets no form field; `cellCases` / `formCases`
    // above replay the fixture, this pins the field rule itself.
    func testIsAttachmentField() {
        let attachment: CardJSON = .object(["type": .string("string"), "format": .string("attachment")])
        let list: CardJSON = .object(["type": .string("array"), "items": attachment])
        XCTAssertTrue(AppRecords.isAttachmentField(attachment))
        XCTAssertTrue(AppRecords.isAttachmentField(list))
        XCTAssertFalse(AppRecords.isAttachmentField(.object(["type": .string("string"), "format": .string("date")])))
        XCTAssertFalse(AppRecords.isAttachmentField(.object(["type": .string("array"), "items": .object(["type": .string("string")])])))
        XCTAssertFalse(AppRecords.isAttachmentField(.object(["type": .string("array"), "format": .string("attachment")])))
        XCTAssertFalse(AppRecords.isAttachmentField(nil))
        // A list of refs reads as a count, but its form input stays the refs.
        let refs: CardJSON = .array([.string("/api/attachments/0f6e2c1a-8b3d-4c5e-9a7f-1b2c3d4e5f60")])
        XCTAssertEqual(AppRecords.cellText(refs), AppRecords.DataTab.Copy.file)
        XCTAssertEqual(AppRecords.formInput(.list, refs), .text("/api/attachments/0f6e2c1a-8b3d-4c5e-9a7f-1b2c3d4e5f60"))
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

    // VAPP-188: the `by` cell, the owner's Author picker and what Save sends.
    func testDataBylineCases() throws {
        for item in try dataCases("bylineCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            let row = AppRecords.Row(
                id: "r1", kind: "entry", data: [:],
                createdBy: item["createdBy"] as? String,
                authorId: item["authorId"] as? String,
                createdAt: "", updatedAt: ""
            )
            XCTAssertEqual(
                AppRecords.recordByline(
                    row,
                    names: try XCTUnwrap(item["names"] as? [String: String], name),
                    fallback: try XCTUnwrap(item["fallback"] as? String, name)
                ),
                item["text"] as? String,
                name
            )
        }
    }

    func testDataAuthorOptionCases() throws {
        for item in try dataCases("authorOptionCases") {
            let users = try XCTUnwrap(item["users"] as? [[String: Any]]).map {
                AppRecords.AuthorOption(id: $0["id"] as? String ?? "", name: $0["name"] as? String ?? "")
            }
            XCTAssertEqual(AppRecords.authorOptions(users).map(\.id), item["order"] as? [String])
        }
    }

    func testDataFormAuthorCases() throws {
        for item in try dataCases("formAuthorCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            let record = (item["record"] as? [String: Any]).map {
                AppRecords.Row(
                    id: "r1", kind: "entry", data: [:],
                    createdBy: $0["createdBy"] as? String,
                    authorId: $0["authorId"] as? String,
                    createdAt: "", updatedAt: ""
                )
            }
            let result = AppRecords.formAuthor(
                viewer: item["viewer"] as? String, record: record, picked: item["picked"] as? String
            )
            XCTAssertEqual(result.defaultAuthor, item["default"] as? String, name)
            XCTAssertEqual(result.send, item["send"] as? String, name)
        }
    }

    // MARK: - Record history + undo (VAPP-215, fixture `history`)

    private func history() throws -> [String: Any] {
        try XCTUnwrap(try fixture()["history"] as? [String: Any])
    }

    private func historyCases(_ key: String) throws -> [[String: Any]] {
        let list = try XCTUnwrap(try history()[key] as? [[String: Any]], key)
        XCTAssertFalse(list.isEmpty, key)
        return list
    }

    /// A fixture object (null = nil).
    private func object(_ value: Any?) -> [String: CardJSON]? {
        value is NSNull ? nil : json(value).objectValue
    }

    private func event(_ item: [String: Any]) -> AppRecords.RecordEvent {
        AppRecords.RecordEvent(
            id: item["id"] as? String ?? "",
            op: item["op"] as? String ?? "",
            actor: item["actor"] as? String,
            via: item["via"] as? String,
            before: object(item["before"]),
            patch: object(item["patch"]),
            createdAt: item["createdAt"] as? String ?? ""
        )
    }

    func testHistoryConstantsMatchTheFixture() throws {
        let h = try history()
        typealias H = AppRecords.History
        typealias C = AppRecords.History.Copy
        XCTAssertEqual(h["undoSeconds"] as? Double, H.undoSeconds)
        XCTAssertEqual(h["retentionDays"] as? Int, H.retentionDays)
        XCTAssertEqual(h["batch"] as? Int, H.batch)
        XCTAssertEqual(h["historyLimit"] as? Int, H.historyLimit)
        XCTAssertEqual(h["maxFields"] as? Int, H.maxFields)
        XCTAssertEqual(h["ops"] as? [String], H.ops)
        XCTAssertEqual(h["vias"] as? [String], H.vias)
        XCTAssertEqual(h["copy"] as? [String: String], [
            "history": C.history, "restore": C.restore, "undo": C.undo, "created": C.created,
            "deleted": C.deleted, "restored": C.restored, "changed": C.changed, "more": C.more,
            "saved": C.saved, "empty": C.empty, "undoCreate": C.undoCreate, "undoUpdate": C.undoUpdate,
            "undoDelete": C.undoDelete, "undoBatch": C.undoBatch, "undoVerb": C.undoVerb, "undoThing": C.undoThing,
        ])
        XCTAssertEqual(AppRecords.undoTitle(.create), C.undoCreate)
        XCTAssertEqual(AppRecords.undoTitle(.update), C.undoUpdate)
        XCTAssertEqual(AppRecords.undoTitle(.delete), C.undoDelete)
        XCTAssertEqual(AppRecords.undoTitle(.batch), C.undoBatch)
    }

    func testHistoryStateCases() throws {
        for item in try historyCases("stateCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            let e = event(try XCTUnwrap(item["event"] as? [String: Any], name))
            XCTAssertEqual(AppRecords.eventState(e), object(item["state"]), name)
        }
    }

    func testHistoryInverseCases() throws {
        for item in try historyCases("inverseCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            let e = event(try XCTUnwrap(item["event"] as? [String: Any], name))
            let inverse = try XCTUnwrap(item["inverse"] as? [String: Any], name)
            let expected: AppRecords.InverseWrite
            if inverse["op"] as? String == "delete" {
                expected = .delete
            } else {
                expected = .restore(try XCTUnwrap(object(inverse["data"]), name))
            }
            XCTAssertEqual(AppRecords.inverseWrite(e), expected, name)
        }
    }

    func testHistoryLabelCases() throws {
        for item in try historyCases("labelCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            let e = event(try XCTUnwrap(item["event"] as? [String: Any], name))
            XCTAssertEqual(AppRecords.eventLabel(e), item["label"] as? String, name)
        }
    }

    func testHistoryRowsCases() throws {
        for item in try historyCases("rowsCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            let events = try XCTUnwrap(item["events"] as? [[String: Any]], name).map { event($0) }
            let rows = AppRecords.historyRows(
                events,
                names: try XCTUnwrap(item["names"] as? [String: String], name),
                fallback: try XCTUnwrap(item["fallback"] as? String, name)
            )
            let expected = try XCTUnwrap(item["rows"] as? [[String: Any]], name).map {
                AppRecords.HistoryRow(
                    id: $0["id"] as? String ?? "",
                    label: $0["label"] as? String ?? "",
                    who: $0["who"] as? String ?? "",
                    at: $0["at"] as? String ?? "",
                    restorable: $0["restorable"] as? Bool ?? false
                )
            }
            XCTAssertEqual(rows, expected, name)
        }
    }

    // MARK: - The offline outbox (VAPP-235, fixture `outbox`)

    private func outbox() throws -> [String: Any] {
        try XCTUnwrap(try fixture()["outbox"] as? [String: Any])
    }

    private func outboxCases(_ key: String) throws -> [[String: Any]] {
        let list = try XCTUnwrap(try outbox()[key] as? [[String: Any]], key)
        XCTAssertFalse(list.isEmpty, key)
        return list
    }

    private func entry(_ value: Any?, _ name: String) throws -> AppRecords.OutboxEntry {
        try XCTUnwrap(AppRecords.OutboxEntry(json: json(value)), name)
    }

    private func entries(_ value: Any?, _ name: String) throws -> [AppRecords.OutboxEntry] {
        try XCTUnwrap(value as? [Any], name).map { try entry($0, name) }
    }

    private func recordRow(_ value: [String: Any]) -> AppRecords.Row {
        AppRecords.Row(
            id: value["id"] as? String ?? "",
            kind: value["kind"] as? String ?? "",
            data: json(value["data"]).objectValue ?? [:],
            createdBy: value["createdBy"] as? String,
            authorId: value["authorId"] as? String,
            createdAt: value["createdAt"] as? String ?? "",
            updatedAt: value["updatedAt"] as? String ?? ""
        )
    }

    private func rowJSON(_ row: AppRecords.Row) -> CardJSON {
        .object([
            "id": .string(row.id), "kind": .string(row.kind), "data": .object(row.data),
            "createdBy": row.createdBy.map(CardJSON.string) ?? .null,
            "authorId": row.authorId.map(CardJSON.string) ?? .null,
            "createdAt": .string(row.createdAt), "updatedAt": .string(row.updatedAt),
        ])
    }

    func testOutboxConstantsMatchTheFixture() throws {
        let o = try outbox()
        XCTAssertEqual(o["copy"] as? [String: String], [
            "pendingOne": AppRecords.Outbox.Copy.pendingOne, "pending": AppRecords.Outbox.Copy.pending,
        ])
    }

    func testOutboxRowsCases() throws {
        for item in try outboxCases("rowsCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            let rows = try XCTUnwrap(item["rows"] as? [[String: Any]], name).map { recordRow($0) }
            let got = AppRecords.outboxRows(rows, entries: try entries(item["entries"], name), me: item["me"] as? String)
            XCTAssertEqual(CardJSON.array(got.map { rowJSON($0) }), json(item["expected"]), name)
        }
    }

    /// VAPP-212: the overlay laid over the rows answers the same cases.
    func testOutboxOverlayCases() throws {
        for item in try outboxCases("rowsCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            let rows = try XCTUnwrap(item["rows"] as? [[String: Any]], name).map { recordRow($0) }
            let overlay = AppRecords.outboxOverlay(rows, entries: try entries(item["entries"], name), me: item["me"] as? String)
            var laid = rows.filter { !overlay.deletes.contains($0.id) }
            for row in overlay.upserts {
                if let i = laid.firstIndex(where: { $0.id == row.id }) { laid[i] = row } else { laid.append(row) }
            }
            XCTAssertEqual(CardJSON.array(laid.map { rowJSON($0) }), json(item["expected"]), name)
        }
    }

    func testOutboxPendingCases() throws {
        for item in try outboxCases("pendingCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            let label = AppRecords.outboxPendingLabel(
                try entries(item["entries"], name), appId: try XCTUnwrap(item["appId"] as? String, name)
            )
            XCTAssertEqual(label, item["label"] as? String, name)
        }
    }

    /// A persisted entry round-trips through its JSON (the GRDB row's `writes`).
    func testOutboxEntryRoundTrips() throws {
        for item in try outboxCases("flushCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            for e in try entries(item["entries"], name) {
                XCTAssertEqual(AppRecords.OutboxEntry(json: e.json), e, name)
            }
        }
    }

    // MARK: - Spreadsheet import (VAPP-228, fixture `csvImport`)

    private func csvImport() throws -> [String: Any] {
        try XCTUnwrap(try fixture()["csvImport"] as? [String: Any])
    }

    private func csvCases(_ key: String) throws -> [[String: Any]] {
        let list = try XCTUnwrap(try csvImport()[key] as? [[String: Any]], key)
        XCTAssertFalse(list.isEmpty, key)
        return list
    }

    private func csvRows(_ value: Any?) throws -> [AppRecords.CsvRow] {
        try XCTUnwrap(value as? [[String: Any]]).map {
            AppRecords.CsvRow(row: try XCTUnwrap($0["row"] as? Int), cells: try XCTUnwrap($0["cells"] as? [String]))
        }
    }

    /// A fixture mapping (null = Skip = absent).
    private func csvMapping(_ value: Any?) throws -> [String: Int] {
        try XCTUnwrap(value as? [String: Any]).compactMapValues { $0 as? Int }
    }

    func testCsvImportConstantsMatchTheFixture() throws {
        let c = try csvImport()
        typealias I = AppRecords.CsvImport
        typealias C = AppRecords.CsvImport.Copy
        typealias S = AppRecords.CsvImport.Issues
        typealias T = AppRecords.CsvImport.TestIds
        XCTAssertEqual(c["maxRows"] as? Int, I.maxRows)
        XCTAssertEqual(c["previewRows"] as? Int, I.previewRows)
        XCTAssertEqual(c["issuesShown"] as? Int, I.issuesShown)
        XCTAssertEqual(c["trueWords"] as? [String], I.trueWords)
        XCTAssertEqual(c["falseWords"] as? [String], I.falseWords)
        XCTAssertEqual(c["authorKeys"] as? [String], I.authorKeys)
        XCTAssertEqual(c["copy"] as? [String: String], [
            "importButton": C.importButton, "title": C.title, "placeholder": C.placeholder, "chooseFile": C.chooseFile,
            "skip": C.skip, "me": C.me, "column": C.column, "issue": C.issue, "rowIssue": C.rowIssue,
            "moreIssues": C.moreIssues, "ready": C.ready, "tooMany": C.tooMany, "noRows": C.noRows,
            "importAction": C.importAction, "imported": C.imported,
        ])
        XCTAssertEqual(c["issues"] as? [String: String], [
            "notNumber": S.notNumber, "notInteger": S.notInteger, "notBoolean": S.notBoolean, "notDate": S.notDate,
            "slashDate": S.slashDate, "notTime": S.notTime, "notDateTime": S.notDateTime, "notEnum": S.notEnum,
            "notMember": S.notMember, "extraCells": S.extraCells,
        ])
        XCTAssertEqual(c["testIds"] as? [String: String], [
            "button": T.button, "dialog": T.dialog, "text": T.text, "file": T.file, "map": T.map,
            "author": T.author, "preview": T.preview, "error": T.error, "confirm": T.confirm,
        ])
    }

    func testCsvParseCases() throws {
        for item in try csvCases("parseCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            let expected = AppRecords.CsvTable(
                header: try XCTUnwrap(item["header"] as? [String], name),
                rows: try csvRows(item["rows"])
            )
            XCTAssertEqual(AppRecords.csvParse(try XCTUnwrap(item["text"] as? String, name)), expected, name)
        }
    }

    func testCsvAutoMapCases() throws {
        for item in try csvCases("mapCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            let fields = try XCTUnwrap(item["fields"] as? [String], name)
            let header = try XCTUnwrap(item["header"] as? [String], name)
            XCTAssertEqual(AppRecords.csvAutoMap(fields: fields, header: header), try csvMapping(item["mapping"]), name)
        }
    }

    func testCsvAuthorColumnCases() throws {
        for item in try csvCases("authorColumnCases") {
            let header = try XCTUnwrap(item["header"] as? [String])
            XCTAssertEqual(AppRecords.csvAuthorColumn(header), item["column"] as? Int, "\(header)")
        }
    }

    func testCsvCellCases() throws {
        for item in try csvCases("cellCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            let expected: AppRecords.CsvCell
            if let issue = item["issue"] as? String {
                expected = .issue(issue)
            } else {
                expected = .value(json(item["value"]))
            }
            let cell = try XCTUnwrap(item["cell"] as? String, name)
            XCTAssertEqual(AppRecords.csvCell(json(item["field"]), cell), expected, name)
        }
    }

    func testCsvRecordsCases() throws {
        for item in try csvCases("rowsCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            var author: AppRecords.CsvAuthor?
            if let raw = item["author"] as? [String: Any] {
                author = AppRecords.CsvAuthor(
                    column: raw["column"] as? Int,
                    importer: try XCTUnwrap(raw["importer"] as? String, name),
                    members: try XCTUnwrap(raw["members"] as? [[String: Any]], name).map {
                        AppRecords.CsvMember(
                            id: $0["id"] as? String ?? "", name: $0["name"] as? String ?? "", email: $0["email"] as? String ?? ""
                        )
                    }
                )
            }
            let records = AppRecords.csvRecords(
                schema: json(item["schema"]),
                header: try XCTUnwrap(item["header"] as? [String], name),
                rows: try csvRows(item["rows"]),
                mapping: try csvMapping(item["mapping"]),
                author: author
            )
            let expected = try XCTUnwrap(item["records"] as? [[String: Any]], name).map {
                AppRecords.CsvRecord(
                    row: $0["row"] as? Int ?? -1,
                    data: json($0["data"]).objectValue ?? [:],
                    author: $0["author"] as? String,
                    issues: $0["issues"] as? [String] ?? ["missing"]
                )
            }
            XCTAssertEqual(records, expected, name)
        }
    }

    func testCsvSummaryCountsTheRowsWithoutIssues() {
        let issues: [[String]] = [[], ["x"], []]
        let records = issues.enumerated().map { AppRecords.CsvRecord(row: $0.offset + 2, data: [:], issues: $0.element) }
        XCTAssertEqual(AppRecords.csvSummary(records), "2 of 3 rows ready")
    }
}

import Foundation
import XCTest
@testable import ExpCore

// VAPP-110: Exponential apps, locked ×4 against
// packages/domain-contract/fixtures/apps.json (TS `apps.test.ts`, desktop
// `domain::apps`, Android `AppsTest`): the copy, the
// panes, the preview frame and the test ids are pinned; `slugifyCases`,
// `versionLabelCases`, `sortAppsCases`, `templateCases` and `automationRowCases` replay. The caps pin to
// `contract.json` `apps`. The `exp:` source derivations pin to the web host.
final class AppsTests: XCTestCase {
    private func repoFile(_ path: String) throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent(path)
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func fixture() throws -> [String: Any] {
        try repoFile("packages/domain-contract/fixtures/apps.json")
    }

    private func cases(_ key: String) throws -> [[String: Any]] {
        let list = try XCTUnwrap(try fixture()[key] as? [[String: Any]])
        XCTAssertFalse(list.isEmpty, key)
        return list
    }

    func testCopyMatchesTheFixture() throws {
        let copy = try XCTUnwrap(try fixture()["copy"] as? [String: Any])
        let flat: [String: String] = [
            "nav": Apps.Copy.nav, "title": Apps.Copy.title, "newApp": Apps.Copy.newApp,
            "untitled": Apps.Copy.untitled, "import": Apps.Copy.import,
            "version": Apps.Copy.version, "noVersion": Apps.Copy.noVersion,
            "versions": Apps.Copy.versions, "head": Apps.Copy.head, "noNote": Apps.Copy.noNote,
            "byAgent": Apps.Copy.byAgent, "viewing": Apps.Copy.viewing, "restore": Apps.Copy.restore,
            "restored": Apps.Copy.restored, "restoredNote": Apps.Copy.restoredNote,
            "publish": Apps.Copy.publish, "published": Apps.Copy.published,
            "publishedToast": Apps.Copy.publishedToast, "publishNothing": Apps.Copy.publishNothing,
            "migrating": Apps.Copy.migrating, "migratingOf": Apps.Copy.migratingOf, "migrationFailed": Apps.Copy.migrationFailed,
            "empty": Apps.Copy.empty,
            "chatEmpty": Apps.Copy.chatEmpty, "previewEmpty": Apps.Copy.previewEmpty,
            "versionsEmpty": Apps.Copy.versionsEmpty, "draftPlaceholder": Apps.Copy.draftPlaceholder,
            "starterTitle": Apps.Copy.starterTitle,
            "composerPlaceholder": Apps.Copy.composerPlaceholder, "noDevice": Apps.Copy.noDevice,
            "namePlaceholder": Apps.Copy.namePlaceholder, "descriptionPlaceholder": Apps.Copy.descriptionPlaceholder,
            "deleteTitle": Apps.Copy.deleteTitle,
            "deleteBody": Apps.Copy.deleteBody, "delete": Apps.Copy.delete,
            "settingsTitle": Apps.Copy.settingsTitle,
            "records": Apps.Copy.records,
            "recordsOne": Apps.Copy.recordsOne, "recordsOf": Apps.Copy.recordsOf,
            "recordsNone": Apps.Copy.recordsNone, "preview": Apps.Copy.preview,
        ]
        // Every string key of `copy`, both ways.
        let strings = copy.compactMapValues { $0 as? String }
        XCTAssertEqual(strings, flat)
        XCTAssertEqual(copy["menu"] as? [String: String], [
            "versions": Apps.Copy.Menu.versions, "settings": Apps.Copy.Menu.settings,
            "export": Apps.Copy.Menu.export, "delete": Apps.Copy.Menu.delete,
        ])
        XCTAssertEqual(copy["starters"] as? [String], Apps.Copy.starters)
        XCTAssertEqual(copy["tabs"] as? [String: String], [
            "chat": Apps.Copy.Tabs.chat, "preview": Apps.Copy.Tabs.preview, "data": Apps.Copy.Tabs.data,
            "versions": Apps.Copy.Tabs.versions, "settings": Apps.Copy.Tabs.settings,
        ])
        XCTAssertEqual(copy["automations"] as? [String: String], [
            "title": Apps.Copy.Automations.title, "schedule": Apps.Copy.Automations.schedule,
            "scheduleZone": Apps.Copy.Automations.scheduleZone, "create": Apps.Copy.Automations.create,
            "update": Apps.Copy.Automations.update, "delete": Apps.Copy.Automations.delete,
            "event": Apps.Copy.Automations.event, "webhook": Apps.Copy.Automations.webhook,
            "paused": Apps.Copy.Automations.paused,
            "failed": Apps.Copy.Automations.failed, "ok": Apps.Copy.Automations.ok,
            "never": Apps.Copy.Automations.never,
        ])
        XCTAssertEqual(Apps.Copy.viewing(2), "Viewing v2")
        XCTAssertEqual(Apps.Copy.restored(1, as: 4), "Restored v1 as v4")
        XCTAssertEqual(Apps.Copy.publishedToast(3), "Published v3")
        XCTAssertEqual(Apps.Copy.deleteTitle("Team dashboard"), "Delete Team dashboard?")
    }

    func testPanesPreviewAndTestIdsMatchTheFixture() throws {
        let fixture = try fixture()
        // Exponential ships no built-in apps (2026-10-24).
        XCTAssertNil(fixture["builtin"])
        XCTAssertEqual(fixture["panes"] as? [String: Double], [
            "chat": Apps.Panes.chat, "preview": Apps.Panes.preview,
            "chatMinWidth": Apps.Panes.chatMinWidth, "breakpoint": Apps.Panes.breakpoint,
        ])
        let preview = try XCTUnwrap(fixture["preview"] as? [String: Any])
        XCTAssertEqual(preview["radius"] as? Double, Apps.Preview.radius)
        XCTAssertEqual(preview["padding"] as? Double, Apps.Preview.padding)
        XCTAssertEqual(preview["surfaceId"] as? String, Apps.Preview.surfaceId)
        // VAPP-179: the preview toolbar (web ≥ md + desktop; phones have
        // none, so its frames are pinned here only as keys).
        XCTAssertEqual(Set(preview.keys), ["radius", "padding", "surfaceId", "$comment", "frames", "defaultFrame", "phoneWidth"])
        XCTAssertEqual(fixture["testIds"] as? [String: String], [
            "list": Apps.TestIds.list, "row": Apps.TestIds.row, "newApp": Apps.TestIds.newApp,
            "editor": Apps.TestIds.editor, "chat": Apps.TestIds.chat, "preview": Apps.TestIds.preview,
            "versions": Apps.TestIds.versions, "versionRow": Apps.TestIds.versionRow, "tabs": Apps.TestIds.tabs,
            "composer": Apps.TestIds.composer, "viewing": Apps.TestIds.viewing,
            "importApp": Apps.TestIds.importApp, "starter": Apps.TestIds.starter,
            "menu": Apps.TestIds.menu, "settings": Apps.TestIds.settings,
            "versionPreview": Apps.TestIds.versionPreview, "versionRestore": Apps.TestIds.versionRestore,
            "publish": Apps.TestIds.publish, "versionPublish": Apps.TestIds.versionPublish,
            "data": Apps.TestIds.data,
            "previewToolbar": Apps.TestIds.previewToolbar, "previewFrame": Apps.TestIds.previewFrame,
            "previewTemplate": Apps.TestIds.previewTemplate, "previewRunAs": Apps.TestIds.previewRunAs,
            "automations": Apps.TestIds.automations, "automationRow": Apps.TestIds.automationRow,
        ])
        // VAPP-177: the floating layout and its chat card are gone.
        XCTAssertNil(fixture["layout"])
        XCTAssertNil(fixture["layoutCases"])
        XCTAssertNil(fixture["dockCard"])
        XCTAssertNil(fixture["chatCardCases"])
    }

    func testCapsMatchTheContract() throws {
        let apps = try XCTUnwrap(try repoFile("packages/domain-contract/contract.json")["apps"] as? [String: Any])
        XCTAssertEqual(apps["slugMax"] as? Int, Apps.slugMax)
        XCTAssertEqual(apps["nameMax"] as? Int, Apps.nameMax)
        XCTAssertEqual(apps["descriptionMax"] as? Int, Apps.descriptionMax)
        XCTAssertEqual(apps["noteMax"] as? Int, Apps.noteMax)
        XCTAssertEqual(apps["packageMaxBytes"] as? Int, Apps.packageMaxBytes)
        XCTAssertEqual(apps["templateMaxComponents"] as? Int, Apps.templateMaxComponents)
        XCTAssertEqual(apps["slugPattern"] as? String, Apps.slugPattern)
        XCTAssertEqual(apps["kindPattern"] as? String, Apps.kindPattern)
        XCTAssertEqual(apps["recordMaxBytes"] as? Int, Apps.recordMaxBytes)
        XCTAssertEqual(apps["recordKindsMax"] as? Int, Apps.recordKindsMax)
    }

    func testSlugifyCases() throws {
        for item in try cases("slugifyCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            let taken = item["taken"] as? [String] ?? []
            XCTAssertEqual(Apps.slugify(name, taken: taken), item["slug"] as? String, name)
        }
    }

    func testSlugsMatchTheContractPattern() throws {
        let pattern = try NSRegularExpression(pattern: Apps.slugPattern)
        for item in try cases("slugifyCases") {
            let slug = Apps.slugify(try XCTUnwrap(item["name"] as? String), taken: item["taken"] as? [String] ?? [])
            XCTAssertNotNil(pattern.firstMatch(in: slug, range: NSRange(slug.startIndex..., in: slug)), slug)
        }
    }

    func testVersionLabelCases() throws {
        for item in try cases("versionLabelCases") {
            let head = try XCTUnwrap(item["headVersion"] as? Int)
            XCTAssertEqual(Apps.versionLabel(head), item["label"] as? String)
        }
    }

    // eApps R8 (#VAPP-233): `versionMigration` replays `migrationCases`.
    func testMigrationCases() throws {
        let block = try XCTUnwrap(try fixture()["migrationCases"] as? [String: Any])
        let list = try XCTUnwrap(block["cases"] as? [[String: Any]])
        XCTAssertFalse(list.isEmpty)
        for item in list {
            let row = try XCTUnwrap(item["row"] as? [String: Any])
            let got = Apps.versionMigration(state: row["migrationState"] as? String, total: row["migrationTotal"] as? Int ?? 0, done: row["migrationDone"] as? Int ?? 0, error: row["migrationError"] as? String)
            XCTAssertEqual(got, item["expected"] as? String, item["name"] as? String ?? "")
        }
    }

    // eApps V1 (#VAPP-210): `shownVersion`, `canPublish`, `grantVersion`,
    // `listPill` and `versionRow` replay `publishCases`.
    func testPublishCases() throws {
        let block = try XCTUnwrap(try fixture()["publishCases"] as? [String: Any])
        let list = try XCTUnwrap(block["cases"] as? [[String: Any]])
        XCTAssertFalse(list.isEmpty)
        for item in list {
            let name = item["name"] as? String ?? ""
            let raw = try XCTUnwrap(item["app"] as? [String: Any])
            let app = Apps.Versions(
                headVersion: try XCTUnwrap(raw["headVersion"] as? Int),
                publishedVersion: try XCTUnwrap(raw["publishedVersion"] as? Int)
            )
            let editing = try XCTUnwrap(item["editing"] as? Bool)
            let viewing = item["viewing"] as? Int
            XCTAssertEqual(Apps.shownVersion(app, editing: editing, viewing: viewing), item["shown"] as? Int, name)
            XCTAssertEqual(Apps.canPublish(app), item["canPublish"] as? Bool, name)
            XCTAssertEqual(Apps.grantVersion(app), item["grant"] as? Int, name)
            XCTAssertEqual(Apps.listPill(app), item["pill"] as? String, name)
            let rows = try XCTUnwrap(item["rows"] as? [[Any]])
            for row in rows {
                let number = try XCTUnwrap(row[0] as? Int)
                XCTAssertEqual(Apps.versionRow(app, number: number).rawValue, row[1] as? String, "\(name) v\(number)")
            }
        }
    }

    // #VAPP-220: `automationRow` replays `automationRowCases`.
    func testAutomationRowCases() throws {
        let block = try XCTUnwrap(try fixture()["automationRowCases"] as? [String: Any])
        let list = try XCTUnwrap(block["cases"] as? [[String: Any]])
        XCTAssertFalse(list.isEmpty)
        for item in list {
            let name = item["name"] as? String ?? ""
            let raw = try XCTUnwrap(item["row"] as? [String: Any])
            let on = try XCTUnwrap(raw["on"] as? [String: Any])
            let trigger: Apps.AutomationTrigger
            if let schedule = on["schedule"] as? [String: Any] {
                trigger = .schedule(cron: try XCTUnwrap(schedule["cron"] as? String), tz: schedule["tz"] as? String)
            } else if let record = on["record"] as? [String: Any] {
                trigger = .record(kind: try XCTUnwrap(record["kind"] as? String), op: try XCTUnwrap(record["op"] as? String))
            } else if let webhook = on["webhook"] as? [String: Any] {
                trigger = .webhook(try XCTUnwrap(webhook["name"] as? String))
            } else {
                trigger = .event(try XCTUnwrap(on["event"] as? String))
            }
            let row = Apps.AutomationRowInput(
                name: try XCTUnwrap(raw["name"] as? String),
                on: trigger,
                paused: raw["paused"] as? String,
                lastStatus: raw["lastStatus"] as? String
            )
            let expected = try XCTUnwrap(item["expected"] as? [String: Any])
            let result = Apps.automationRow(row)
            XCTAssertEqual(result.trigger, expected["trigger"] as? String, name)
            XCTAssertEqual(result.status.rawValue, expected["status"] as? String, name)
            XCTAssertEqual(result.label, expected["label"] as? String, name)
        }
    }

    func testEditorTabsMatchTheFixture() throws {
        let tabs = try XCTUnwrap(try fixture()["editorTabs"] as? [String: Any])
        XCTAssertEqual(tabs["wide"] as? [String], Apps.EditorTabs.wide)
        XCTAssertEqual(tabs["phone"] as? [String], Apps.EditorTabs.phone)
        XCTAssertEqual(tabs["menu"] as? [String], Apps.EditorTabs.menu)
        XCTAssertEqual(tabs["default"] as? String, Apps.EditorTabs.default)
        XCTAssertEqual(tabs["phoneDefault"] as? String, Apps.EditorTabs.phoneDefault)
        XCTAssertEqual(tabs["settingsMaxWidth"] as? Double, Apps.EditorTabs.settingsMaxWidth)
        XCTAssertEqual(tabs["search"] as? String, Apps.EditorTabs.search)
        XCTAssertEqual(tabs.count, 7)
    }

    func testEditorTabCases() throws {
        for item in try cases("editorTabCases") {
            let requested = item["requested"] as? String
            let phone = try XCTUnwrap(item["phone"] as? Bool)
            let target = Apps.editorTab(requested: requested, phone: phone)
            let label = "\(requested ?? "nil") phone=\(phone)"
            XCTAssertEqual(target.tab, item["tab"] as? String, label)
            XCTAssertEqual(target.sheet, item["sheet"] as? String, label)
        }
    }

    func testIsDraft() {
        XCTAssertTrue(Apps.isDraft(headVersion: 0))
        XCTAssertFalse(Apps.isDraft(headVersion: 1))
    }

    func testSortAppsCases() throws {
        struct Row { let slug: String; let name: String }
        for item in try cases("sortAppsCases") {
            let rows = try XCTUnwrap(item["apps"] as? [[String: Any]]).map {
                XCTAssertNil($0["builtin"])
                return Row(slug: $0["slug"] as? String ?? "", name: $0["name"] as? String ?? "")
            }
            let sorted = Apps.sortApps(rows, name: \.name).map(\.slug)
            XCTAssertEqual(sorted, item["order"] as? [String])
        }
    }

    func testEditorBriefMatchesTheReference() {
        let brief = Apps.editorBrief(name: "Team dashboard", slug: "team-dashboard", headVersion: 3, description: "  Feedback at a glance ")
        XCTAssertTrue(brief.hasPrefix(
            "You are the editor session of the Exponential app \"Team dashboard\" (slug `team-dashboard`, v3). It is for: Feedback at a glance\n"
        ))
        XCTAssertTrue(brief.contains("exponential_apps {op: \"upsert\", slug: \"team-dashboard\", package, note} (members keep running the published version)"))
        XCTAssertTrue(brief.contains("publish with exponential_apps {op: \"publish\", slug: \"team-dashboard\"} only when the person asks"))
        XCTAssertTrue(brief.hasSuffix("keep replies to one or two lines and save drafts early."))
        XCTAssertFalse(brief.contains("DRAFT"))
        let draft = Apps.editorBrief(name: Apps.Copy.untitled, slug: "untitled-app", headVersion: 0, description: nil)
        XCTAssertTrue(draft.contains("(slug `untitled-app`, Draft).\nIt is a new DRAFT with a placeholder name"))
    }

    func testTemplateCases() throws {
        for item in try cases("templateCases") {
            let ids = try XCTUnwrap(item["templates"] as? [String])
            let templates = Dictionary(uniqueKeysWithValues: ids.map { ($0, ["components": [Any]()]) })
            let json = String(decoding: try JSONSerialization.data(withJSONObject: ["id": "a", "templates": templates]), as: UTF8.self)
            let label = "\(ids) picked=\(item["picked"] as? String ?? "nil")"
            XCTAssertEqual(Apps.previewTemplates(packageJSON: json), item["list"] as? [String], label)
            XCTAssertEqual(Apps.previewTemplate(packageJSON: json, picked: item["picked"] as? String), item["shown"] as? String, label)
        }
        XCTAssertEqual(Apps.previewTemplates(packageJSON: "not json"), [])
        XCTAssertNil(Apps.previewTemplate(packageJSON: "not json"))
    }

    // MARK: - VAPP-157 the media rule (`mediaPolicy.cases`)

    func testMediaPolicyConstantsMatchTheFixture() throws {
        let policy = try XCTUnwrap(try fixture()["mediaPolicy"] as? [String: Any])
        XCTAssertEqual(policy["dataMimePrefixes"] as? [String], Apps.mediaDataMimePrefixes)
        XCTAssertEqual(policy["deniedDataMimes"] as? [String], Apps.mediaDeniedDataMimes)
        XCTAssertEqual(policy["attachmentPath"] as? String, Apps.mediaAttachmentPath)
    }

    func testMediaPolicyCases() throws {
        let policy = try XCTUnwrap(try fixture()["mediaPolicy"] as? [String: Any])
        let origin = try XCTUnwrap(policy["origin"] as? String)
        let list = try XCTUnwrap(policy["cases"] as? [[String: Any]])
        XCTAssertFalse(list.isEmpty)
        for item in list {
            let src = try XCTUnwrap(item["src"] as? String)
            XCTAssertEqual(Apps.appMediaAllowed(src, origin: origin), try XCTUnwrap(item["expected"] as? Bool), src)
        }
        // A trailing slash on the origin changes nothing; no origin = relative + data only.
        let uuid = "0b6c1a52-3e1f-4d7a-9c55-2f8e4a1b7c90"
        XCTAssertTrue(Apps.appMediaAllowed("\(origin)/api/attachments/\(uuid)", origin: origin + "/"))
        XCTAssertFalse(Apps.appMediaAllowed("\(origin)/api/attachments/\(uuid)", origin: ""))
        XCTAssertTrue(Apps.appMediaAllowed("/api/attachments/\(uuid)", origin: ""))
        XCTAssertFalse(Apps.appMediaAllowed("/api/attachments/\(uuid)\n", origin: origin))
    }

    // MARK: - exp: sources (web `devicesValue`; the rest: AppSourcesTests)

    func testDevicesValueDerivesLikeTheWebHost() {
        let now = Date(timeIntervalSince1970: 1_760_000_000)
        func stamp(_ secondsAgo: Double) -> String {
            ISO8601DateFormatter().string(from: now.addingTimeInterval(-secondsAgo))
        }
        let value = AppHostSources.devicesValue([
            .init(id: "s", label: "build-box", kind: "server", platform: "linux", version: "1.2.0", icon: "server", lastSeenAt: stamp(3 * 3600)),
            .init(id: "n", label: "Never", kind: "desktop", platform: nil, version: nil, icon: "laptop", lastSeenAt: nil),
            .init(id: "o", label: "Alex's MacBook Pro", kind: "desktop", platform: "macos", version: nil, icon: "laptop", lastSeenAt: stamp(5)),
        ], now: now)
        XCTAssertEqual(value.rows.map(\.id), ["o", "s", "n"])
        XCTAssertEqual(value.count, 3)
        XCTAssertEqual(value.online, 1)
        XCTAssertEqual(value.rows[0], .init(id: "o", label: "Alex's MacBook Pro", detail: "Desktop · macos", icon: "laptop", discTone: "success", tone: "live", status: "Online"))
        XCTAssertEqual(value.rows[1].detail, "Server · linux · v1.2.0")
        XCTAssertEqual(value.rows[1].status, "Last seen 3 h ago")
        XCTAssertEqual(value.rows[1].tone, "idle")
        XCTAssertEqual(value.rows[1].discTone, "muted")
        XCTAssertEqual(value.rows[2].status, "Offline")
    }
}

import GRDB
import Foundation
import XCTest
@testable import ExpCore

// #EXP-2172: the CI page, replayed against packages/domain-contract/fixtures/
// ci-page.json (TS `ci-page.test.ts`, desktop `domain::ci_page`, Android
// `CiPageTest`), comparing the WHOLE result; `testTreesAreWebs` replays the
// genUI trees of fixtures/ci-page-view.json node for node.

private func str(_ value: Any?) -> String? { value as? String }
private func opt(_ value: String?) -> Any { value ?? NSNull() }

func ciPageInput(_ raw: [String: Any]) throws -> CiPage.Input {
    let runs = try ((raw["runs"] as? [[String: Any]]) ?? []).map { r in
        CiPage.Run(
            id: try XCTUnwrap(str(r["id"])), repositoryId: try XCTUnwrap(str(r["repositoryId"])),
            number: try XCTUnwrap(r["number"] as? Int), name: try XCTUnwrap(str(r["name"])),
            sha: str(r["sha"]) ?? "", ref: str(r["ref"]), trigger: try XCTUnwrap(str(r["trigger"])),
            state: try XCTUnwrap(str(r["state"])), pullRequestId: str(r["pullRequestId"]),
            createdByUserId: str(r["createdByUserId"]), provider: try XCTUnwrap(str(r["provider"])),
            startedAt: str(r["startedAt"]), completedAt: str(r["completedAt"]), createdAt: try XCTUnwrap(str(r["createdAt"]))
        )
    }
    let jobs = try ((raw["jobs"] as? [[String: Any]]) ?? []).map { j in
        CiPage.Job(
            runId: try XCTUnwrap(str(j["runId"])), status: try XCTUnwrap(str(j["status"])),
            conclusion: str(j["conclusion"]), runsOn: (j["runsOn"] as? [String]) ?? [],
            runnerDeviceId: str(j["runnerDeviceId"]), provider: str(j["provider"]) ?? "exponential"
        )
    }
    let repos = try ((raw["repos"] as? [[String: Any]]) ?? []).map {
        CiPage.Repo(id: try XCTUnwrap(str($0["id"])), fullName: try XCTUnwrap(str($0["fullName"])))
    }
    let prs = try ((raw["pullRequests"] as? [[String: Any]]) ?? []).map { p in
        CiPage.Pr(
            id: try XCTUnwrap(str(p["id"])), number: try XCTUnwrap(p["number"] as? Int),
            title: str(p["title"]) ?? "", repositoryId: str(p["repositoryId"]) ?? "",
            rootSessionId: str(p["rootSessionId"]), base: str(p["base"]) ?? "",
            mergeState: str(p["mergeState"]), mergePosition: p["mergePosition"] as? Int, mergeTrainId: str(p["mergeTrainId"])
        )
    }
    let sessions = try ((raw["sessions"] as? [[String: Any]]) ?? []).map {
        CiPage.Session(id: try XCTUnwrap(str($0["id"])), userId: try XCTUnwrap(str($0["userId"])))
    }
    let runners = try ((raw["runners"] as? [[String: Any]]) ?? []).map { r in
        CiPage.Runner(
            id: try XCTUnwrap(str(r["id"])), label: try XCTUnwrap(str(r["label"])), platform: str(r["platform"]),
            labels: (r["labels"] as? [String]) ?? [], slots: try XCTUnwrap(r["slots"] as? Int), lastSeenAt: str(r["lastSeenAt"])
        )
    }
    return CiPage.Input(
        runs: runs, jobs: jobs, repos: repos, pullRequests: prs, sessions: sessions, runners: runners,
        viewerId: try XCTUnwrap(str(raw["viewerId"])), now: str(raw["now"]) ?? ""
    )
}

func ciPageFilter(_ raw: Any?) throws -> CiPage.RunFilter {
    let f = try XCTUnwrap(raw as? [String: Any])
    return CiPage.RunFilter(
        status: try XCTUnwrap(CiPage.Filter(rawValue: try XCTUnwrap(str(f["status"])))),
        repositoryId: str(f["repositoryId"]),
        source: str(f["source"]).flatMap(CiPage.Source.init(rawValue:))
    )
}

final class CiPageTests: XCTestCase {
    private func fixture() throws -> [String: Any] { try ciFixture("ci-page.json") }
    private func scenario() throws -> CiPage.Input { try ciPageInput(try XCTUnwrap(try fixture()["scenario"] as? [String: Any])) }
    private func cases(_ key: String) throws -> [[String: Any]] {
        let list = try XCTUnwrap(try fixture()[key] as? [[String: Any]])
        XCTAssertFalse(list.isEmpty, key)
        return list
    }

    func testCopyLimitsAndIds() throws {
        let f = try fixture()
        let copy = try XCTUnwrap(f["copy"] as? [String: Any])
        XCTAssertEqual(copy["title"] as? String, CiPage.Copy.title)
        XCTAssertEqual(copy["tabs"] as? [String: String], CiPage.Copy.tabs)
        XCTAssertEqual(copy["tiles"] as? [String: String], [
            "running": CiPage.Copy.Tiles.running, "queued": CiPage.Copy.Tiles.queued,
            "failedToday": CiPage.Copy.Tiles.failedToday, "runners": CiPage.Copy.Tiles.runners,
            "runnersValue": CiPage.Copy.Tiles.runnersValue,
        ])
        XCTAssertEqual(copy["filters"] as? [String: String], CiPage.Copy.filters)
        XCTAssertEqual(copy["repositories"] as? [String: String], ["all": CiPage.Copy.allRepositories])
        XCTAssertEqual(copy["sources"] as? [String: String], CiPage.Copy.sources)
        XCTAssertEqual(copy["columns"] as? [String: String], CiPage.Copy.columns)
        XCTAssertEqual(copy["runsSection"] as? String, CiPage.Copy.runsSection)
        XCTAssertEqual(copy["trainSection"] as? String, CiPage.Copy.trainSection)
        XCTAssertEqual(copy["pullRequest"] as? String, CiPage.Copy.pullRequest)
        XCTAssertEqual(copy["triggers"] as? [String: String], CiPage.Copy.triggers)
        XCTAssertEqual(copy["unknownRepository"] as? String, CiPage.Copy.unknownRepository)
        XCTAssertEqual(copy["train"] as? [String: String], ["batch": CiPage.Copy.trainBatch])
        XCTAssertEqual(copy["runners"] as? [String: String], [
            "busy": CiPage.Copy.Runners.busy, "queue": CiPage.Copy.Runners.queue,
            "offlineSince": CiPage.Copy.Runners.offlineSince, "noLabels": CiPage.Copy.Runners.noLabels,
        ])
        XCTAssertEqual(copy["empty"] as? [String: String], [
            "runs": CiPage.Copy.Empty.runs, "filtered": CiPage.Copy.Empty.filtered,
            "train": CiPage.Copy.Empty.train, "runners": CiPage.Copy.Empty.runners,
        ])
        let limits = try XCTUnwrap(f["limits"] as? [String: Any])
        XCTAssertEqual(limits["runs"] as? Int, CiPage.Limits.runs)
        XCTAssertEqual((limits["runnerOnlineMs"] as? NSNumber)?.doubleValue, CiPage.Limits.runnerOnlineMs)
        let i = CiPage.TestIds.self
        XCTAssertEqual(f["testIds"] as? [String: String], [
            "page": i.page, "tiles": i.tiles, "tabs": i.tabs, "filters": i.filters, "statusFilter": i.statusFilter,
            "repoFilter": i.repoFilter, "sourceFilter": i.sourceFilter, "runs": i.runs, "runRow": i.runRow,
            "train": i.train, "trainRow": i.trainRow, "runners": i.runners, "runnerRow": i.runnerRow,
        ])
    }

    func testSpecimenIsTheScenario() throws {
        XCTAssertEqual(CiPage.specimen, try scenario())
    }

    func testRunList() throws {
        let input = try scenario()
        for c in try cases("runListCases") {
            let rows = CiPage.runList(input, filter: try ciPageFilter(c["filter"])).map { r -> [String: Any] in
                [
                    "id": r.id, "identifier": r.identifier, "title": r.title, "status": r.status.rawValue,
                    "statusLabel": r.statusLabel, "glyph": r.glyph.rawValue, "tone": r.tone.rawValue,
                    "repositoryId": r.repositoryId, "repository": r.repository, "source": r.source.rawValue,
                    "sourceLabel": r.sourceLabel, "ref": r.ref, "trigger": r.trigger, "duration": opt(r.duration),
                    "startedAt": opt(r.startedAt), "subtitle": r.subtitle,
                ]
            }
            XCTAssertEqual(rows as NSArray, c["expected"] as? NSArray, c["name"] as? String ?? "")
        }
    }

    func testRepoOptions() throws {
        let got = CiPage.repoOptions(try scenario()).map { ["label": $0.label, "value": $0.value] }
        XCTAssertEqual(got as NSArray, try fixture()["repoOptions"] as? NSArray)
    }

    func testTiles() throws {
        let got = CiPage.tiles(try scenario()).map { ["label": $0.label, "value": $0.value, "tone": $0.tone, "delta": opt($0.delta)] as [String: Any] }
        XCTAssertEqual(got as NSArray, try fixture()["tiles"] as? NSArray)
    }

    func testTabCounts() throws {
        let got = CiPage.tabCounts(try scenario())
        XCTAssertEqual(["train": got.train, "runners": got.runners] as NSDictionary, try fixture()["tabCounts"] as? NSDictionary)
    }

    func testTrainGroups() throws {
        let input = try scenario()
        for c in try cases("trainCases") {
            let got = CiPage.trainGroups(input, queue: try XCTUnwrap(c["queue"] as? Bool)).map { g -> [String: Any] in
                [
                    "repositoryId": g.repositoryId, "title": g.title, "caption": g.caption,
                    "rows": g.rows.map { r -> [String: Any] in
                        [
                            "id": r.id, "identifier": r.identifier, "title": r.title, "subtitle": opt(r.subtitle),
                            "glyph": r.glyph.rawValue, "badge": r.badge, "pullRequestId": r.pullRequestId,
                        ]
                    },
                ]
            }
            XCTAssertEqual(got as NSArray, c["expected"] as? NSArray, c["name"] as? String ?? "")
        }
    }

    func testRunnerRows() throws {
        let got = CiPage.runnerRows(try scenario()).map { r -> [String: Any] in
            [
                "id": r.id, "title": r.title, "subtitle": r.subtitle, "online": r.online,
                "busy": opt(r.busy), "queue": opt(r.queue), "offlineSince": opt(r.offlineSince),
            ]
        }
        XCTAssertEqual(got as NSArray, try fixture()["runners"] as? NSArray)
    }

    func testBadge() throws {
        for c in try cases("badgeCases") {
            var raw = try XCTUnwrap(c["input"] as? [String: Any])
            raw["now"] = ""
            let got = CiPage.ciBadge(try ciPageInput(raw))
            XCTAssertEqual(["count": got.count, "tone": got.tone.rawValue] as NSDictionary, c["expected"] as? NSDictionary, c["name"] as? String ?? "")
        }
    }

    func testTreesAreWebs() throws {
        let scenario = try scenario()
        var empty = scenario
        empty.runs = []
        empty.jobs = []
        empty.pullRequests = []
        empty.runners = []
        let list = try XCTUnwrap(try ciFixture("ci-page-view.json")["cases"] as? [[String: Any]])
        XCTAssertFalse(list.isEmpty)
        for c in list {
            let input = str(c["world"]) == "empty" ? empty : scenario
            let tree = CiPageView.ciPageTree(input, state: CiPageView.State(
                tab: try XCTUnwrap(CiPage.Tab(rawValue: try XCTUnwrap(str(c["tab"])))),
                filter: try ciPageFilter(c["filter"]),
                phone: c["phone"] as? Bool == true
            ))
            let expected = try XCTUnwrap(CardJSON(any: c["tree"]))
            XCTAssertEqual(expected, tree, c["name"] as? String ?? "")
            if expected != tree { XCTFail("\(c["name"] ?? ""): \(tree.compactString)") }
        }
    }

    func testPagePath() {
        XCTAssertTrue(CiPage.isPagePath("/ci"))
        XCTAssertTrue(CiPage.isPagePath("/ci/?tab=train#x"))
        XCTAssertFalse(CiPage.isPagePath("/t/acme/ci/r12"))
        XCTAssertFalse(CiPage.isPagePath("/cid"))
        XCTAssertEqual(AppLink.classify("/ci", origin: "https://app.example.com"), .app(path: "/ci"))
    }

    func testStateEvents() {
        let state = CiPageView.State(phone: true)
        XCTAssertEqual(CiPageView.stateAfter(state, event: CiPageView.Events.tab, payload: ["value": .string("train")])?.tab, .train)
        XCTAssertEqual(CiPageView.stateAfter(state, event: CiPageView.Events.status, payload: ["value": .string("mine")])?.filter.status, .mine)
        let repo = CiPageView.stateAfter(state, event: CiPageView.Events.repository, payload: ["value": .string("repo-site")])
        XCTAssertEqual(repo?.filter.repositoryId, "repo-site")
        XCTAssertNil(CiPageView.stateAfter(repo!, event: CiPageView.Events.repository, payload: ["value": .string("all")])?.filter.repositoryId)
        XCTAssertEqual(CiPageView.stateAfter(state, event: CiPageView.Events.source, payload: ["value": .string("github")])?.filter.source, .github)
        XCTAssertNil(CiPageView.stateAfter(state, event: CiPageView.Events.openRun, payload: nil))
    }
}

// #EXP-2172: the synced rows as the page's input.
final class CiPageRowsTests: XCTestCase {
    func testFetchRowsReadsTheShapes() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("ci-page-rows-\(UUID().uuidString).sqlite")
        let pool = try DatabasePool(path: url.path)
        try DatabaseManager.runMigrations(on: pool)
        try pool.write { db in
            try CiRunEntity(
                id: "g1", teamId: "t", repositoryId: "repo", number: 7, name: "build", ref: "main", state: "failure",
                createdByUserId: "u-me", completedAt: "2026-11-27T10:00:00.000Z", createdAt: "2026-11-27T09:00:00.000Z",
                updatedAt: "x", provider: "github"
            ).save(db)
            try CiJobEntity(
                id: "j1", teamId: "t", runId: "g1", check: "build", runsOn: "[\"linux\"]", status: "completed",
                conclusion: "failure", createdAt: "x", updatedAt: "x", provider: "github"
            ).save(db)
            try db.execute(sql: """
                INSERT INTO "devices" ("id", "user_id", "device_id", "label", "runner")
                VALUES ('d1', 'u', 'mint-id', '', '{"enabled":true,"labels":["gpu"],"slots":2}'),
                       ('d2', 'u', 'mac', 'Mac', NULL)
                """)
        }
        let input = try pool.read { db in try CiPage.fetchRows(db, viewerId: "u-me") }
        XCTAssertEqual(input.runs.map(\.provider), ["github"])
        XCTAssertEqual(input.jobs.first?.runsOn, ["linux"])
        XCTAssertEqual(input.runners.map(\.label), ["mint-id"])
        XCTAssertEqual(input.runners.first?.slots, 2)
        XCTAssertEqual(CiPage.ciBadge(input), NavBadges.Value(count: 1, tone: .attention))
    }
}

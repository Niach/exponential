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
            startedAt: str(r["startedAt"]), completedAt: str(r["completedAt"]), createdAt: try XCTUnwrap(str(r["createdAt"])),
            title: str(r["title"]), mergeTrainId: str(r["mergeTrainId"])
        )
    }
    let jobs = try ((raw["jobs"] as? [[String: Any]]) ?? []).map { j in
        CiPage.Job(
            id: str(j["id"]) ?? "", runId: try XCTUnwrap(str(j["runId"])), check: str(j["check"]) ?? "",
            status: try XCTUnwrap(str(j["status"])), conclusion: str(j["conclusion"]), runsOn: (j["runsOn"] as? [String]) ?? [],
            runnerDeviceId: str(j["runnerDeviceId"]), provider: str(j["provider"]) ?? "exponential"
        )
    }
    let repos = try ((raw["repos"] as? [[String: Any]]) ?? []).map(ciPageRepo)
    let prs = try ((raw["pullRequests"] as? [[String: Any]]) ?? []).map { p in
        CiPage.Pr(
            id: try XCTUnwrap(str(p["id"])), number: try XCTUnwrap(p["number"] as? Int),
            title: str(p["title"]) ?? "", repositoryId: str(p["repositoryId"]) ?? "",
            rootSessionId: str(p["rootSessionId"]), base: str(p["base"]) ?? "", state: str(p["state"]) ?? "open",
            mergeState: str(p["mergeState"]), mergeWait: str(p["mergeWait"]), mergeHold: str(p["mergeHold"]),
            mergeNow: p["mergeNow"] as? Bool ?? false, mergeAttempts: p["mergeAttempts"] as? Int ?? 0,
            mergePosition: p["mergePosition"] as? Int, mergeTrainId: str(p["mergeTrainId"])
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
    let users = try ((raw["users"] as? [[String: Any]]) ?? []).map {
        CiPage.User(id: try XCTUnwrap(str($0["id"])), name: try XCTUnwrap(str($0["name"])), image: str($0["image"]))
    }
    let trains = try ((raw["trains"] as? [[String: Any]]) ?? []).map { t in
        CiPage.Train(
            id: try XCTUnwrap(str(t["id"])), repositoryId: try XCTUnwrap(str(t["repositoryId"])), base: str(t["base"]) ?? "",
            baseSha: str(t["baseSha"]), units: (t["units"] as? [[String]]) ?? [], state: try XCTUnwrap(str(t["state"])),
            attempt: t["attempt"] as? Int ?? 0, bisectOf: str(t["bisectOf"]), pending: (t["pending"] as? [[[String]]]) ?? [],
            untested: t["untested"] as? Bool ?? false, testedSha: str(t["testedSha"]),
            createdAt: str(t["createdAt"]) ?? "", finishedAt: str(t["finishedAt"])
        )
    }
    let events = try ((raw["trainEvents"] as? [[String: Any]]) ?? []).map { e in
        CiPage.TrainEvent(
            id: try XCTUnwrap(str(e["id"])), repositoryId: try XCTUnwrap(str(e["repositoryId"])),
            pullRequestId: try XCTUnwrap(str(e["pullRequestId"])), mergeTrainId: str(e["mergeTrainId"]),
            kind: try XCTUnwrap(str(e["kind"])), base: str(e["base"]) ?? "", attempt: e["attempt"] as? Int ?? 0,
            reason: str(e["reason"]), checks: (e["checks"] as? [String]) ?? [], ciJobId: str(e["ciJobId"]),
            jobUrl: str(e["jobUrl"]), files: (e["files"] as? [String]) ?? [], sha: str(e["sha"]),
            byUserId: str(e["byUserId"]), createdAt: str(e["createdAt"]) ?? ""
        )
    }
    return CiPage.Input(
        runs: runs, jobs: jobs, repos: repos, pullRequests: prs, sessions: sessions, runners: runners, users: users,
        trains: trains, trainEvents: events,
        viewerId: try XCTUnwrap(str(raw["viewerId"])), now: str(raw["now"]) ?? "",
        utcOffsetMinutes: raw["utcOffsetMinutes"] as? Int ?? 0
    )
}

func ciPageRepo(_ raw: [String: Any]) throws -> CiPage.Repo {
    CiPage.Repo(id: try XCTUnwrap(str(raw["id"])), fullName: try XCTUnwrap(str(raw["fullName"])), defaultBranch: str(raw["defaultBranch"]))
}

/// `CiPage.RunContext` as the fixture's JSON.
func ciRunContextJSON(_ c: CiPage.RunContext) -> [String: Any] {
    [
        "kind": c.kind.rawValue, "badge": c.badge, "title": opt(c.title), "caption": opt(c.caption),
        "outcome": c.outcome.map { o -> [String: Any] in
            [
                "pullRequestId": o.pullRequestId, "identifier": o.identifier, "title": o.title, "text": o.text,
                "glyph": o.glyph, "tone": o.tone, "jobId": opt(o.jobId),
            ]
        },
    ]
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
        let c = CiPage.Copy.self
        XCTAssertEqual(copy["title"] as? String, c.title)
        XCTAssertEqual(copy["tabs"] as? [String: String], c.tabs)
        XCTAssertEqual(copy["phoneTabs"] as? [String: String], c.phoneTabs)
        XCTAssertEqual(copy["filters"] as? [String: String], c.filters)
        XCTAssertEqual(copy["filterLabels"] as? [String: String], c.filterLabels)
        XCTAssertEqual(copy["repositories"] as? [String: String], ["all": c.allRepositories])
        XCTAssertEqual(copy["sources"] as? [String: String], c.sources)
        XCTAssertEqual(copy["columns"] as? [String: String], c.columns)
        XCTAssertEqual(copy["pullRequest"] as? String, c.pullRequest)
        XCTAssertEqual(copy["triggers"] as? [String: String], c.triggers)
        XCTAssertEqual(copy["unknownRepository"] as? String, c.unknownRepository)
        XCTAssertEqual(copy["inProgress"] as? String, c.inProgress)
        XCTAssertEqual(copy["today"] as? String, c.today)
        XCTAssertEqual(copy["yesterday"] as? String, c.yesterday)
        XCTAssertEqual(copy["months"] as? [String], c.months)
        XCTAssertEqual(copy["queued"] as? String, c.queued)
        XCTAssertEqual(copy["batchTitle"] as? String, c.batchTitle)
        XCTAssertEqual(copy["batchOne"] as? String, c.batchOne)
        XCTAssertEqual(copy["runnerChip"] as? String, c.runnerChip)
        XCTAssertEqual(copy["waitingForRunner"] as? String, c.waitingForRunner)
        XCTAssertEqual(copy["kinds"] as? [String: String], c.kinds)
        XCTAssertEqual(copy["prs"] as? [String: String], ["open": c.Prs.open, "branches": c.Prs.branches, "closed": c.Prs.closed])
        XCTAssertEqual(Set(copy.keys).count, 25)
        let q = c.Queue.self
        let queue = try XCTUnwrap(copy["queue"] as? [String: Any])
        let strings: [String: String] = [
            "testing": q.testing, "base": q.base, "rerun": q.rerun, "split": q.split, "untested": q.untested,
            "runningFor": q.runningFor, "left": q.left, "almostDone": q.almostDone, "batch": q.batch, "openRun": q.openRun,
            "upNext": q.upNext, "position": q.position, "mergeNow": q.mergeNow, "notReady": q.notReady, "sentBack": q.sentBack,
            "failedIn": q.failedIn, "conflictIn": q.conflictIn, "tries": q.tries, "landed": q.landed, "landedWith": q.landedWith,
            "mergedNow": q.mergedNow, "openLog": q.openLog, "resume": q.resume, "more": q.more, "into": q.into,
            "whatHappened": q.whatHappened,
        ]
        for (key, value) in strings { XCTAssertEqual(queue[key] as? String, value, key) }
        XCTAssertEqual(queue["menu"] as? [String: String], q.menu)
        XCTAssertEqual(queue["stats"] as? [String: String], q.stats)
        XCTAssertEqual(queue["waits"] as? [String: String], q.waits)
        XCTAssertEqual(queue["holds"] as? [String: String], q.holds)
        XCTAssertEqual(queue["outcome"] as? [String: String], q.outcome)
        XCTAssertEqual(Set(queue.keys).count, strings.count + 5)
        XCTAssertEqual(copy["runners"] as? [String: String], [
            "busy": c.Runners.busy, "queue": c.Runners.queue,
            "offlineSince": c.Runners.offlineSince, "noLabels": c.Runners.noLabels,
        ])
        XCTAssertEqual(copy["empty"] as? [String: String], [
            "runs": c.Empty.runs, "filtered": c.Empty.filtered, "train": c.Empty.train,
            "prs": c.Empty.prs, "releases": c.Empty.releases, "runners": c.Empty.runners,
        ])
        let limits = try XCTUnwrap(f["limits"] as? [String: Any])
        XCTAssertEqual(limits["runs"] as? Int, CiPage.Limits.runs)
        XCTAssertEqual((limits["runnerOnlineMs"] as? NSNumber)?.doubleValue, CiPage.Limits.runnerOnlineMs)
        XCTAssertEqual(limits["subjects"] as? Int, CiPage.Limits.subjects)
        XCTAssertEqual(limits["subjectRuns"] as? Int, CiPage.Limits.subjectRuns)
        XCTAssertEqual(limits["history"] as? Int, CiPage.Limits.history)
        XCTAssertEqual((limits["historyHours"] as? NSNumber)?.doubleValue, CiPage.Limits.historyHours)
        XCTAssertEqual(limits["etaSamples"] as? Int, CiPage.Limits.etaSamples)
        XCTAssertEqual(limits.count, 7)
        let i = CiPage.TestIds.self
        XCTAssertEqual(f["testIds"] as? [String: String], [
            "page": i.page, "tabs": i.tabs, "filters": i.filters, "statusFilter": i.statusFilter,
            "repoFilter": i.repoFilter, "sourceFilter": i.sourceFilter, "runs": i.runs, "runRow": i.runRow,
            "runners": i.runners, "runnerRow": i.runnerRow, "runnerChip": i.runnerChip, "queue": i.queue,
            "queueRepo": i.queueRepo, "queueStats": i.queueStats, "queueNow": i.queueNow, "queueRow": i.queueRow,
            "prs": i.prs, "prRow": i.prRow, "releases": i.releases,
        ])
        let tones = try XCTUnwrap(f["statusTones"] as? [String: [String: String]])
        for status in [CiRun.Status.passed, .failed, .running, .queued, .cancelled] {
            let (glyph, tone) = CiPage.statusTone(status)
            XCTAssertEqual(tones[status.rawValue], ["glyph": glyph.rawValue, "tone": tone], status.rawValue)
        }
    }

    func testSpecimenIsTheScenario() throws {
        XCTAssertEqual(CiPage.specimen, try scenario())
    }

    private static func userJSON(_ u: CiPage.User?) -> Any {
        u.map { ["id": $0.id, "name": $0.name, "image": opt($0.image)] as [String: Any] } ?? NSNull()
    }

    private static func pillJSON(_ p: CiPage.Pill?) -> Any {
        p.map { ["text": $0.text, "tone": $0.tone] as [String: Any] } ?? NSNull()
    }

    private static func rowJSON(_ r: CiPage.RunListRow) -> [String: Any] {
        [
            "id": r.id, "status": r.status.rawValue, "statusLabel": r.statusLabel, "glyph": r.glyph.rawValue, "tone": r.tone,
            "title": r.title, "workflow": r.workflow, "repositoryId": r.repositoryId, "repository": r.repository,
            "repo": r.repo, "ref": r.ref, "sha": r.sha, "trigger": r.trigger, "source": r.source.rawValue,
            "sourceLabel": r.sourceLabel, "sourceIcon": r.sourceIcon, "duration": opt(r.duration), "at": r.at,
            "author": userJSON(r.author),
            "subtitle": r.subtitle, "phoneSubtitle": r.phoneSubtitle, "phoneMeta": r.phoneMeta,
        ]
    }

    private static func groupsJSON(_ groups: [CiPage.RunGroup]) -> NSArray {
        groups.map { g -> [String: Any] in
            ["id": g.id, "title": g.title, "count": g.count.map { $0 as Any } ?? NSNull(), "rows": g.rows.map(rowJSON)]
        } as NSArray
    }

    private static func queueRowJSON(_ r: CiPage.QueueRow) -> [String: Any] {
        [
            "id": r.id, "pullRequestId": r.pullRequestId, "identifier": r.identifier, "title": r.title,
            "subtitle": opt(r.subtitle), "glyph": r.glyph, "tone": r.tone, "badge": pillJSON(r.badge), "at": opt(r.at),
            "action": opt(r.action?.rawValue), "runId": opt(r.runId), "jobId": opt(r.jobId), "url": opt(r.url), "menu": r.menu,
        ]
    }

    private static func queueJSON(_ repos: [CiPage.QueueRepo]) -> NSArray {
        repos.map { repo -> [String: Any] in
            [
                "repositoryId": repo.repositoryId, "title": repo.title, "caption": repo.caption,
                "stats": [
                    "inQueue": repo.stats.inQueue, "landedToday": repo.stats.landedToday,
                    "sentBackToday": repo.stats.sentBackToday, "typicalRun": opt(repo.stats.typicalRun),
                ] as [String: Any],
                "testing": repo.testing.map { n -> [String: Any] in
                    [
                        "trainId": n.trainId, "title": n.title, "subtitle": n.subtitle, "status": n.status.rawValue,
                        "runId": opt(n.runId), "elapsed": n.elapsed, "eta": opt(n.eta),
                        "progress": n.progress.map { $0 as Any } ?? NSNull(),
                        "members": n.members.map { m -> [String: Any] in
                            ["pullRequestId": m.pullRequestId, "identifier": m.identifier, "title": m.title, "author": userJSON(m.author)]
                        },
                    ]
                },
                "upNext": repo.upNext.map(queueRowJSON), "waiting": repo.waiting.map(queueRowJSON),
                "sentBack": repo.sentBack.map(queueRowJSON), "landed": repo.landed.map(queueRowJSON),
            ]
        } as NSArray
    }

    private static func subjectsJSON(_ groups: [CiPage.SubjectGroup]) -> NSArray {
        groups.map { g -> [String: Any] in
            [
                "id": g.id.rawValue, "title": g.title, "count": g.count, "open": g.open,
                "rows": g.rows.map { r -> [String: Any] in
                    [
                        "id": r.id, "pullRequestId": opt(r.pullRequestId), "identifier": opt(r.identifier), "title": r.title,
                        "status": r.status.rawValue, "glyph": r.glyph.rawValue, "tone": r.tone, "subtitle": r.subtitle,
                        "phoneSubtitle": r.phoneSubtitle, "meta": r.meta, "pill": pillJSON(r.pill), "runId": r.runId,
                        "runs": r.runs.map(rowJSON),
                    ]
                },
            ]
        } as NSArray
    }

    func testRunGroups() throws {
        let input = try scenario()
        for c in try cases("runGroupsCases") {
            XCTAssertEqual(Self.groupsJSON(CiPage.runGroups(input, filter: try ciPageFilter(c["filter"]))), c["expected"] as? NSArray, c["name"] as? String ?? "")
        }
    }

    func testReleaseGroups() throws {
        let input = try scenario()
        for c in try cases("releaseGroupsCases") {
            let got = CiPage.runGroups(input, filter: try ciPageFilter(c["filter"]), kinds: [.release])
            XCTAssertEqual(Self.groupsJSON(got), c["expected"] as? NSArray, c["name"] as? String ?? "")
        }
    }

    func testPrGroups() throws {
        let input = try scenario()
        for c in try cases("prGroupsCases") {
            XCTAssertEqual(Self.subjectsJSON(CiPage.prGroups(input, filter: try ciPageFilter(c["filter"]))), c["expected"] as? NSArray, c["name"] as? String ?? "")
        }
    }

    func testQueue() throws {
        let raw = try XCTUnwrap(try fixture()["scenario"] as? [String: Any])
        for c in try cases("queueCases") {
            var merged = raw
            for (key, value) in (c["input"] as? [String: Any]) ?? [:] { merged[key] = value }
            let got = Self.queueJSON(CiPage.queue(try ciPageInput(merged)))
            XCTAssertEqual(got, c["expected"] as? NSArray, c["name"] as? String ?? "")
        }
    }

    func testKinds() throws {
        for c in try cases("kindCases") {
            let r = try XCTUnwrap(c["run"] as? [String: Any])
            let run = CiPage.Run(
                id: "x", repositoryId: str(r["repositoryId"]) ?? "", number: 1, name: "x", ref: str(r["ref"]),
                trigger: try XCTUnwrap(str(r["trigger"])), state: "success", pullRequestId: str(r["pullRequestId"]),
                createdByUserId: nil, provider: "exponential", startedAt: nil, completedAt: nil, createdAt: "",
                mergeTrainId: str(r["mergeTrainId"])
            )
            let repos = try ((c["repos"] as? [[String: Any]]) ?? []).map(ciPageRepo)
            XCTAssertEqual(CiPage.runKind(run, repos: repos).rawValue, c["expected"] as? String, c["name"] as? String ?? "")
        }
    }

    func testRunContext() throws {
        let input = try scenario()
        for c in try cases("runContextCases") {
            let id = try XCTUnwrap(str(c["run"]))
            let run = try XCTUnwrap(input.runs.first { $0.id == id })
            XCTAssertEqual(ciRunContextJSON(CiPage.runContext(run, input)) as NSDictionary, c["expected"] as? NSDictionary, id)
        }
    }

    func testTypicalTrain() throws {
        let input = try scenario()
        let expected = try XCTUnwrap(try fixture()["typicalTrainMs"] as? [String: Any])
        for (repo, ms) in expected {
            XCTAssertEqual(CiPage.typicalTrainMs(input, repositoryId: repo), (ms as? NSNumber)?.doubleValue, repo)
        }
    }

    func testEta() throws {
        for c in try cases("etaCases") {
            let got = CiPage.trainEta(typicalMs: (c["typicalMs"] as? NSNumber)?.doubleValue, elapsedMs: try XCTUnwrap(c["elapsedMs"] as? NSNumber).doubleValue)
            let json: Any = got.map { ["eta": $0.eta, "progress": $0.progress] as NSDictionary } ?? NSNull()
            XCTAssertEqual(json as? NSObject, (c["expected"] as? NSObject) ?? NSNull(), "\(c)")
        }
    }

    func testDayTitle() throws {
        for c in try cases("dayTitleCases") {
            XCTAssertEqual(
                CiPage.dayTitle(try XCTUnwrap(str(c["day"])), now: try XCTUnwrap(str(c["now"])), utcOffsetMinutes: try XCTUnwrap(c["utcOffsetMinutes"] as? Int)),
                c["expected"] as? String
            )
        }
    }

    func testRunnerChip() throws {
        let chip = try XCTUnwrap(CiPage.runnerChip(try scenario()))
        XCTAssertEqual(["label": chip.label, "online": chip.online] as NSDictionary, try fixture()["runnerChip"] as? NSDictionary)
        var none = try scenario()
        none.runners = []
        XCTAssertNil(CiPage.runnerChip(none))
    }

    func testRepoOptions() throws {
        let got = CiPage.repoOptions(try scenario()).map { ["label": $0.label, "value": $0.value] }
        XCTAssertEqual(got as NSArray, try fixture()["repoOptions"] as? NSArray)
    }

    func testTabCounts() throws {
        let got = CiPage.tabCounts(try scenario())
        let json = Dictionary(uniqueKeysWithValues: got.map { ($0.key.rawValue, $0.value) })
        XCTAssertEqual(json as NSDictionary, try fixture()["tabCounts"] as? NSDictionary)
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
        empty.trains = []
        empty.trainEvents = []
        let list = try XCTUnwrap(try ciFixture("ci-page-view.json")["cases"] as? [[String: Any]])
        XCTAssertFalse(list.isEmpty)
        for c in list {
            let input = str(c["world"]) == "empty" ? empty : scenario
            // A `repositoryId` case is the repo page's Runs tab (#EXP-2214: with its `tab`).
            let tree: CardJSON
            if let repositoryId = str(c["repositoryId"]) {
                tree = CiPageView.ciRepoRunsTree(
                    input, repositoryId: repositoryId, tab: str(c["tab"]).flatMap(CiPage.Tab.init(rawValue:)) ?? .train,
                    phone: c["phone"] as? Bool == true
                )
            } else { tree = CiPageView.ciPageTree(input, state: CiPageView.State(
                tab: try XCTUnwrap(CiPage.Tab(rawValue: try XCTUnwrap(str(c["tab"])))),
                filter: try ciPageFilter(c["filter"]),
                phone: c["phone"] as? Bool == true
            )) }
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
        XCTAssertEqual(state.tab, .train, "the merge queue opens first")
        XCTAssertEqual(CiPageView.stateAfter(state, event: CiPageView.Events.tab, payload: ["value": .string("releases")])?.tab, .releases)
        XCTAssertEqual(CiPageView.stateAfter(state, event: CiPageView.Events.status, payload: ["value": .string("mine")])?.filter.status, .mine)
        let repo = CiPageView.stateAfter(state, event: CiPageView.Events.repository, payload: ["value": .string("repo-site")])
        XCTAssertEqual(repo?.filter.repositoryId, "repo-site")
        XCTAssertNil(CiPageView.stateAfter(repo!, event: CiPageView.Events.repository, payload: ["value": .string("all")])?.filter.repositoryId)
        XCTAssertNil(CiPageView.stateAfter(state, event: CiPageView.Events.openRun, payload: nil))
        XCTAssertNil(CiPageView.stateAfter(state, event: CiPageView.Events.queueMenu, payload: ["value": .string("hold")]))
        XCTAssertEqual(CiPageView.stateAfter(state, event: CiPageView.Events.runners, payload: nil)?.tab, .runners)
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
                updatedAt: "x", provider: "github", title: "Pricing page copy"
            ).save(db)
            try UserEntity(id: "u-me", name: "Mara Lind", email: "mara@example.com", image: nil, createdAt: "x", updatedAt: "x").save(db)
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
        XCTAssertEqual(input.runs.first?.title, "Pricing page copy")
        XCTAssertEqual(input.users, [CiPage.User(id: "u-me", name: "Mara Lind", image: nil)])
        XCTAssertEqual(input.jobs.first?.runsOn, ["linux"])
        XCTAssertEqual(input.runners.map(\.label), ["mint-id"])
        XCTAssertEqual(input.runners.first?.slots, 2)
        XCTAssertEqual(CiPage.ciBadge(input), NavBadges.Value(count: 1, tone: .attention))
    }
}

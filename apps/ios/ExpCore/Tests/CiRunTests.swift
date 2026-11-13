import Foundation
import XCTest
@testable import ExpCore

// #EXP-1617 / #EXP-1804: the CI run page and the runner section, replayed
// against packages/domain-contract/fixtures/ci-run.json and
// runner-settings.json (TS `ci-run.test.ts`, desktop `domain::ci_run`,
// Android `CiRunTest`); `CiRunViewTests` replays the genUI trees
// (fixtures/ci-run-view.json).

func ciFixture(_ name: String) throws -> [String: Any] {
    let url = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()          // ExpCore/Tests/
        .deletingLastPathComponent()          // ExpCore/
        .deletingLastPathComponent()          // apps/ios/
        .deletingLastPathComponent()          // apps/
        .deletingLastPathComponent()          // the repo root
        .appendingPathComponent("packages/domain-contract/fixtures/\(name)")
    let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
    return try XCTUnwrap(json as? [String: Any])
}

func ciString(_ value: Any?) -> String? { value as? String }

func ciDate(_ raw: Any?) throws -> Date {
    try XCTUnwrap(WireTimestamps.parse(try XCTUnwrap(raw as? String)))
}

func ciJob(_ raw: [String: Any]) throws -> CiRun.Job {
    CiRun.Job(
        id: ciString(raw["id"]) ?? "",
        check: ciString(raw["check"]) ?? "",
        position: (raw["position"] as? NSNumber)?.intValue ?? 0,
        status: try XCTUnwrap(ciString(raw["status"])),
        conclusion: ciString(raw["conclusion"]),
        reason: ciString(raw["reason"]),
        runnerLabel: ciString(raw["runnerLabel"]),
        startedAt: ciString(raw["startedAt"]),
        completedAt: ciString(raw["completedAt"]),
        steps: try ((raw["steps"] as? [[String: Any]]) ?? []).map(ciStep)
    )
}

func ciStep(_ raw: [String: Any]) throws -> CiRun.Step {
    CiRun.Step(
        name: try XCTUnwrap(ciString(raw["name"])),
        status: try XCTUnwrap(ciString(raw["status"])),
        conclusion: ciString(raw["conclusion"]),
        startedAt: (raw["startedAt"] as? NSNumber)?.doubleValue,
        completedAt: (raw["completedAt"] as? NSNumber)?.doubleValue
    )
}

func ciRunModel(_ raw: [String: Any]) throws -> CiRun.Run {
    CiRun.Run(
        id: try XCTUnwrap(ciString(raw["id"])),
        number: try XCTUnwrap(raw["number"] as? NSNumber).intValue,
        name: try XCTUnwrap(ciString(raw["name"])),
        state: try XCTUnwrap(ciString(raw["state"])),
        error: ciString(raw["error"]),
        sha: try XCTUnwrap(ciString(raw["sha"])),
        startedAt: ciString(raw["startedAt"]),
        completedAt: ciString(raw["completedAt"])
    )
}

final class CiRunTests: XCTestCase {
    private func cases(_ file: String, _ key: String) throws -> [[String: Any]] {
        let list = try XCTUnwrap(try ciFixture(file)[key] as? [[String: Any]])
        XCTAssertFalse(list.isEmpty, key)
        return list
    }

    private func optional(_ value: String?) -> Any { value ?? NSNull() }

    private func rowDict(_ row: CiRun.JobRow) -> NSDictionary {
        [
            "id": row.id, "title": row.title, "subtitle": optional(row.subtitle),
            "glyph": row.glyph.rawValue, "tone": row.tone.rawValue, "duration": optional(row.duration),
        ]
    }

    func testCopyAndConstants() throws {
        let fixture = try ciFixture("ci-run.json")
        let copy = try XCTUnwrap(fixture["copy"] as? [String: Any])
        XCTAssertEqual(copy["title"] as? String, CiRun.Copy.title)
        XCTAssertEqual(copy["jobs"] as? String, CiRun.Copy.jobs)
        XCTAssertEqual(copy["steps"] as? String, CiRun.Copy.steps)
        XCTAssertEqual(copy["log"] as? String, CiRun.Copy.log)
        XCTAssertEqual(copy["rerunFailed"] as? String, CiRun.Copy.rerunFailed)
        XCTAssertEqual(copy["cancel"] as? String, CiRun.Copy.cancel)
        XCTAssertEqual(copy["emptyLog"] as? String, CiRun.Copy.emptyLog)
        XCTAssertEqual(copy["failedJobs"] as? String, CiRun.Copy.failedJobs)
        XCTAssertEqual(copy["pipelineError"] as? String, CiRun.Copy.pipelineError)
        XCTAssertEqual(copy["none"] as? String, CiRun.Copy.none)
        XCTAssertEqual(copy["reasons"] as? [String: String], CiRun.Copy.reasons)
        let tiles = try XCTUnwrap(copy["tiles"] as? [String: String])
        XCTAssertEqual(tiles, ["status": CiRun.Copy.tileStatus, "duration": CiRun.Copy.tileDuration, "runner": CiRun.Copy.tileRunner, "commit": CiRun.Copy.tileCommit])
        let status = try XCTUnwrap(copy["status"] as? [String: String])
        for (key, value) in status { XCTAssertEqual(CiRun.statusLabel(try XCTUnwrap(CiRun.Status(rawValue: key))), value) }
        XCTAssertEqual(fixture["liveTailMs"] as? Int, CiRun.liveTailMs)
        let maxLines = try XCTUnwrap(fixture["logMaxLines"] as? [String: Int])
        XCTAssertEqual(maxLines, ["wide": CiRun.logMaxLinesWide, "phone": CiRun.logMaxLinesPhone])
        let ids = try XCTUnwrap(fixture["testIds"] as? [String: String])
        XCTAssertEqual(ids, [
            "page": CiRun.TestIds.page, "tiles": CiRun.TestIds.tiles, "jobRow": CiRun.TestIds.jobRow,
            "steps": CiRun.TestIds.steps, "log": CiRun.TestIds.log, "rerun": CiRun.TestIds.rerun, "cancel": CiRun.TestIds.cancel,
        ])
    }

    func testRunStatus() throws {
        for item in try cases("ci-run.json", "runStatus") {
            let jobs = try XCTUnwrap(item["jobs"] as? [[String: Any]]).map { ($0["status"] as! String, ciString($0["conclusion"])) }
            let (status, tone) = CiRun.runStatus(state: try XCTUnwrap(ciString(item["state"])), jobs: jobs)
            XCTAssertEqual(["status": status.rawValue, "tone": tone.rawValue] as NSDictionary, item["expected"] as? NSDictionary, "\(item["name"] ?? "")")
        }
    }

    func testTiles() throws {
        for item in try cases("ci-run.json", "tiles") {
            let run = try XCTUnwrap(item["run"] as? [String: Any])
            let jobs = try XCTUnwrap(item["jobs"] as? [[String: Any]]).map {
                ($0["status"] as! String, ciString($0["conclusion"]), ciString($0["runnerLabel"]))
            }
            let tiles = CiRun.tiles(
                state: try XCTUnwrap(ciString(run["state"])), sha: try XCTUnwrap(ciString(run["sha"])),
                startedAt: ciString(run["startedAt"]), completedAt: ciString(run["completedAt"]),
                jobs: jobs, now: try ciDate(item["now"])
            )
            let actual = tiles.map { ["label": $0.label, "value": $0.value, "tone": $0.tone.rawValue, "delta": $0.delta.map { $0 as Any } ?? NSNull()] as [String: Any] }
            XCTAssertEqual(actual as NSArray, item["expected"] as? NSArray, "\(item["name"] ?? "")")
        }
    }

    func testJobRows() throws {
        for item in try cases("ci-run.json", "jobRows") {
            let jobs = try XCTUnwrap(item["jobs"] as? [[String: Any]]).map(ciJob)
            let rows = CiRun.jobRows(jobs, now: try ciDate(item["now"]))
            XCTAssertEqual(rows.map(rowDict) as NSArray, item["expected"] as? NSArray, "\(item["name"] ?? "")")
        }
    }

    func testDefaultJob() throws {
        for item in try cases("ci-run.json", "defaultJob") {
            let jobs = try XCTUnwrap(item["jobs"] as? [[String: Any]]).map(ciJob)
            XCTAssertEqual(CiRun.defaultJob(jobs, requested: ciString(item["requested"])), ciString(item["expected"]), "\(item["name"] ?? "")")
        }
    }

    func testSteps() throws {
        for item in try cases("ci-run.json", "steps") {
            let steps = try XCTUnwrap(item["steps"] as? [[String: Any]]).map(ciStep)
            let actual = CiRun.steps(steps, now: try ciDate(item["now"])).map {
                ["title": $0.title, "status": $0.status.rawValue, "description": optional($0.description)]
            }
            XCTAssertEqual(actual as NSArray, item["expected"] as? NSArray, "\(item["name"] ?? "")")
        }
    }

    func testLogView() throws {
        for item in try cases("ci-run.json", "logView") {
            let view = CiRun.logView(try XCTUnwrap(ciString(item["text"])))
            let expected = try XCTUnwrap(item["expected"] as? [String: Any])
            XCTAssertEqual(view.code, expected["code"] as? String, "\(item["name"] ?? "")")
            XCTAssertEqual(view.highlight, expected["highlight"] as? [Int], "\(item["name"] ?? "")")
        }
    }

    func testActions() throws {
        for item in try cases("ci-run.json", "actions") {
            let actions = CiRun.actions(state: try XCTUnwrap(ciString(item["state"])), error: ciString(item["error"]))
            XCTAssertEqual(["rerunFailed": actions.rerunFailed, "cancel": actions.cancel] as NSDictionary, item["expected"] as? NSDictionary, "\(item["name"] ?? "")")
        }
    }

    func testSpecimenMatchesTheFixture() throws {
        let specimen = try XCTUnwrap(try ciFixture("ci-run.json")["specimen"] as? [String: Any])
        XCTAssertEqual(try ciRunModel(try XCTUnwrap(specimen["run"] as? [String: Any])), CiRun.specimenRun)
        XCTAssertEqual(try XCTUnwrap(specimen["jobs"] as? [[String: Any]]).map(ciJob), CiRun.specimenJobs)
        XCTAssertEqual(ciString(specimen["selected"]), CiRun.specimenSelected)
        XCTAssertEqual(ciString(specimen["log"]), CiRun.specimenLog)
        XCTAssertEqual(ciString(specimen["now"]), CiRun.specimenNow)
    }

    func testLogTailDropsLateChunks() {
        let tail = CiRun.LogTail(jobId: "j1")
        let first = tail.appending(jobId: "j1", readFrom: 0, text: "a\n", nextOffset: 2, complete: false)
        XCTAssertEqual(first.text, "a\n")
        XCTAssertEqual(first.appending(jobId: "j1", readFrom: 0, text: "a\n", nextOffset: 2, complete: false), first)
        XCTAssertEqual(first.appending(jobId: "j2", readFrom: 2, text: "x", nextOffset: 3, complete: true), first)
        let done = first.appending(jobId: "j1", readFrom: 2, text: "b", nextOffset: 3, complete: true)
        XCTAssertEqual(done.text, "a\nb")
        XCTAssertTrue(done.complete)
        XCTAssertTrue(done.loaded)
    }

    func testRunLinks() {
        XCTAssertEqual(CiRun.parsePath("/t/acme/ci/r12?job=j2#x")?.runId, "r12")
        XCTAssertEqual(CiRun.parsePath("/t/acme/ci/r12")?.team, "acme")
        XCTAssertNil(CiRun.parsePath("/t/acme/ci"))
        XCTAssertNil(CiRun.parsePath("/t/acme/issues/r12"))
        XCTAssertEqual(CiRun.jobParam("/t/acme/ci/r12?x=1&job=j2#h"), "j2")
        XCTAssertNil(CiRun.jobParam("/t/acme/ci/r12?job="))
        XCTAssertNil(CiRun.jobParam("/t/acme/ci/r12"))
        XCTAssertTrue(CiRun.isRunLink("https://app.example.com/t/acme/ci/r12?job=j2", origin: "https://app.example.com"))
        XCTAssertFalse(CiRun.isRunLink("https://github.com/t/acme/ci/r12", origin: "https://app.example.com"))
    }

    func testEntitiesDecodeTheWire() throws {
        let job = try JSONDecoder().decode(CiJobEntity.self, from: Data("""
            {"id":"j1","team_id":"t","run_id":"r1","key":"test","check":"test (linux)","position":"1",
             "runs_on":["linux"],"needs":[],"matrix":{},"timeout_min":"60","status":"completed","conclusion":"failure",
             "reason":null,"runner_device_id":"d1","log_bytes":"12",
             "steps":[{"name":"Checkout","status":"completed","conclusion":"success","startedAt":1000,"completedAt":5000}],
             "created_at":"x","updated_at":"x"}
            """.utf8))
        XCTAssertEqual(job.position, 1)
        XCTAssertEqual(job.parsedSteps, [CiRun.Step(name: "Checkout", status: "completed", conclusion: "success", startedAt: 1000, completedAt: 5000)])
        XCTAssertEqual(job.job(runnerLabel: "mint").runnerLabel, "mint")
        let run = try JSONDecoder().decode(CiRunEntity.self, from: Data("""
            {"id":"r1","team_id":"t","repository_id":"repo","number":"12","name":"ci","sha":"abc","trigger":"push",
             "state":"failure","created_at":"x","updated_at":"x"}
            """.utf8))
        XCTAssertEqual(run.run.number, 12)
        let device = try JSONDecoder().decode(DeviceEntity.self, from: Data("""
            {"id":"d1","user_id":"u","device_id":"dev","label":"mint","runner":{"enabled":true,"labels":["gpu"],"slots":2,"timeoutMin":60}}
            """.utf8))
        XCTAssertEqual(RunnerSettings.Settings(json: device.runner), RunnerSettings.specimenSettings)
    }

    // MARK: - runner-settings.json

    func testRunnerCopyAndConstants() throws {
        let fixture = try ciFixture("runner-settings.json")
        let copy = try XCTUnwrap(fixture["copy"] as? [String: Any])
        XCTAssertEqual(copy["section"] as? String, RunnerSettings.Copy.section)
        let enabled = try XCTUnwrap(copy["enabled"] as? [String: String])
        XCTAssertEqual(enabled, ["label": RunnerSettings.Copy.enabledLabel, "description": RunnerSettings.Copy.enabledDescription])
        let labels = try XCTUnwrap(copy["labels"] as? [String: String])
        XCTAssertEqual(labels, ["label": RunnerSettings.Copy.labelsLabel, "placeholder": RunnerSettings.Copy.labelsPlaceholder, "invalid": RunnerSettings.Copy.labelsInvalid])
        XCTAssertEqual(copy["slots"] as? String, RunnerSettings.Copy.slots)
        XCTAssertEqual(copy["timeout"] as? String, RunnerSettings.Copy.timeout)
        XCTAssertEqual(copy["recent"] as? String, RunnerSettings.Copy.recent)
        XCTAssertEqual(copy["noJobs"] as? String, RunnerSettings.Copy.noJobs)
        XCTAssertEqual(copy["runSubtitle"] as? String, RunnerSettings.Copy.runSubtitle)
        let notice = try XCTUnwrap(copy["notice"] as? [String: [String: String]])
        XCTAssertEqual(Set(notice.keys), Set(RunnerSettings.notices.keys))
        for (key, value) in notice {
            let n = try XCTUnwrap(RunnerSettings.notices[key])
            XCTAssertEqual(value, ["type": n.type, "title": n.title, "message": n.message])
        }
        let defaults = try XCTUnwrap(fixture["defaults"] as? [String: Any])
        XCTAssertEqual(RunnerSettings.Settings(json: String(data: try JSONSerialization.data(withJSONObject: defaults), encoding: .utf8)), .defaults)
        XCTAssertEqual(RunnerSettings.Settings(json: nil), .defaults)
        let limits = try XCTUnwrap(fixture["limits"] as? [String: Any])
        XCTAssertEqual(limits["labels"] as? Int, RunnerSettings.maxLabels)
        XCTAssertEqual(limits["recent"] as? Int, RunnerSettings.maxRecent)
        XCTAssertEqual(limits["slots"] as? [String: Int], ["min": RunnerSettings.minSlots, "max": RunnerSettings.maxSlots])
        let options = try XCTUnwrap(fixture["timeoutOptions"] as? [[String: String]])
        XCTAssertEqual(options, RunnerSettings.timeoutOptions.map { ["value": $0.value, "label": $0.label] })
        let ids = try XCTUnwrap(fixture["testIds"] as? [String: String])
        XCTAssertEqual(ids, [
            "section": RunnerSettings.TestIds.section, "switch": RunnerSettings.TestIds.toggle, "labels": RunnerSettings.TestIds.labels,
            "slots": RunnerSettings.TestIds.slots, "timeout": RunnerSettings.TestIds.timeout, "recent": RunnerSettings.TestIds.recent,
        ])
    }

    func testNormalizeLabel() throws {
        for item in try cases("runner-settings.json", "normalizeLabel") {
            XCTAssertEqual(RunnerSettings.normalizeLabel(try XCTUnwrap(ciString(item["input"]))), ciString(item["expected"]), "\(item["input"] ?? "")")
        }
    }

    func testNotice() throws {
        for item in try cases("runner-settings.json", "notice") {
            let caps = try XCTUnwrap(item["caps"] as? [String])
            XCTAssertEqual(RunnerSettings.notice(client: try XCTUnwrap(ciString(item["client"])), caps: caps), ciString(item["expected"]), "\(item["name"] ?? "")")
        }
    }

    func testOsLabel() throws {
        for item in try cases("runner-settings.json", "osLabel") {
            XCTAssertEqual(RunnerSettings.osLabel(ciString(item["platform"])), ciString(item["expected"]))
        }
    }

    private func runnerJob(_ raw: [String: Any]) throws -> RunnerSettings.Job {
        RunnerSettings.Job(
            id: try XCTUnwrap(ciString(raw["id"])), runnerDeviceId: ciString(raw["runnerDeviceId"]),
            check: try XCTUnwrap(ciString(raw["check"])), runNumber: try XCTUnwrap(raw["runNumber"] as? Int),
            repo: try XCTUnwrap(ciString(raw["repo"])), status: try XCTUnwrap(ciString(raw["status"])),
            conclusion: ciString(raw["conclusion"]), startedAt: ciString(raw["startedAt"]), completedAt: ciString(raw["completedAt"])
        )
    }

    func testRecentJobs() throws {
        let items = try cases("runner-settings.json", "recentJobs")
        for item in items {
            let jobs = try XCTUnwrap(item["jobs"] as? [[String: Any]]).map(runnerJob)
            let rows = RunnerSettings.recentJobs(deviceId: try XCTUnwrap(ciString(item["deviceId"])), jobs: jobs, now: try ciDate(item["now"]))
            XCTAssertEqual(rows.map(rowDict) as NSArray, item["expected"] as? NSArray, "\(item["name"] ?? "")")
        }
        // The specimen's jobs = recentJobs[0].
        let first = try XCTUnwrap(items.first)
        XCTAssertEqual(try XCTUnwrap(first["jobs"] as? [[String: Any]]).map(runnerJob), RunnerSettings.specimenJobs)
        XCTAssertEqual(ciString(first["now"]), RunnerSettings.specimenNow)
        XCTAssertEqual(ciString(first["deviceId"]), RunnerSettings.specimenDeviceId)
    }

    func testRunnerSpecimenMatchesTheFixture() throws {
        let specimen = try XCTUnwrap(try ciFixture("runner-settings.json")["specimen"] as? [String: Any])
        XCTAssertEqual(ciString(specimen["client"]), RunnerSettings.specimenClient)
        let device = try XCTUnwrap(specimen["device"] as? [String: Any])
        XCTAssertEqual(ciString(device["id"]), RunnerSettings.specimenDeviceId)
        XCTAssertEqual(ciString(device["platform"]), RunnerSettings.specimenPlatform)
        XCTAssertEqual(device["caps"] as? [String], RunnerSettings.specimenCaps)
        let runner = try JSONSerialization.data(withJSONObject: try XCTUnwrap(device["runner"]))
        XCTAssertEqual(RunnerSettings.Settings(json: String(data: runner, encoding: .utf8)), RunnerSettings.specimenSettings)
    }
}

// #EXP-1804: `CiRunView` builds the SAME genUI trees as web
// `apps/web/src/lib/ci-run-view.ts`: fixtures/ci-run-view.json holds them,
// replayed node for node (ids, props, slots, events, child order).
final class CiRunViewTests: XCTestCase {
    private func cases(_ key: String) throws -> [[String: Any]] {
        let list = try XCTUnwrap(try ciFixture("ci-run-view.json")[key] as? [[String: Any]])
        XCTAssertFalse(list.isEmpty, key)
        return list
    }

    private func settings(_ raw: Any?) throws -> RunnerSettings.Settings {
        let data = try JSONSerialization.data(withJSONObject: try XCTUnwrap(raw as? [String: Any]))
        return RunnerSettings.Settings(json: String(data: data, encoding: .utf8))
    }

    /// The first differing path, for a readable failure.
    private func diff(_ a: CardJSON, _ b: CardJSON, _ path: String = "") -> String? {
        switch (a, b) {
        case let (.object(x), .object(y)):
            for key in Set(x.keys).union(y.keys).sorted() {
                guard let l = x[key] else { return "\(path)/\(key): missing" }
                guard let r = y[key] else { return "\(path)/\(key): unexpected" }
                if let d = diff(l, r, "\(path)/\(key)") { return d }
            }
            return nil
        case let (.array(x), .array(y)):
            if x.count != y.count { return "\(path): \(x.count) vs \(y.count) items" }
            for (i, pair) in zip(x, y).enumerated() { if let d = diff(pair.0, pair.1, "\(path)/\(i)") { return d } }
            return nil
        default:
            return a == b ? nil : "\(path): \(a.compactString) vs \(b.compactString)"
        }
    }

    func testRunPageTreesAreWebs() throws {
        for item in try cases("ciRun") {
            let model = try XCTUnwrap(item["model"] as? [String: Any])
            let tree = CiRunView.ciRunTree(
                run: try ciRunModel(try XCTUnwrap(model["run"] as? [String: Any])),
                jobs: try XCTUnwrap(model["jobs"] as? [[String: Any]]).map(ciJob),
                selected: ciString(item["selected"]),
                log: ciString(item["log"]),
                now: try ciDate(item["now"]),
                phone: try XCTUnwrap(item["phone"] as? Bool),
                busy: ciString(item["busy"]).flatMap(CiRunView.Busy.init(rawValue:))
            )
            let expected = try XCTUnwrap(CardJSON(any: item["tree"]))
            XCTAssertNil(diff(expected, tree), item["name"] as? String ?? "")
        }
    }

    func testRunnerSectionTreesAreWebs() throws {
        let sample = RunnerSettings.recentJobs(
            deviceId: RunnerSettings.specimenDeviceId, jobs: RunnerSettings.specimenJobs,
            now: try ciDate(RunnerSettings.specimenNow)
        )
        for item in try cases("runner") {
            let tree = CiRunView.runnerSectionTree(
                client: try XCTUnwrap(ciString(item["client"])),
                caps: try XCTUnwrap(item["caps"] as? [String]),
                platform: ciString(item["platform"]),
                settings: try settings(item["settings"]),
                owner: try XCTUnwrap(item["owner"] as? Bool),
                refused: try XCTUnwrap(item["refused"] as? Bool),
                recent: try XCTUnwrap(item["recent"] as? Bool) ? sample : []
            )
            let expected = try XCTUnwrap(CardJSON(any: item["tree"]))
            XCTAssertNil(diff(expected, tree), item["name"] as? String ?? "")
        }
    }

    func testRunnerChangeEventsWriteWhatWebWrites() throws {
        for item in try cases("after") {
            let payload = try XCTUnwrap(CardJSON(any: item["payload"])).objectValue
            let (next, refused) = CiRunView.runnerSettingsAfter(
                try settings(item["current"]), event: try XCTUnwrap(ciString(item["event"])), payload: payload
            )
            let name = "\(item["event"] ?? "") \(item["payload"] ?? "")"
            let expected = item["settings"] is NSNull || item["settings"] == nil ? nil : try settings(item["settings"])
            XCTAssertEqual(next, expected, name)
            XCTAssertEqual(refused, item["refused"] as? Bool, name)
        }
    }
}

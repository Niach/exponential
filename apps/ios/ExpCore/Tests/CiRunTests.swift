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
        completedAt: ciString(raw["completedAt"]),
        ref: ciString(raw["ref"]),
        provider: ciString(raw["provider"]) ?? "exponential",
        externalUrl: ciString(raw["externalUrl"]),
        externalAccess: ciString(raw["externalAccess"]),
        title: ciString(raw["title"]),
        createdAt: ciString(raw["createdAt"])
    )
}

func ciPullRequest(_ raw: Any?) throws -> CiRun.PullRequest? {
    guard let pr = raw as? [String: Any] else { return nil }
    return CiRun.PullRequest(number: try XCTUnwrap(pr["number"] as? Int), base: try XCTUnwrap(ciString(pr["base"])))
}

final class CiRunTests: XCTestCase {
    private func cases(_ file: String, _ key: String) throws -> [[String: Any]] {
        let list = try XCTUnwrap(try ciFixture(file)[key] as? [[String: Any]])
        XCTAssertFalse(list.isEmpty, key)
        return list
    }

    private func optional(_ value: String?) -> Any { value ?? NSNull() }

    private func rowDict(_ row: CiRun.JobRow) -> NSDictionary {
        var out: [String: Any] = [
            "id": row.id, "title": row.title, "subtitle": optional(row.subtitle),
            "glyph": row.glyph.rawValue, "tone": row.tone.rawValue, "duration": optional(row.duration),
        ]
        if let status = row.status { out["status"] = status.rawValue }
        if let statusTone = row.statusTone { out["statusTone"] = statusTone }
        return out as NSDictionary
    }

    func testCopyAndConstants() throws {
        let fixture = try ciFixture("ci-run.json")
        let copy = try XCTUnwrap(fixture["copy"] as? NSDictionary)
        let c = CiRun.Copy.self
        let statuses: [CiRun.Status] = [.queued, .running, .passed, .failed, .cancelled]
        let ours: [String: Any] = [
            "title": c.title, "jobs": c.jobs, "steps": c.steps, "log": c.log,
            "rerunFailed": c.rerunFailed, "cancel": c.cancel, "rerunAll": c.rerunAll, "fix": c.fix,
            "openExternal": c.openExternal, "breadcrumbRoot": c.breadcrumbRoot, "pullRequest": c.pullRequest,
            "sources": c.sources,
            "fixPrompt": ["head": c.fixPromptHead, "job": c.fixPromptJob, "read": c.fixPromptRead, "tail": c.fixPromptTail],
            "emptyLog": c.emptyLog, "pipelineError": c.pipelineError, "none": c.none,
            "status": Dictionary(uniqueKeysWithValues: statuses.map { ($0.rawValue, CiRun.statusLabel($0)) }),
            "reasons": c.reasons, "more": c.more,
            "jobsCaption": Dictionary(uniqueKeysWithValues: c.jobsCaption.map { ($0.key.rawValue, $0.value) }),
            "jobSummary": [
                "failed": c.jobSummaryFailed, "running": c.jobSummaryRunning, "passed": c.jobSummaryPassed,
                "queued": c.jobSummaryQueued, "skipped": c.jobSummarySkipped, "on": c.jobSummaryOn,
            ],
            "failure": ["title": c.failureTitle],
        ]
        XCTAssertEqual(ours as NSDictionary, copy)
        XCTAssertEqual(fixture["fixLogLines"] as? Int, CiRun.fixLogLines)
        XCTAssertEqual(fixture["liveTailMs"] as? Int, CiRun.liveTailMs)
        let maxLines = try XCTUnwrap(fixture["logMaxLines"] as? [String: Int])
        XCTAssertEqual(maxLines, ["wide": CiRun.logMaxLinesWide, "phone": CiRun.logMaxLinesPhone])
        let ids = try XCTUnwrap(fixture["testIds"] as? [String: String])
        let t = CiRun.TestIds.self
        XCTAssertEqual(ids, [
            "page": t.page, "jobRow": t.jobRow, "steps": t.steps, "log": t.log, "rerun": t.rerun, "cancel": t.cancel,
            "rerunAll": t.rerunAll, "fix": t.fix, "openExternal": t.openExternal, "breadcrumb": t.breadcrumb,
            "source": t.source, "chips": t.chips, "more": t.more, "failure": t.failure, "jobCard": t.jobCard,
        ])
    }

    func testRunStatus() throws {
        for item in try cases("ci-run.json", "runStatus") {
            let jobs = try XCTUnwrap(item["jobs"] as? [[String: Any]]).map { ($0["status"] as! String, ciString($0["conclusion"])) }
            let (status, tone) = CiRun.runStatus(state: try XCTUnwrap(ciString(item["state"])), jobs: jobs)
            XCTAssertEqual(["status": status.rawValue, "tone": tone] as NSDictionary, item["expected"] as? NSDictionary, "\(item["name"] ?? "")")
        }
    }

    func testJobsCaption() throws {
        for item in try cases("ci-run.json", "jobsCaption") {
            let jobs = try XCTUnwrap(item["jobs"] as? [[String: Any]]).map(ciJob)
            XCTAssertEqual(CiRun.jobsCaption(jobs), ciString(item["expected"]), "\(item["name"] ?? "")")
        }
    }

    func testJobSummary() throws {
        for item in try cases("ci-run.json", "jobSummary") {
            let job = try ciJob(try XCTUnwrap(item["job"] as? [String: Any]))
            XCTAssertEqual(CiRun.jobSummary(job, now: try ciDate(item["now"])), ciString(item["expected"]), "\(item["name"] ?? "")")
        }
    }

    func testChips() throws {
        for item in try cases("ci-run.json", "chips") {
            let chips = CiRun.chips(
                run: try ciRunModel(try XCTUnwrap(item["run"] as? [String: Any])),
                jobs: try XCTUnwrap(item["jobs"] as? [[String: Any]]).map(ciJob),
                pullRequest: try ciPullRequest(item["pullRequest"]),
                author: ciString(item["author"]),
                now: try ciDate(item["now"])
            )
            let actual = chips.map { ["id": $0.id, "label": $0.label, "icon": optional($0.icon), "dot": optional($0.dot)] as NSDictionary }
            XCTAssertEqual(actual as NSArray, item["expected"] as? NSArray, "\(item["name"] ?? "")")
        }
    }

    func testFailureSummary() throws {
        for item in try cases("ci-run.json", "failureSummary") {
            let job = item["job"] as? [String: Any]
            let summary = CiRun.failureSummary(check: ciString(job?["check"]), log: ciString(item["log"]))
            let actual: Any = summary.map { ["title": $0.title, "message": $0.message] as NSDictionary } ?? NSNull()
            XCTAssertEqual(actual as? NSObject, item["expected"] as? NSObject, "\(item["name"] ?? "")")
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
            let actions = CiRun.actions(
                state: try XCTUnwrap(ciString(item["state"])), error: ciString(item["error"]),
                provider: ciString(item["provider"]), externalAccess: ciString(item["externalAccess"]),
                externalUrl: ciString(item["externalUrl"])
            )
            XCTAssertEqual([
                "primary": optional(actions.primary?.rawValue), "fix": actions.fix, "menu": actions.menu.map(\.rawValue),
            ] as NSDictionary, item["expected"] as? NSDictionary, "\(item["name"] ?? "")")
        }
    }

    func testFixPrompt() throws {
        for item in try cases("ci-run.json", "fixPrompt") {
            let input = try XCTUnwrap(item["input"] as? [String: Any])
            let run = try XCTUnwrap(input["run"] as? [String: Any])
            let prompt = CiRun.fixPrompt(
                run: CiRun.Run(
                    id: try XCTUnwrap(ciString(run["id"])), number: try XCTUnwrap(run["number"] as? Int),
                    name: try XCTUnwrap(ciString(run["name"])), state: "failure", error: nil,
                    sha: try XCTUnwrap(ciString(run["sha"])), startedAt: nil, completedAt: nil, ref: ciString(run["ref"])
                ),
                repository: try XCTUnwrap(ciString(input["repository"])),
                job: ciString(input["job"]),
                log: ciString(input["log"])
            )
            XCTAssertEqual(prompt, ciString(item["expected"]), "\(item["name"] ?? "")")
        }
    }

    func testSpecimenMatchesTheFixture() throws {
        let specimen = try XCTUnwrap(try ciFixture("ci-run.json")["specimen"] as? [String: Any])
        XCTAssertEqual(try ciRunModel(try XCTUnwrap(specimen["run"] as? [String: Any])), CiRun.specimenRun)
        XCTAssertEqual(ciString(specimen["repository"]), CiRun.specimenRepository)
        XCTAssertEqual(try ciPullRequest(specimen["pullRequest"]), CiRun.specimenPullRequest)
        XCTAssertEqual(ciString(specimen["author"]), CiRun.specimenAuthor)
        XCTAssertEqual(try XCTUnwrap(specimen["jobs"] as? [[String: Any]]).map(ciJob), CiRun.specimenJobs)
        XCTAssertEqual(ciString(specimen["selected"]), CiRun.specimenSelected)
        XCTAssertEqual(ciString(specimen["log"]), CiRun.specimenLog)
        XCTAssertEqual(ciString(specimen["now"]), CiRun.specimenNow)
        // The specimen selects its failing job; its log holds one error line.
        XCTAssertEqual(CiRun.defaultJob(CiRun.specimenJobs, requested: nil), CiRun.specimenSelected)
        XCTAssertEqual(CiRun.logView(CiRun.specimenLog).highlight.count, 1)
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
            {"id":"d1","user_id":"u","device_id":"dev","label":"mint","runner":{"enabled":true,"labels":["gpu"],"slots":2,"timeoutMin":60,"memoryMaxMb":8192,"pauseWhenBusy":true}}
            """.utf8))
        XCTAssertEqual(RunnerSettings.Settings(json: device.runner), RunnerSettings.specimenSettings)
        // An older row without the #EXP-2193 fields takes the defaults.
        let old = RunnerSettings.Settings(json: #"{"enabled":true,"labels":[],"slots":1,"timeoutMin":30}"#)
        XCTAssertEqual(old.memoryMaxMb, 8192)
        XCTAssertTrue(old.pauseWhenBusy)
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
        XCTAssertEqual(copy["memory"] as? String, RunnerSettings.Copy.memory)
        XCTAssertEqual(copy["pause"] as? String, RunnerSettings.Copy.pause)
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
        XCTAssertEqual(limits["memoryMaxMb"] as? [String: Int], [
            "min": RunnerSettings.minMemoryMb, "max": RunnerSettings.maxMemoryMb, "step": RunnerSettings.memoryStepMb,
        ])
        let options = try XCTUnwrap(fixture["timeoutOptions"] as? [[String: String]])
        XCTAssertEqual(options, RunnerSettings.timeoutOptions.map { ["value": $0.value, "label": $0.label] })
        let memory = try XCTUnwrap(fixture["memoryOptions"] as? [[String: String]])
        XCTAssertEqual(memory, RunnerSettings.memoryOptions.map { ["value": $0.value, "label": $0.label] })
        let ids = try XCTUnwrap(fixture["testIds"] as? [String: String])
        XCTAssertEqual(ids, [
            "section": RunnerSettings.TestIds.section, "switch": RunnerSettings.TestIds.toggle, "labels": RunnerSettings.TestIds.labels,
            "slots": RunnerSettings.TestIds.slots, "timeout": RunnerSettings.TestIds.timeout, "recent": RunnerSettings.TestIds.recent,
            "memory": RunnerSettings.TestIds.memory, "pause": RunnerSettings.TestIds.pause,
        ])
    }

    func testNormalizeLabel() throws {
        for item in try cases("runner-settings.json", "normalizeLabel") {
            XCTAssertEqual(RunnerSettings.normalizeLabel(try XCTUnwrap(ciString(item["input"]))), ciString(item["expected"]), "\(item["input"] ?? "")")
        }
    }

    func testUnsupported() throws {
        for item in try cases("runner-settings.json", "unsupported") {
            XCTAssertEqual(
                RunnerSettings.unsupported(kind: ciString(item["kind"]), platform: ciString(item["platform"])),
                try XCTUnwrap(item["expected"] as? Bool), "\(item["name"] ?? "")"
            )
        }
    }

    func testNotice() throws {
        for item in try cases("runner-settings.json", "notice") {
            let caps = try XCTUnwrap(item["caps"] as? [String])
            XCTAssertEqual(
                RunnerSettings.notice(
                    client: try XCTUnwrap(ciString(item["client"])), caps: caps,
                    kind: ciString(item["kind"]), platform: ciString(item["platform"])
                ),
                ciString(item["expected"]), "\(item["name"] ?? "")"
            )
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
        XCTAssertEqual(ciString(device["kind"]), RunnerSettings.specimenKind)
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

    private func model(_ raw: [String: Any]) throws -> CiRunView.Model {
        CiRunView.Model(
            run: try ciRunModel(try XCTUnwrap(raw["run"] as? [String: Any])),
            repository: ciString(raw["repository"]),
            pullRequest: try ciPullRequest(raw["pullRequest"]),
            author: ciString(raw["author"]),
            jobs: try XCTUnwrap(raw["jobs"] as? [[String: Any]]).map(ciJob),
            context: try (raw["context"] as? [String: Any]).map(runContext)
        )
    }

    /// #EXP-2214: the model's `context` (ci-page.json `runContext`).
    private func runContext(_ raw: [String: Any]) throws -> CiPage.RunContext {
        CiPage.RunContext(
            kind: try XCTUnwrap(CiPage.RunKind(rawValue: try XCTUnwrap(ciString(raw["kind"])))),
            badge: try XCTUnwrap(ciString(raw["badge"])),
            title: ciString(raw["title"]),
            caption: ciString(raw["caption"]),
            outcome: try ((raw["outcome"] as? [[String: Any]]) ?? []).map { o in
                CiPage.RunOutcome(
                    pullRequestId: try XCTUnwrap(ciString(o["pullRequestId"])), identifier: try XCTUnwrap(ciString(o["identifier"])),
                    title: try XCTUnwrap(ciString(o["title"])), text: try XCTUnwrap(ciString(o["text"])),
                    glyph: try XCTUnwrap(ciString(o["glyph"])), tone: try XCTUnwrap(ciString(o["tone"])), jobId: ciString(o["jobId"])
                )
            }
        )
    }

    func testRunPageTreesAreWebs() throws {
        for item in try cases("ciRun") {
            let tree = CiRunView.ciRunTree(
                try model(try XCTUnwrap(item["model"] as? [String: Any])),
                selected: ciString(item["selected"]),
                log: ciString(item["log"]),
                now: try ciDate(item["now"]),
                phone: try XCTUnwrap(item["phone"] as? Bool),
                busy: ciString(item["busy"]).flatMap(CiRunView.Busy.init(rawValue:)),
                jobOpen: item["jobOpen"] as? Bool ?? false
            )
            let expected = try XCTUnwrap(CardJSON(any: item["tree"]))
            XCTAssertNil(diff(expected, tree), item["name"] as? String ?? "")
        }
    }

    func testQueueSpecimenIsTheFixtureCase() throws {
        let item = try XCTUnwrap(try cases("ciRun").first { ($0["name"] as? String)?.hasPrefix("#EXP-2214 a merge queue run") == true })
        XCTAssertEqual(CiRunView.queueSpecimenModel, try model(try XCTUnwrap(item["model"] as? [String: Any])))
        XCTAssertEqual(CiRunView.queueSpecimenSelected, ciString(item["selected"]))
        XCTAssertEqual(CiRunView.queueSpecimenLog, ciString(item["log"]))
        XCTAssertEqual(CiRunView.queueSpecimenNow, ciString(item["now"]))
        XCTAssertEqual(CiRunView.ciRunTitle(CiRunView.queueSpecimenModel), "Testing #210")
    }

    func testHeaderAndTitle() throws {
        let item = try XCTUnwrap(try cases("ciRun").first)
        let m = try model(try XCTUnwrap(item["model"] as? [String: Any]))
        let header = CiRunView.ciRunHeader(m)
        XCTAssertEqual(header.title, "ci #12")
        XCTAssertEqual(header.subtitle, "acme/engine")
        XCTAssertEqual(CiRunView.ciRunTitle(m), "Mirror prunes deleted branches")
        XCTAssertEqual(CiRunView.specimenModel.run, CiRun.specimenRun)
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
                kind: ciString(item["kind"]),
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

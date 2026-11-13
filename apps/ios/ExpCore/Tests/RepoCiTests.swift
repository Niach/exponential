import Foundation
import XCTest
@testable import ExpCore

// #EXP-1617 E4 part 4: the repo page's Runs tab + CI settings, replayed by
// path against packages/domain-contract/fixtures/repo-ci.json (TS
// `repo-ci.test.ts`, desktop `domain::repo_ci`, Android `RepoCiTest`): EVERY
// runRows, pipelineRows, requiredSuggestions, secretName and switches case,
// comparing the WHOLE result, plus the copy, test ids and limits; then the
// genUI trees (`RepoCiView`, web `repo-ci-view.ts`).
final class RepoCiTests: XCTestCase {
    private func fixture() throws -> [String: Any] { try ciFixture("repo-ci.json") }

    private func cases(_ key: String) throws -> [[String: Any]] {
        try XCTUnwrap(try fixture()[key] as? [[String: Any]], key)
    }

    private func run(_ raw: [String: Any]) throws -> RepoCi.Run {
        RepoCi.Run(
            id: try XCTUnwrap(raw["id"] as? String),
            number: try XCTUnwrap(raw["number"] as? Int),
            name: try XCTUnwrap(raw["name"] as? String),
            sha: raw["sha"] as? String ?? "",
            ref: raw["ref"] as? String,
            trigger: raw["trigger"] as? String ?? "",
            state: raw["state"] as? String ?? "pending",
            startedAt: raw["startedAt"] as? String,
            completedAt: raw["completedAt"] as? String
        )
    }

    private func job(_ raw: [String: Any]) throws -> RepoCi.Job {
        RepoCi.Job(
            runId: try XCTUnwrap(raw["runId"] as? String),
            status: raw["status"] as? String ?? "",
            conclusion: raw["conclusion"] as? String,
            check: raw["check"] as? String ?? ""
        )
    }

    private func runs(_ item: [String: Any]) throws -> [RepoCi.Run] {
        try ((item["runs"] as? [[String: Any]]) ?? []).map(run)
    }

    private func jobs(_ item: [String: Any]) throws -> [RepoCi.Job] {
        try ((item["jobs"] as? [[String: Any]]) ?? []).map(job)
    }

    private func dict(_ row: RepoCi.RunRow) -> NSDictionary {
        [
            "id": row.id,
            "identifier": row.identifier,
            "title": row.title,
            "status": row.status.rawValue,
            "statusLabel": row.statusLabel,
            "glyph": row.glyph.rawValue,
            "tone": row.tone.rawValue,
            "commit": row.commit,
            "ref": row.ref,
            "trigger": row.trigger,
            "subtitle": row.subtitle,
            "duration": row.duration as Any? ?? NSNull(),
            "startedAt": row.startedAt as Any? ?? NSNull(),
        ]
    }

    func testCopyTestIdsAndLimits() throws {
        let fixture = try fixture()
        let copy = try XCTUnwrap(fixture["copy"] as? [String: Any])
        let runs = try XCTUnwrap(copy["runs"] as? [String: Any])
        XCTAssertEqual(runs["noRef"] as? String, RepoCi.Copy.Runs.noRef)
        XCTAssertEqual(runs["columns"] as? [String: String], [
            "status": RepoCi.Copy.Runs.colStatus, "run": RepoCi.Copy.Runs.colRun, "commit": RepoCi.Copy.Runs.colCommit,
            "ref": RepoCi.Copy.Runs.colRef, "trigger": RepoCi.Copy.Runs.colTrigger,
            "duration": RepoCi.Copy.Runs.colDuration, "started": RepoCi.Copy.Runs.colStarted,
        ])
        XCTAssertEqual(copy["triggers"] as? [String: String], RepoCi.Copy.triggers)
        let status = try XCTUnwrap(copy["status"] as? [String: String])
        for s in [CiRun.Status.queued, .running, .passed, .failed, .cancelled] {
            XCTAssertEqual(status[s.rawValue], CiRun.statusLabel(s))
        }
        let ci = try XCTUnwrap(copy["ci"] as? [String: Any])
        XCTAssertEqual(ci["section"] as? String, RepoCi.Copy.Ci.section)
        XCTAssertEqual(ci["noPipeline"] as? String, RepoCi.Copy.Ci.noPipeline)
        XCTAssertEqual(ci["required"] as? [String: String], [
            "label": RepoCi.Copy.Ci.requiredLabel,
            "description": RepoCi.Copy.Ci.requiredDescription,
            "placeholder": RepoCi.Copy.Ci.requiredPlaceholder,
        ])
        let secrets = try XCTUnwrap(ci["secrets"] as? [String: Any])
        let s = RepoCi.Copy.Secrets.self
        XCTAssertEqual(secrets["columns"] as? [String: String], ["name": s.colName, "updated": s.colUpdated])
        let plain: [String: String] = [
            "title": s.title, "add": s.add, "name": s.name, "value": s.value, "save": s.save, "cancel": s.cancel,
            "delete": s.delete, "deleteTitle": s.deleteTitle, "deleteConfirm": s.deleteConfirm, "empty": s.empty,
            "invalid": s.invalid,
        ]
        XCTAssertEqual(secrets.filter { $0.key != "columns" } as? [String: String], plain)
        for key in RepoCi.Switch.allCases {
            let raw = try XCTUnwrap(ci[key.rawValue] as? [String: String])
            XCTAssertEqual(raw["label"], RepoCi.Copy.switch(key).label)
            XCTAssertEqual(raw["description"], RepoCi.Copy.switch(key).description)
        }
        XCTAssertEqual(fixture["testIds"] as? [String: String], [
            "runs": RepoCi.TestIds.runs, "runRow": RepoCi.TestIds.runRow, "ci": RepoCi.TestIds.ci,
            "pipelines": RepoCi.TestIds.pipelines, "required": RepoCi.TestIds.required,
            "secrets": RepoCi.TestIds.secrets, "addSecret": RepoCi.TestIds.addSecret,
            "githubStatuses": RepoCi.TestIds.githubStatuses, "actionsRunners": RepoCi.TestIds.actionsRunners,
        ])
        XCTAssertEqual(fixture["limits"] as? [String: Int], ["runs": RepoCi.maxRuns])
    }

    func testRunRows() throws {
        let all = try cases("runRows")
        XCTAssertFalse(all.isEmpty)
        for item in all {
            let name = item["name"] as? String ?? ""
            let rows = RepoCi.runRows(try runs(item), jobs: try jobs(item), now: try ciDate(item["now"]))
            let expected = try XCTUnwrap(item["expected"] as? [[String: Any]])
            XCTAssertEqual(rows.map(dict), expected.map { $0 as NSDictionary }, name)
        }
    }

    func testRunRowsCapAtTheLimit() {
        let many = (1 ... RepoCi.maxRuns + 5).map { RepoCi.Run(id: "r\($0)", number: $0, name: "ci") }
        let rows = RepoCi.runRows(many, jobs: [], now: Date())
        XCTAssertEqual(rows.count, RepoCi.maxRuns)
        XCTAssertEqual(rows.first?.identifier, "#\(RepoCi.maxRuns + 5)")
    }

    func testPipelineRows() throws {
        for item in try cases("pipelineRows") {
            let rows = RepoCi.pipelineRows(try runs(item))
            let expected = try XCTUnwrap(item["expected"] as? [[String: String]])
            XCTAssertEqual(rows.map { ["name": $0.name, "runId": $0.runId, "state": $0.state] }, expected, item["name"] as? String ?? "")
        }
    }

    func testRequiredSuggestions() throws {
        for item in try cases("requiredSuggestions") {
            let got = RepoCi.requiredSuggestions(
                try runs(item), jobs: try jobs(item), required: item["required"] as? [String] ?? []
            )
            XCTAssertEqual(got, item["expected"] as? [String], item["name"] as? String ?? "")
        }
    }

    func testSecretName() throws {
        let all = try cases("secretName")
        XCTAssertFalse(all.isEmpty)
        for item in all {
            let input = try XCTUnwrap(item["input"] as? String)
            XCTAssertEqual(RepoCi.secretName(input), item["valid"] as? Bool, input)
        }
    }

    func testSwitches() throws {
        for item in try cases("switches") {
            let provider = try XCTUnwrap(item["provider"] as? String)
            XCTAssertEqual(RepoCi.switches(provider: provider).map(\.rawValue), item["expected"] as? [String], provider)
        }
    }

    /// The specimen = runRows[0]'s runs + jobs (with checks added).
    func testSpecimenIsTheFixture() throws {
        let item = try XCTUnwrap(try cases("runRows").first)
        XCTAssertEqual(RepoCi.specimenRuns, try runs(item))
        XCTAssertEqual(RepoCi.specimenJobs.map { RepoCi.Job(runId: $0.runId, status: $0.status, conclusion: $0.conclusion) }, try jobs(item))
        XCTAssertEqual(RepoCi.specimenNow, item["now"] as? String)
    }

    // MARK: - The trees (web `repo-ci-view.ts`, phone layout)

    private func object(_ json: CardJSON?) -> [String: CardJSON] { json?.objectValue ?? [:] }

    private func children(_ json: CardJSON?) -> [CardJSON] {
        if case let .array(items)? = object(json)["children"] { return items }
        return []
    }

    private func id(_ json: CardJSON?) -> String? {
        if case let .string(value)? = object(json)["id"] { return value }
        return nil
    }

    private func component(_ json: CardJSON?) -> String? {
        if case let .string(value)? = object(json)["component"] { return value }
        return nil
    }

    func testRunsTree() throws {
        let item = try XCTUnwrap(try cases("runRows").first)
        let rows = RepoCi.runRows(try runs(item), jobs: try jobs(item), now: try ciDate(item["now"]))
        let tree = RepoCiView.runsTree(rows)
        XCTAssertEqual(id(tree), RepoCi.TestIds.runs)
        XCTAssertEqual(component(tree), "Section")
        let rowNodes = children(tree)
        XCTAssertEqual(rowNodes.map(id), rows.map { "repo-run.\($0.id)" })
        let first = object(rowNodes.first)
        let props = object(first["props"])
        XCTAssertEqual(props["identifier"], .string("#13"))
        XCTAssertEqual(props["subtitle"], .string("main · ddddddd · Merge train"))
        XCTAssertNil(props["meta"])
        XCTAssertEqual(props["pressable"], .bool(true))
        let leading = object(object(first["slots"])["leading"])
        XCTAssertEqual(object(leading["props"])["name"], .string("ci-queued"))
        XCTAssertEqual(object(leading["props"])["tone"], .string("neutral"))
        let event = object(object(object(first["on"])["press"])["event"])
        XCTAssertEqual(event["name"], .string(RepoCiView.Events.openRun))
        XCTAssertEqual(object(event["context"])["run"], .string("r13"))
        XCTAssertEqual(object(object(rowNodes[1])["props"])["meta"], .string("5m 0s"))
    }

    func testSectionTreeGithubManager() {
        let tree = RepoCiView.sectionTree(
            provider: "github",
            pipelines: RepoCi.pipelineRows(RepoCi.specimenRuns),
            suggestions: ["test (linux)"],
            secrets: RepoCi.specimenSecrets,
            githubStatuses: true,
            actionsRunners: false,
            manager: true
        )
        XCTAssertEqual(id(tree), RepoCi.TestIds.ci)
        XCTAssertEqual(object(object(tree)["props"])["title"], .string("CI"))
        XCTAssertEqual(children(tree).map(id), [
            "repo-ci-pipelines", "repo-ci-required-row", "repo-ci-secrets-heading", "repo-ci-secrets",
            "repo-ci-add-secret", "repo-ci-github-statuses-row", "repo-ci-actions-runners-row",
        ])
        let pipelines = children(children(tree)[0])
        XCTAssertEqual(pipelines.map(id), ["repo-ci-pipelines.r13", "repo-ci-pipelines.r11"])
        let chip = object(object(object(pipelines[0])["slots"])["trailing"])
        XCTAssertEqual(object(chip["props"])["label"], .string("Running"))
        XCTAssertEqual(object(chip["props"])["dot"], .string("$color.warning"))
        let add = object(children(tree)[4])
        XCTAssertEqual(object(add["props"]), ["label": .string("Add secret"), "variant": .string("outline"), "size": .string("sm")])
        let secret = children(children(tree)[3])[0]
        let delete = object(object(object(secret)["slots"])["trailing"])
        XCTAssertEqual(object(delete["props"]), ["label": .string("Delete"), "variant": .string("ghost"), "size": .string("sm")])
        let statuses = object(children(children(tree)[5]).first)
        XCTAssertEqual(object(statuses["props"])["checked"], .bool(true))
        XCTAssertEqual(object(statuses["props"])["disabled"], .bool(false))
    }

    func testSectionTreeMemberAndEcode() {
        let tree = RepoCiView.sectionTree(
            provider: "ecode", pipelines: [], suggestions: [], secrets: [],
            githubStatuses: false, actionsRunners: false, manager: false
        )
        XCTAssertEqual(children(tree).map(id), ["repo-ci-pipelines", "repo-ci-required-row", "repo-ci-secrets-heading", "repo-ci-secrets"])
        XCTAssertEqual(component(children(tree)[0]), "Text")
        let chipInput = object(children(children(tree)[1]).first)
        XCTAssertEqual(object(chipInput["props"])["disabled"], .bool(true))
        // Loading: the secrets block waits.
        let loading = RepoCiView.sectionTree(
            provider: "ecode", pipelines: [], suggestions: [], secrets: nil,
            githubStatuses: false, actionsRunners: false, manager: true
        )
        XCTAssertEqual(children(loading).map(id), ["repo-ci-pipelines", "repo-ci-required-row"])
    }

    func testRequiredFrom() {
        XCTAssertEqual(RepoCiView.requiredFrom([" build ", "", "build", "lint"]), ["build", "lint"])
    }
}

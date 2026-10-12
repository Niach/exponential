import XCTest
@testable import ExpCore

// feedback-states.json `starting` (final review): one startId per draft
// start, bound to the device + the draft fingerprint. Recheck R2: the
// fingerprint holds every `starting.fingerprint.fields` entry and replays
// `starting.fingerprint.cases`. Android mirrors these cases
// (`DraftStartTest.kt`).
final class DraftStartTests: XCTestCase {
    private let draft = DraftStart.Fingerprint(
        deviceId: "mac", text: "fix the login", issueIds: ["i1"], attachments: ["a1"],
        agent: "claude", model: "opus", effort: "high", planMode: false
    )
    private var counter = 0
    private func mint() -> String {
        counter += 1
        return "id-\(counter)"
    }

    private func with(
        _ base: DraftStart.Fingerprint,
        deviceId: String? = nil,
        text: String? = nil,
        issueIds: [String]? = nil,
        actionId: String?? = nil,
        attachments: [String]? = nil,
        model: String? = nil,
        account: String? = nil
    ) -> DraftStart.Fingerprint {
        DraftStart.Fingerprint(
            deviceId: deviceId ?? base.deviceId,
            text: text ?? base.text,
            issueIds: issueIds ?? Array(base.issueIds),
            actionId: actionId ?? base.actionId,
            actionInputs: base.actionInputs,
            repositoryId: base.repositoryId,
            attachments: attachments ?? Array(base.attachments),
            agent: base.agent,
            model: model ?? base.model,
            effort: base.effort,
            planMode: base.planMode,
            account: account ?? base.account,
            mcpServerIds: Array(base.mcpServerIds),
            computerUse: base.computerUse,
            ultracode: base.ultracode,
            subagentModel: base.subagentModel,
            resume: base.resume
        )
    }

    func testTheFirstStartMints() {
        let start = DraftStart.resolve(pending: nil, fingerprint: draft, mint: mint)
        XCTAssertEqual(start.startId, "id-1")
        XCTAssertEqual(start.deviceId, "mac")
    }

    func testTheSameDraftOnTheSameDeviceReusesTheId() {
        let first = DraftStart.resolve(pending: nil, fingerprint: draft, mint: mint)
        let again = DraftStart.resolve(pending: first, fingerprint: draft, mint: mint)
        XCTAssertEqual(again, first)
    }

    func testAnotherDeviceMintsAFreshId() {
        let first = DraftStart.resolve(pending: nil, fingerprint: draft, mint: mint)
        let other = DraftStart.resolve(pending: first, fingerprint: with(draft, deviceId: "linux"), mint: mint)
        XCTAssertEqual(other.startId, "id-2")
    }

    func testAChangedDraftMintsAFreshId() {
        let first = DraftStart.resolve(pending: nil, fingerprint: draft, mint: mint)
        let changes = [
            with(draft, text: "fix it"),
            with(draft, issueIds: ["i1", "i2"]),
            with(draft, issueIds: [], actionId: .some("act")),
            with(draft, attachments: []),
            with(draft, model: "sonnet"),
            with(draft, account: "profile-2"),
        ]
        for change in changes {
            let next = DraftStart.resolve(pending: first, fingerprint: change, mint: mint)
            XCTAssertNotEqual(next.startId, first.startId)
        }
    }

    func testALandingClearsOnlyAnUnchangedDraft() {
        let start = DraftStart.resolve(pending: nil, fingerprint: draft, mint: mint)
        XCTAssertTrue(start.landsInto(draft))
        XCTAssertFalse(start.landsInto(with(draft, text: "a new prompt", issueIds: [], attachments: [])))
    }

    func testTheWireOptionsFeedTheFingerprint() {
        let options = SteerStartOptions(
            agent: "claude", model: "opus", subagentModel: "haiku", effort: "high",
            ultracode: true, planMode: true, resume: true, account: "acc1",
            mcpServerIds: ["m2", "m1"], computerUse: true
        )
        let viaOptions = DraftStart.Fingerprint(deviceId: "d1", text: "x", options: options)
        let spelled = DraftStart.Fingerprint(
            deviceId: "d1", text: "x", agent: "claude", model: "opus", effort: "high",
            planMode: true, account: "acc1", mcpServerIds: ["m1", "m2"], computerUse: true,
            ultracode: true, subagentModel: "haiku", resume: true
        )
        XCTAssertEqual(viaOptions, spelled)
    }

    func testTheQuietWatchSpansTheDedupeWindow() {
        XCTAssertEqual(StartedRunMatch.watchWindow, 1800)
        XCTAssertEqual(FeedbackStates.Starting.watchMs, FeedbackStates.Starting.dedupeMs)
    }

    // MARK: - feedback-states.json `starting.fingerprint`

    private func fingerprintFixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/feedback-states.json")
        let json = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any]
        )
        let starting = try XCTUnwrap(json["starting"] as? [String: Any])
        return try XCTUnwrap(starting["fingerprint"] as? [String: Any])
    }

    private func fingerprint(_ raw: [String: Any]) throws -> DraftStart.Fingerprint {
        func string(_ key: String) -> String? { raw[key] as? String }
        func bool(_ key: String) -> Bool? { (raw[key] as? NSNumber)?.boolValue }
        func list(_ key: String) -> [String] { raw[key] as? [String] ?? [] }
        return DraftStart.Fingerprint(
            deviceId: try XCTUnwrap(string("deviceId")),
            text: try XCTUnwrap(string("text")),
            issueIds: list("issueIds"),
            actionId: string("actionId"),
            actionInputs: raw["actionInputs"] as? [String: String],
            repositoryId: string("repositoryId"),
            attachments: list("attachments"),
            agent: string("agent"),
            model: string("model"),
            effort: string("effort"),
            planMode: bool("planMode"),
            account: string("account"),
            mcpServerIds: list("mcpServerIds"),
            computerUse: bool("computerUse"),
            ultracode: bool("ultracode"),
            subagentModel: string("subagentModel"),
            resume: bool("resume") ?? false
        )
    }

    func testTheFingerprintReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try fingerprintFixture()["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"] as? String ?? "?"
            let a = try fingerprint(try XCTUnwrap(c["a"] as? [String: Any]))
            let b = try fingerprint(try XCTUnwrap(c["b"] as? [String: Any]))
            let same = try XCTUnwrap((c["same"] as? NSNumber)?.boolValue)
            XCTAssertEqual(a == b, same, name)
            let first = DraftStart.resolve(pending: nil, fingerprint: a, mint: mint)
            let next = DraftStart.resolve(pending: first, fingerprint: b, mint: mint)
            XCTAssertEqual(next.startId == first.startId, same, name)
        }
    }

    func testEveryFingerprintFieldIsCovered() throws {
        let fields = try XCTUnwrap(try fingerprintFixture()["fields"] as? [String])
        let names = Set(fields.map { $0.split(separator: " ").first.map(String.init) ?? $0 })
        let mirrored = Set(Mirror(reflecting: draft).children.compactMap(\.label))
        XCTAssertEqual(names, mirrored)
    }
}

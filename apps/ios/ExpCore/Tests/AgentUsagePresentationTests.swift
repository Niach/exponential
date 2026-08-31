import Foundation
import XCTest
@testable import ExpCore

// EXP-484: the agent auth-status / usage presentation rules, locked against
// the SAME fixture the web, Android and desktop mirrors use (same test names).
// Everything here is pure: parsing the device-reported jsonb, grouping the
// reported windows into cards (EXP-688), the severity thresholds, the
// freshness gate, and the countdown/caption strings — all four clients must
// print them byte-for-byte.
final class AgentUsagePresentationTests: XCTestCase {
    private let now = WireTimestamps.parse("2026-08-28T10:00:00Z")!

    private let usageJson = """
        {"fetchedAt":"2026-08-28T09:58:00Z","stale":false,"windows":[
        {"key":"session","label":"5h","percent":42,"resetsAt":"2026-08-28T12:10:30Z"},
        {"key":"weekly","label":"Week","percent":78,"resetsAt":"2026-09-01T00:00:00Z"},
        {"key":"model:fable","label":"Fable","percent":96,"resetsAt":null}]}
        """

    private let accountsJson = """
        {"claude":{"signedIn":true,"email":"danny@yourev.at","plan":"max",
        "checkedAt":"2026-08-28T09:58:00Z"},
        "codex":{"signedIn":false,"checkedAt":"2026-08-28T09:58:00Z"},
        "pi":{"signedIn":true,"plan":"anthropic (oauth)",
        "checkedAt":"2026-08-28T09:58:00Z"}}
        """

    // A device that over-reports must not paint outside the bar; a negative
    // percentage floors at zero rather than inverting it.
    func testParseClampsPercent() throws {
        let usage = try XCTUnwrap(AgentUsagePresentation.parse("""
            {"fetchedAt":"2026-08-28T09:58:00Z","windows":[
            {"key":"session","label":"5h","percent":150},
            {"key":"weekly","label":"Week","percent":-20},
            {"key":"credits","label":"Credits","percent":null}]}
            """))
        let windows = try XCTUnwrap(usage.windows)
        XCTAssertEqual(windows[0].percent, 100)
        XCTAssertEqual(windows[1].percent, 0)
        XCTAssertNil(windows[2].percent)
        XCTAssertEqual(windows[0].id, "session")
    }

    // Absent or undecodable JSON yields NO usage — never a half-read report.
    // An object with no windows is legal (the device reported nothing yet).
    func testParseRejectsMalformed() {
        XCTAssertNil(AgentUsagePresentation.parse(nil))
        XCTAssertNil(AgentUsagePresentation.parse(""))
        XCTAssertNil(AgentUsagePresentation.parse("not json"))
        XCTAssertNil(AgentUsagePresentation.parse("[1,2]"))
        // A window missing its identity is not a window — it drops, and the
        // rest of the report survives (matching the web/Android parsers: one
        // bad entry must never blank a machine's whole usage).
        let mixed = AgentUsagePresentation.parse(
            #"{"windows":[{"label":"5h"},{"key":"weekly","label":"Week","percent":78}]}"#
        )
        XCTAssertEqual(mixed?.windows?.map(\.key), ["weekly"])
        // A window with no label still selects and remembers — only `key` is
        // load-bearing.
        XCTAssertEqual(
            AgentUsagePresentation.parse(#"{"windows":[{"key":"weekly"}]}"#)?.windows?.first?.label,
            ""
        )
        XCTAssertNotNil(AgentUsagePresentation.parse("{}"))
        XCTAssertNil(AgentUsagePresentation.parseMap("not json"))
        XCTAssertNil(AgentUsagePresentation.parseAccounts("not json"))
        XCTAssertNil(AgentUsagePresentation.parseMap(nil))
        XCTAssertNil(AgentUsagePresentation.parseAccounts(nil))
    }

    // EXP-688: every reported window is drawn, grouped Current session /
    // (untitled weekly) / Other. There is no pinned window any more — the
    // cards and their titles are the whole contract, locked ×4. EXP-694
    // emptied the weekly group's title: its card titles already say "All
    // models" / "<Label> only", so every renderer skips the header.
    func testUsageGroupsSplitCurrentWeeklyAndOther() throws {
        let usage = try XCTUnwrap(AgentUsagePresentation.parse("""
            {"fetchedAt":"2026-08-28T09:58:00Z","stale":false,"windows":[
            {"key":"session","label":"5h","percent":42,"resetsAt":"2026-08-28T12:10:30Z"},
            {"key":"weekly","label":"Week","percent":78,"resetsAt":"2026-09-01T00:00:00Z"},
            {"key":"model:fable","label":"Fable","percent":96,"resetsAt":null},
            {"key":"credits","label":"Credits","percent":16,"resetsAt":null}]}
            """))
        let groups = AgentUsagePresentation.usageGroups(usage, now: now)
        XCTAssertEqual(groups.map(\.key), ["session", "weekly", "other"])
        XCTAssertEqual(groups.map(\.title), ["Current session", "", "Other"])
        XCTAssertEqual(groups[0].cards.map(\.title), ["Current session"])
        // The all-models window leads its group; the per-model ones follow in
        // report order.
        XCTAssertEqual(groups[1].cards.map(\.title), ["All models", "Fable only"])
        XCTAssertEqual(groups[2].cards.map(\.title), ["Credits"])
        XCTAssertEqual(groups[0].cards[0].percent, 42)
        XCTAssertEqual(groups[0].cards[0].caption, "resets in 2h 10m")
        XCTAssertEqual(groups[1].cards[0].severity, .warning)
        XCTAssertEqual(groups[1].cards[1].severity, .danger)
        // A window that never resets has nothing to say under its bar.
        XCTAssertEqual(groups[1].cards[1].caption, "")
        XCTAssertEqual(groups[2].cards[0].key, "credits")

        // An idle session window reads as not-started rather than "0%" alone,
        // and an empty group is dropped entirely.
        let idle = try XCTUnwrap(AgentUsagePresentation.parse("""
            {"fetchedAt":"2026-08-28T09:58:00Z","windows":[
            {"key":"session","label":"5h","percent":0,"resetsAt":null}]}
            """))
        let idleGroups = AgentUsagePresentation.usageGroups(idle, now: now)
        XCTAssertEqual(idleGroups.map(\.key), ["session"])
        XCTAssertEqual(idleGroups[0].cards[0].caption, "Starts when a message is sent")
        XCTAssertEqual(idleGroups[0].cards[0].severity, .normal)
        XCTAssertTrue(AgentUsagePresentation.usageGroups(AgentUsage(windows: []), now: now).isEmpty)
    }

    func testSeverityThresholds() {
        XCTAssertEqual(AgentUsagePresentation.severity(nil), .normal)
        XCTAssertEqual(AgentUsagePresentation.severity(0), .normal)
        XCTAssertEqual(AgentUsagePresentation.severity(42), .normal)
        XCTAssertEqual(AgentUsagePresentation.severity(74.9), .normal)
        XCTAssertEqual(AgentUsagePresentation.severity(75), .warning)
        XCTAssertEqual(AgentUsagePresentation.severity(94.9), .warning)
        XCTAssertEqual(AgentUsagePresentation.severity(95), .danger)
        XCTAssertEqual(AgentUsagePresentation.severity(100), .danger)
        XCTAssertEqual(AgentUsagePresentation.severity(150), .danger)
    }

    // Fail-closed: only numbers fetched inside the window may be drawn, and an
    // unreadable stamp is as good as none. A future stamp is clock skew.
    func testFreshnessWindow() {
        XCTAssertEqual(AgentUsagePresentation.freshWindow, 15 * 60)
        XCTAssertTrue(
            AgentUsagePresentation.isFresh(fetchedAt: "2026-08-28T09:58:00Z", now: now)
        )
        XCTAssertTrue(
            AgentUsagePresentation.isFresh(fetchedAt: "2026-08-28T09:45:01Z", now: now)
        )
        XCTAssertFalse(
            AgentUsagePresentation.isFresh(fetchedAt: "2026-08-28T09:45:00Z", now: now)
        )
        XCTAssertTrue(
            AgentUsagePresentation.isFresh(fetchedAt: "2026-08-28T10:05:00Z", now: now)
        )
        XCTAssertFalse(AgentUsagePresentation.isFresh(fetchedAt: nil, now: now))
        XCTAssertFalse(AgentUsagePresentation.isFresh(fetchedAt: "whenever", now: now))
    }

    // The four countdown strings are locked ×4 — no localization, no plurals,
    // and a zero smaller unit is dropped rather than printed.
    func testResetCountdownStrings() {
        XCTAssertEqual(
            AgentUsagePresentation.resetCountdown(resetsAt: "2026-08-28T12:10:30Z", now: now),
            "resets in 2h 10m"
        )
        XCTAssertEqual(
            AgentUsagePresentation.resetCountdown(resetsAt: "2026-09-01T00:00:00Z", now: now),
            "resets in 3d 14h"
        )
        XCTAssertEqual(
            AgentUsagePresentation.resetCountdown(resetsAt: "2026-08-28T10:45:00Z", now: now),
            "resets in 45m"
        )
        XCTAssertEqual(
            AgentUsagePresentation.resetCountdown(resetsAt: "2026-08-28T10:00:30Z", now: now),
            "resets soon"
        )
        XCTAssertEqual(
            AgentUsagePresentation.resetCountdown(resetsAt: "2026-08-28T09:00:00Z", now: now),
            "resets soon"
        )
        XCTAssertEqual(
            AgentUsagePresentation.resetCountdown(resetsAt: "2026-08-28T12:00:00Z", now: now),
            "resets in 2h"
        )
        XCTAssertEqual(
            AgentUsagePresentation.resetCountdown(resetsAt: "2026-08-31T10:00:00Z", now: now),
            "resets in 3d"
        )
        XCTAssertNil(AgentUsagePresentation.resetCountdown(resetsAt: nil, now: now))
        XCTAssertNil(AgentUsagePresentation.resetCountdown(resetsAt: "whenever", now: now))
    }

    // claude reports email + plan, codex may be signed out, pi reports a
    // provider and never an email. A machine that never probed is `unknown` —
    // which is NOT "signed out". EXP-694: an email wins outright — no "signed
    // in as" prefix and no " · <plan>" suffix.
    func testAccountCaptions() throws {
        let accounts = try XCTUnwrap(AgentUsagePresentation.parseAccounts(accountsJson))
        XCTAssertEqual(
            AgentUsagePresentation.accountRow(agent: "claude", account: accounts["claude"]),
            "claude · danny@yourev.at"
        )
        XCTAssertEqual(
            AgentUsagePresentation.accountRow(agent: "codex", account: accounts["codex"]),
            "codex · signed out"
        )
        XCTAssertEqual(
            AgentUsagePresentation.accountRow(agent: "pi", account: accounts["pi"]),
            "pi · anthropic (oauth)"
        )
        XCTAssertEqual(
            AgentUsagePresentation.accountRow(agent: "codex", account: nil),
            "codex · unknown"
        )
        XCTAssertEqual(
            AgentUsagePresentation.accountCaption(
                AgentAccount(signedIn: true, email: "danny@yourev.at")
            ),
            "danny@yourev.at"
        )
        XCTAssertEqual(
            AgentUsagePresentation.accountCaption(AgentAccount(signedIn: true)),
            "signed in"
        )
    }

    // The session bar renders only for a live run whose own machine reported
    // FRESH, non-empty numbers for the agent the run uses.
    func testSessionUsageHiddenCases() {
        let row = device(agentUsage: #"{"claude":\#(usageJson)}"#)
        XCTAssertEqual(
            AgentUsagePresentation.sessionUsage(
                session: session(), devices: [row], now: now
            )?.agent,
            "claude"
        )
        XCTAssertEqual(
            AgentUsagePresentation.sessionUsage(
                session: session(), devices: [row], now: now
            )?.usage.windows?.count,
            3
        )
        // Ended run: the host's limits stopped being the viewer's business.
        XCTAssertNil(AgentUsagePresentation.sessionUsage(
            session: session(status: "ended"), devices: [row], now: now
        ))
        // No agent recorded on the run (a pre-EXP-484 row).
        XCTAssertNil(AgentUsagePresentation.sessionUsage(
            session: session(agent: nil), devices: [row], now: now
        ))
        // The run's agent is not the one the machine reported for.
        XCTAssertNil(AgentUsagePresentation.sessionUsage(
            session: session(agent: "codex"), devices: [row], now: now
        ))
        // No devices row for the stamped id, and no stamped id at all.
        XCTAssertNil(AgentUsagePresentation.sessionUsage(
            session: session(), devices: [], now: now
        ))
        XCTAssertNil(AgentUsagePresentation.sessionUsage(
            session: session(deviceId: nil), devices: [row], now: now
        ))
        // Stale numbers must not read as current ones.
        XCTAssertNil(AgentUsagePresentation.sessionUsage(
            session: session(), devices: [row],
            now: WireTimestamps.parse("2026-08-28T10:20:00Z")!
        ))
        // Reported, but with nothing to draw.
        XCTAssertNil(AgentUsagePresentation.sessionUsage(
            session: session(),
            devices: [device(agentUsage: #"{"claude":{"fetchedAt":"2026-08-28T09:58:00Z","windows":[]}}"#)],
            now: now
        ))
        XCTAssertNil(AgentUsagePresentation.sessionUsage(
            session: session(), devices: [device(agentUsage: nil)], now: now
        ))
    }

    func testParsesAgentLoginResult() {
        // Codex publishes a device code beside the URL.
        let withCode = AgentUsagePresentation.parseAgentLoginResult(
            #"{"agent":"codex","phase":"url","url":"https://auth.openai.com/device","code":"ABCD-EFGHI"}"#
        )
        XCTAssertEqual(withCode?.url, "https://auth.openai.com/device")
        XCTAssertEqual(withCode?.code, "ABCD-EFGHI")
        // Claude's flow has none — the link alone is the whole answer.
        let urlOnly = AgentUsagePresentation.parseAgentLoginResult(
            #"{"agent":"claude","phase":"url","url":"https://claude.ai/oauth/authorize?x=1"}"#
        )
        XCTAssertEqual(urlOnly?.url, "https://claude.ai/oauth/authorize?x=1")
        XCTAssertNil(urlOnly?.code)
        // Anything that is not a link payload — a failure sentence, an older
        // build's plain text, nothing at all — is shown verbatim instead.
        XCTAssertNil(AgentUsagePresentation.parseAgentLoginResult("The machine refused the command."))
        XCTAssertNil(AgentUsagePresentation.parseAgentLoginResult(
            #"{"agent":"claude","phase":"failed","message":"timed out"}"#
        ))
        XCTAssertNil(AgentUsagePresentation.parseAgentLoginResult(nil))
    }

    private func session(
        agent: String? = "claude",
        status: String = "running",
        deviceId: String? = "dev-1"
    ) -> CodingSessionEntity {
        CodingSessionEntity(
            id: "sess-1", issueId: "issue-1", teamId: "team-1", userId: "me",
            deviceLabel: "macbook", deviceId: deviceId, status: status, agent: agent,
            startedAt: "2026-08-28T09:00:00Z", endedAt: nil,
            createdAt: "2026-08-28T09:00:00Z", updatedAt: "2026-08-28T09:30:00Z"
        )
    }

    private func device(agentUsage: String?) -> DeviceEntity {
        DeviceEntity(
            id: "row-1", userId: "me", deviceId: "dev-1", label: "macbook",
            agentAccounts: accountsJson, agentUsage: agentUsage,
            agentUsageAt: "2026-08-28T09:58:00Z"
        )
    }
}

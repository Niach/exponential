import XCTest
@testable import ExpCore

// EXP-92: the Universal-Link parser must stay the exact inverse of the URL
// builder — these are the only two path shapes the AASA claims.
// EXP-180 (the great rename): the canonical web route is
// `/t/{team}/boards/{board}/issues/{identifier}`; the legacy `/w/` and
// `/projects/` forms are dead on the web (no redirects) and must not parse.
final class WebLinksTests: XCTestCase {
    func testParseIssueUrl() {
        let url = URL(string: "https://app.exponential.at/t/acme/boards/web/issues/EXP-42")!
        XCTAssertEqual(
            WebLinks.parse(url),
            .issue(teamSlug: "acme", boardSlug: "web", identifier: "EXP-42")
        )
    }

    func testParseInviteUrl() {
        let url = URL(string: "https://app.exponential.at/invite/abc123")!
        XCTAssertEqual(WebLinks.parse(url), .invite(token: "abc123"))
    }

    func testMintParseRoundTrip() {
        let minted = WebLinks.issue(
            instanceUrl: "https://app.exponential.at/",
            teamSlug: "acme",
            boardSlug: "web",
            identifier: "EXP-42"
        )!
        XCTAssertEqual(minted.absoluteString, "https://app.exponential.at/t/acme/boards/web/issues/EXP-42")
        XCTAssertEqual(
            WebLinks.parse(minted),
            .issue(teamSlug: "acme", boardSlug: "web", identifier: "EXP-42")
        )
    }

    // EXP-825: the team's Agent page — the ONE launcher — is linkable too.
    func testParseAgentUrl() {
        let url = URL(string: "https://app.exponential.at/t/acme/agent")!
        XCTAssertEqual(WebLinks.parse(url), .agent(teamSlug: "acme"))
        let trailing = URL(string: "https://app.exponential.at/t/acme/agent/")!
        XCTAssertEqual(WebLinks.parse(trailing), .agent(teamSlug: "acme"))
    }

    func testAgentLinkMintParseRoundTrip() throws {
        let minted = try XCTUnwrap(
            WebLinks.agent(instanceUrl: "https://app.exponential.at/", teamSlug: "acme")
        )
        XCTAssertEqual(minted.absoluteString, "https://app.exponential.at/t/acme/agent")
        XCTAssertEqual(WebLinks.parse(minted), .agent(teamSlug: "acme"))
        XCTAssertNil(WebLinks.agent(instanceUrl: nil, teamSlug: "acme"))
        XCTAssertNil(WebLinks.agent(instanceUrl: "https://app.exponential.at", teamSlug: " "))
    }

    // EXP-1288: a thread link from Mail, Safari or a share sheet opens the
    // Thread screen, like the in-app `AppLink.thread`.
    func testParseThreadUrl() {
        let url = URL(string: "https://app.exponential.at/t/acme/threads/7f0c2b1e-1111-4222-8333-944455556666")!
        XCTAssertEqual(
            WebLinks.parse(url),
            .thread(teamSlug: "acme", threadId: "7f0c2b1e-1111-4222-8333-944455556666")
        )
        let trailing = URL(string: "https://app.exponential.at/t/acme/threads/abc/")!
        XCTAssertEqual(WebLinks.parse(trailing), .thread(teamSlug: "acme", threadId: "abc"))
    }

    func testThreadLinkMintParseRoundTrip() throws {
        let minted = try XCTUnwrap(
            WebLinks.thread(instanceUrl: "https://app.exponential.at/", teamSlug: "acme", threadId: "abc")
        )
        XCTAssertEqual(minted.absoluteString, "https://app.exponential.at/t/acme/threads/abc")
        XCTAssertEqual(WebLinks.parse(minted), .thread(teamSlug: "acme", threadId: "abc"))
        XCTAssertNil(WebLinks.thread(instanceUrl: nil, teamSlug: "acme", threadId: "abc"))
        XCTAssertNil(WebLinks.thread(instanceUrl: "https://app.exponential.at", teamSlug: " ", threadId: "abc"))
        XCTAssertNil(WebLinks.thread(instanceUrl: "https://app.exponential.at", teamSlug: "acme", threadId: ""))
    }

    // EXP-1291: the PR body footer links the ROOT session with
    // `?view=guide` — the thread on its Guide; a bare session link opens it
    // without.
    func testParseSessionUrlCarriesTheGuideView() {
        let guide = URL(string: "https://app.exponential.at/t/acme/sessions/abc?view=guide")!
        XCTAssertEqual(WebLinks.parse(guide), .session(teamSlug: "acme", sessionId: "abc", guide: true))
        let bare = URL(string: "https://app.exponential.at/t/acme/sessions/abc/")!
        XCTAssertEqual(WebLinks.parse(bare), .session(teamSlug: "acme", sessionId: "abc", guide: false))
        let other = URL(string: "https://app.exponential.at/t/acme/sessions/abc?view=run")!
        XCTAssertEqual(WebLinks.parse(other), .session(teamSlug: "acme", sessionId: "abc", guide: false))
    }

    func testTrailingSlashTolerated() {
        let url = URL(string: "https://app.exponential.at/t/acme/boards/web/issues/EXP-42/")!
        XCTAssertEqual(
            WebLinks.parse(url),
            .issue(teamSlug: "acme", boardSlug: "web", identifier: "EXP-42")
        )
    }

    func testRejectsUnclaimedPaths() {
        for path in [
            "/", "/t/acme", "/t/acme/boards/web", "/t/acme/inbox",
            "/t/acme/boards/web/issues", "/t/acme/boards/web/issues/EXP-1/changes",
            // The dead legacy forms (EXP-180): `/w/` and `/projects/` no longer
            // exist on the web, so the app must not claim them either.
            "/w/acme/projects/web/issues/EXP-42", "/w/acme/boards/web/issues/EXP-42",
            "/t/acme/projects/web/issues/EXP-42",
            // Only the Agent page itself — a deeper agent path is not
            // something the app renders from a link.
            "/t/acme/agent/extra", "/agent",
            // Only the session page itself (EXP-1291).
            "/t/acme/sessions", "/t/acme/sessions/abc/files", "/sessions/abc",
            // Only the thread page itself (EXP-1288).
            "/t/acme/threads", "/t/acme/threads/abc/guide", "/threads/abc",
            "/invite", "/auth/login",
        ] {
            let url = URL(string: "https://app.exponential.at\(path)")!
            XCTAssertNil(WebLinks.parse(url), "expected nil for \(path)")
        }
    }

    // EXP-188 join-team paste tolerance: a full invite link or a raw token —
    // mirror of the desktop `extract_token` (join_team.rs) test cases.
    func testExtractInviteTokenAcceptsLinksAndRawTokens() {
        XCTAssertEqual(
            WebLinks.extractInviteToken("https://app.exponential.at/invite/tok123"),
            "tok123"
        )
        XCTAssertEqual(
            WebLinks.extractInviteToken("https://app.exponential.at/invite/tok123?x=1"),
            "tok123"
        )
        XCTAssertEqual(
            WebLinks.extractInviteToken("https://app.exponential.at/invite/tok123#frag"),
            "tok123"
        )
        XCTAssertEqual(
            WebLinks.extractInviteToken("https://app.exponential.at/invite/tok123/"),
            "tok123"
        )
        XCTAssertEqual(WebLinks.extractInviteToken(" tok123 "), "tok123")
        XCTAssertNil(WebLinks.extractInviteToken("not a token"))
        XCTAssertNil(WebLinks.extractInviteToken(""))
        XCTAssertNil(WebLinks.extractInviteToken("   "))
        XCTAssertNil(WebLinks.extractInviteToken("https://x/invite/"))
    }

    // EXP-725: the invite creator mints `{base}/invite/{token}` — and what it
    // hands out must paste straight back into the join field.
    func testInviteLinkRoundTripsThroughTheTokenExtractor() throws {
        let minted = try XCTUnwrap(
            WebLinks.invite(instanceUrl: "https://app.exponential.at/", token: "tok123")
        )
        XCTAssertEqual(minted.absoluteString, "https://app.exponential.at/invite/tok123")
        XCTAssertEqual(WebLinks.extractInviteToken(minted.absoluteString), "tok123")
        XCTAssertEqual(WebLinks.parse(minted), .invite(token: "tok123"))
    }

    func testInviteLinkNeedsBothABaseAndAToken() {
        XCTAssertNil(WebLinks.invite(instanceUrl: nil, token: "tok123"))
        XCTAssertNil(WebLinks.invite(instanceUrl: "   ", token: "tok123"))
        XCTAssertNil(WebLinks.invite(instanceUrl: "https://app.exponential.at", token: " "))
    }
}

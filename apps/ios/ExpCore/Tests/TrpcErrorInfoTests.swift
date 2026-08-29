import Foundation
import XCTest
@testable import ExpCore

// Compliance lock (EXP-216 / App Store 3.1.1): server plan-cap messages carry
// purchase language ("Add seats or upgrade…") that must never render in the
// app — `trpcUserMessage` swaps them for the neutral copy, and the REV2-55
// team-delete billing gate for copy that names the web instead of a Billing
// screen this app doesn't have. Ordinary errors keep passing the server
// message through verbatim.
final class TrpcErrorInfoTests: XCTestCase {
    private func envelope(message: String, code: String?, batched: Bool = false) -> String {
        let codeJson = code.map { "\"data\": {\"code\": \"\($0)\"}," } ?? ""
        let error = "{\"error\": {\(codeJson) \"message\": \"\(message)\"}}"
        return batched ? "[\(error)]" : error
    }

    func testPlanLimitMessageIsNeutralized() {
        let body = envelope(
            message: "Your plan allows up to 1 seat. Add seats or upgrade to invite more teammates.",
            code: "PRECONDITION_FAILED"
        )
        let error = TrpcError.httpError(412, body)
        XCTAssertTrue(error.isPlanLimitError)
        XCTAssertEqual(error.trpcUserMessage, planLimitNeutralMessage)
        XCTAssertFalse(error.trpcUserMessage.localizedCaseInsensitiveContains("upgrade"))
        XCTAssertFalse(error.trpcUserMessage.localizedCaseInsensitiveContains("seat"))
    }

    func testPlanLimitMessageIsNeutralizedInBatchedEnvelope() {
        let body = envelope(
            message: "Your plan allows up to 10 teams on the free plan. Upgrade to create more.",
            code: "PRECONDITION_FAILED",
            batched: true
        )
        let error = TrpcError.httpError(412, body)
        XCTAssertTrue(error.isPlanLimitError)
        XCTAssertEqual(error.trpcUserMessage, planLimitNeutralMessage)
    }

    // REV2-55: `teams.delete` refuses a team whose subscription is still live.
    // The server's copy points at "team settings → Billing" — a web-only
    // screen — so the app substitutes wording that names the web.

    func testTeamDeleteSubscriptionGateIsRewrittenForNative() {
        let body = envelope(
            message: "This team has an active subscription. Cancel the subscription in team settings → Billing before deleting the team.",
            code: "PRECONDITION_FAILED"
        )
        let error = TrpcError.httpError(412, body)
        XCTAssertFalse(error.isPlanLimitError)
        XCTAssertEqual(error.trpcUserMessage, teamDeleteSubscriptionMessage)
        XCTAssertEqual(error.localizedDescription, teamDeleteSubscriptionMessage)
        XCTAssertFalse(error.trpcUserMessage.localizedCaseInsensitiveContains("team settings"))
    }

    func testOtherSubscriptionMessagesPassThroughVerbatim() {
        // billing.cancelSubscription's "has NO active subscription" is a
        // different precondition — the prefix must not swallow it.
        let body = envelope(message: "This team has no active subscription", code: "PRECONDITION_FAILED")
        let error = TrpcError.httpError(412, body)
        XCTAssertEqual(error.trpcUserMessage, "This team has no active subscription")
    }

    func testTeamDeleteGatePrefixWithoutPreconditionCodePassesThrough() {
        let body = envelope(message: "This team has an active subscription somewhere", code: "BAD_REQUEST")
        let error = TrpcError.httpError(400, body)
        XCTAssertEqual(error.trpcUserMessage, "This team has an active subscription somewhere")
    }

    func testOrdinaryMessagePassesThroughVerbatim() {
        let body = envelope(message: "Board not found", code: "NOT_FOUND")
        let error = TrpcError.httpError(404, body)
        XCTAssertFalse(error.isPlanLimitError)
        XCTAssertEqual(error.trpcUserMessage, "Board not found")
    }

    func testPreconditionFailedWithoutPrefixPassesThrough() {
        // PRECONDITION_FAILED is shared with non-billing preconditions — only
        // the "Your plan allows" prefix marks a plan cap.
        let body = envelope(message: "No repository linked to this board", code: "PRECONDITION_FAILED")
        let error = TrpcError.httpError(412, body)
        XCTAssertFalse(error.isPlanLimitError)
        XCTAssertEqual(error.trpcUserMessage, "No repository linked to this board")
    }

    func testPrefixWithoutPreconditionCodePassesThrough() {
        let body = envelope(message: "Your plan allows something", code: "BAD_REQUEST")
        let error = TrpcError.httpError(400, body)
        XCTAssertFalse(error.isPlanLimitError)
        XCTAssertEqual(error.trpcUserMessage, "Your plan allows something")
    }

    func testUnparsableBodyFallsBackToLocalizedDescription() {
        let error = TrpcError.httpError(500, "<html>gateway error</html>")
        XCTAssertFalse(error.isPlanLimitError)
        XCTAssertEqual(error.trpcUserMessage, error.localizedDescription)
    }

    // EXP-219: `localizedDescription` is sanitized at the source, so the many
    // surfaces that render it directly can never show a raw response body.

    func testLocalizedDescriptionShowsServerMessage() {
        let error = TrpcError.httpError(404, envelope(message: "Board not found", code: "NOT_FOUND"))
        XCTAssertEqual(error.localizedDescription, "Board not found")
    }

    func testLocalizedDescriptionNeutralizesPlanLimitCopy() {
        let body = envelope(
            message: "Your plan allows up to 1 seat. Add seats or upgrade to invite more teammates.",
            code: "PRECONDITION_FAILED"
        )
        let error = TrpcError.httpError(412, body)
        XCTAssertEqual(error.localizedDescription, planLimitNeutralMessage)
        XCTAssertFalse(error.localizedDescription.localizedCaseInsensitiveContains("upgrade"))
    }

    func testLocalizedDescriptionNeverEchoesUnparsableBody() {
        let error = TrpcError.httpError(502, "<html>bad gateway</html>")
        XCTAssertEqual(error.localizedDescription, "Request failed (HTTP 502)")
    }

    func testNestedJsonEnvelopeIsParsed() {
        let error = TrpcError.httpError(
            412, "{\"error\": {\"json\": {\"message\": \"No repository linked to this board\"}}}"
        )
        XCTAssertEqual(error.localizedDescription, "No repository linked to this board")
        XCTAssertEqual(error.trpcUserMessage, "No repository linked to this board")
    }

    // The tRPC `code` is the structured signal callers classify on — e.g. the
    // agent-session viewer treats NOT_FOUND / FORBIDDEN from steer.mintTicket
    // as terminal instead of retrying the reconnect loop forever.

    func testErrorCodeIsExtracted() {
        let error = TrpcError.httpError(404, envelope(message: "Coding session not found", code: "NOT_FOUND"))
        XCTAssertEqual(error.trpcErrorCode, "NOT_FOUND")
    }

    func testErrorCodeIsNilForNonTrpcFailures() {
        XCTAssertNil(TrpcError.httpError(502, "<html>bad gateway</html>").trpcErrorCode)
        XCTAssertNil(NSError(domain: "test", code: 1).trpcErrorCode)
    }

    func testNonTrpcErrorFallsBackToLocalizedDescription() {
        let error = NSError(domain: "test", code: 1, userInfo: [NSLocalizedDescriptionKey: "boom"])
        XCTAssertFalse(error.isPlanLimitError)
        XCTAssertEqual(error.trpcUserMessage, "boom")
    }

    // EXP-533: a request that never reached the server must read as "you are
    // offline", not as Apple's URLError copy ("A server with the specified
    // hostname could not be found."). The sentence is byte-identical on web,
    // Android and desktop.

    private static let offlineCodes: [URLError.Code] = [
        .notConnectedToInternet,
        .dnsLookupFailed,
        .cannotFindHost,
        .cannotConnectToHost,
        .networkConnectionLost,
        .timedOut,
        .internationalRoamingOff,
        .dataNotAllowed,
    ]

    func testTransportFailuresReadAsTheOfflineSentence() {
        for code in Self.offlineCodes {
            let error = URLError(code)
            XCTAssertTrue(error.isOfflineError, "\(code) should count as offline")
            XCTAssertEqual(error.userFacingMessage, offlineErrorMessage)
            // The 38 legacy call sites go through the alias and get it too.
            XCTAssertEqual(error.trpcUserMessage, offlineErrorMessage)
        }
    }

    func testOfflineSentenceIsTheLockedCopy() {
        XCTAssertEqual(offlineErrorMessage, "You're offline. Check your connection and try again.")
    }

    func testNonTransportUrlErrorKeepsItsDescription() {
        let error = URLError(.badServerResponse)
        XCTAssertFalse(error.isOfflineError)
        XCTAssertEqual(error.userFacingMessage, error.localizedDescription)
        XCTAssertNotEqual(error.userFacingMessage, offlineErrorMessage)
    }

    func testTrpcFailureIsNeverOffline() {
        // The server answered — that proves it was reachable.
        let error = TrpcError.httpError(500, envelope(message: "Internal server error", code: "INTERNAL_SERVER_ERROR"))
        XCTAssertFalse(error.isOfflineError)
        XCTAssertEqual(error.userFacingMessage, "Internal server error")
    }

    func testServerMessagesStillWinOverTheOfflineSentence() {
        // The 412 surfaces (plan caps, preconditions) are unchanged by EXP-533.
        let planCap = TrpcError.httpError(
            412,
            envelope(message: "Your plan allows up to 1 seat. Add seats or upgrade to invite more teammates.", code: "PRECONDITION_FAILED")
        )
        XCTAssertEqual(planCap.userFacingMessage, planLimitNeutralMessage)
        let precondition = TrpcError.httpError(
            412, envelope(message: "No repository linked to this board", code: "PRECONDITION_FAILED")
        )
        XCTAssertEqual(precondition.userFacingMessage, "No repository linked to this board")
    }

    // The "Fix conflicts" recovery run rebases and re-merges, so it is only
    // offered for a REAL content conflict: the server answers CONFLICT / 409
    // for that and PRECONDITION_FAILED for every other refusal.

    func testConflictIsRecognizedOn409() {
        let error = TrpcError.httpError(
            409, envelope(message: "This pull request has merge conflicts with the base branch.", code: "CONFLICT")
        )
        XCTAssertTrue(error.isMergeConflict)
    }

    func testNonConflictRefusalsAreNotConflicts() {
        let stale = TrpcError.httpError(
            412, envelope(message: "Head branch changed on GitHub. Refresh and try again.", code: "PRECONDITION_FAILED")
        )
        XCTAssertFalse(stale.isMergeConflict)
        let policy = TrpcError.httpError(
            412, envelope(message: "Squash merges are disabled for this repository", code: "PRECONDITION_FAILED")
        )
        XCTAssertFalse(policy.isMergeConflict)
        XCTAssertFalse(TrpcError.httpError(404, envelope(message: "Not found", code: "NOT_FOUND")).isMergeConflict)
        XCTAssertFalse(TrpcError.httpError(500, "<html>bad gateway</html>").isMergeConflict)
        XCTAssertFalse(URLError(.notConnectedToInternet).isMergeConflict)
        XCTAssertFalse(NSError(domain: "test", code: 1).isMergeConflict)
    }

    // TRANSITIONAL (EXP-533): remove once every server answers a real conflict with 409
    func testLegacy412ConflictMessageStillCountsAsAConflict() {
        let error = TrpcError.httpError(
            412,
            envelope(message: "This pull request has merge conflicts with the base branch.", code: "PRECONDITION_FAILED")
        )
        XCTAssertTrue(error.isMergeConflict)
    }

    func testMergeFailureCarriesMessageAndConflictFlag() {
        let conflict = MergeFailure(
            error: TrpcError.httpError(
                409, envelope(message: "This pull request has merge conflicts with the base branch.", code: "CONFLICT")
            )
        )
        XCTAssertTrue(conflict.isConflict)
        XCTAssertEqual(conflict.message, "This pull request has merge conflicts with the base branch.")

        let offline = MergeFailure(error: URLError(.notConnectedToInternet))
        XCTAssertFalse(offline.isConflict)
        XCTAssertEqual(offline.message, offlineErrorMessage)
    }
}

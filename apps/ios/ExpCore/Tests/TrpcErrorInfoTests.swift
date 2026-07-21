import Foundation
import XCTest
@testable import ExpCore

// Compliance lock (EXP-216 / App Store 3.1.1): server plan-cap messages carry
// purchase language ("Add seats or upgrade…") that must never render in the
// app — `trpcUserMessage` swaps them for the neutral copy. Ordinary errors
// keep passing the server message through verbatim.
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

    func testNonTrpcErrorFallsBackToLocalizedDescription() {
        let error = NSError(domain: "test", code: 1, userInfo: [NSLocalizedDescriptionKey: "boom"])
        XCTAssertFalse(error.isPlanLimitError)
        XCTAssertEqual(error.trpcUserMessage, "boom")
    }
}

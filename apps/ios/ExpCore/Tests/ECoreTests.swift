import Foundation
import XCTest
@testable import ExpCore

// #EXP-1504: packages/ecore/fixtures/ecore.json, replayed against the
// generated `ECore` (Rust tests/fixtures.rs, TS ecore.test.ts and Kotlin
// ECoreTest replay the same file).
final class ECoreTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/ecore/fixtures/ecore.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func cases(_ key: String) throws -> [[String: Any]] {
        try XCTUnwrap(try fixture()[key] as? [[String: Any]])
    }

    func testIds() throws {
        for c in try cases("idCases") {
            let id = try XCTUnwrap(c["id"] as? String)
            XCTAssertEqual(ECore.isValidId(id), c["valid"] as? Bool, id)
        }
    }

    func testTokensAndBearer() throws {
        for c in try cases("tokenCases") {
            let token = try XCTUnwrap(c["token"] as? String)
            XCTAssertEqual(ECore.isValidToken(token), c["valid"] as? Bool, token)
        }
        for c in try cases("bearerCases") {
            let header = try XCTUnwrap(c["header"] as? String)
            XCTAssertEqual(ECore.parseBearer(header), c["token"] as? String, header)
        }
        let b = try XCTUnwrap(try fixture()["bearerHeader"] as? [String: String])
        XCTAssertEqual(ECore.bearerHeader(b["token"]!), b["header"])
    }

    func testCapabilitiesAndGrants() throws {
        for c in try cases("capabilityCases") {
            let name = try XCTUnwrap(c["name"] as? String)
            XCTAssertEqual(ECore.Capability(rawValue: name) != nil, c["known"] as? Bool, name)
        }
        for c in try cases("grantCases") {
            let capabilities = try XCTUnwrap(c["capabilities"] as? [String]).compactMap(ECore.Capability.init(rawValue:))
            let admits = ECore.grantAdmits(
                capabilities: capabilities,
                expiresAt: (c["expiresAt"] as? NSNumber)?.int64Value,
                now: try XCTUnwrap(c["now"] as? NSNumber).int64Value,
                capability: try XCTUnwrap(ECore.Capability(rawValue: try XCTUnwrap(c["capability"] as? String)))
            )
            XCTAssertEqual(admits, c["admits"] as? Bool, "\(c)")
        }
    }

    func testVerdictsAndErrors() throws {
        let verdicts = try cases("verdictCases")
        XCTAssertEqual(verdicts.compactMap { $0["verdict"] as? String }, ECore.DeliveryVerdict.allCases.map(\.rawValue))
        for c in verdicts {
            let verdict = try XCTUnwrap(ECore.DeliveryVerdict(rawValue: try XCTUnwrap(c["verdict"] as? String)))
            XCTAssertEqual(ECore.mayRetry(verdict), c["mayRetry"] as? Bool)
        }
        let errors = try cases("errorCases")
        XCTAssertEqual(errors.compactMap { $0["code"] as? String }, ECore.ErrorCode.allCases.map(\.rawValue))
        for c in errors {
            let code = try XCTUnwrap(ECore.ErrorCode(rawValue: try XCTUnwrap(c["code"] as? String)))
            XCTAssertEqual(ECore.isRefusal(code), c["refusal"] as? Bool)
            XCTAssertEqual(ECore.defaultRetryable(code), c["retryable"] as? Bool)
        }
        let message = try XCTUnwrap(try fixture()["errorMessage"] as? [String: Any])
        for c in try XCTUnwrap(message["cases"] as? [[String: Any]]) {
            if let text = c["message"] as? String {
                XCTAssertEqual(ECore.clampMessage(text), c["clamped"] as? String)
            } else {
                let long = String(repeating: try XCTUnwrap(c["repeat"] as? String), count: try XCTUnwrap(c["times"] as? Int))
                XCTAssertEqual(ECore.clampMessage(long).unicodeScalars.count, c["clampedLength"] as? Int)
            }
        }
    }

    func testEnvelopes() throws {
        for c in try cases("envelopeCases") {
            let e = try XCTUnwrap(c["envelope"] as? [String: Any])
            let errors = ECore.envelopeErrors(
                v: try XCTUnwrap(e["v"] as? Int),
                id: try XCTUnwrap(e["id"] as? String),
                kind: try XCTUnwrap(e["kind"] as? String),
                at: try XCTUnwrap(e["at"] as? NSNumber).int64Value,
                source: try XCTUnwrap(e["source"] as? String),
                sessionId: e["sessionId"] as? String
            )
            XCTAssertEqual(errors, c["errors"] as? [String], "\(e)")
        }
    }
}

@testable import ExpCore
import Foundation
import XCTest

// #EXP-1950: packages/domain-contract/fixtures/wrap-signature.json through
// the FULL eNet binding: the vector, every `accepted`, `refused` and
// `vouchCases` case by name. The same replay runs in TS, Rust and Kotlin.
// The push-only binding exports the same functions failing closed
// (`EnetError.Crypto`, "wrap signature: unavailable"; Rust's
// tests/wrap_sig_fixture.rs holds it).
final class WrapSignatureFixtureTests: XCTestCase {
    #if ENET_NODE
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/wrap-signature.json")
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any])
    }

    private func object(_ f: [String: Any], _ key: String) throws -> [String: Any] {
        try XCTUnwrap(f[key] as? [String: Any], key)
    }

    private func cases(_ f: [String: Any], _ key: String) throws -> [[String: Any]] {
        try XCTUnwrap(f[key] as? [[String: Any]], key)
    }

    private func text(_ c: [String: Any], _ key: String) throws -> String {
        try XCTUnwrap(c[key] as? String, key)
    }

    /// One epoch above u32 stays malformed.
    private func epoch(_ c: [String: Any]) throws -> UInt32 {
        let raw = try XCTUnwrap(c["epoch"] as? NSNumber, "epoch").uint64Value
        return UInt32(clamping: raw)
    }

    private func pins(_ c: [String: Any]) throws -> [String] {
        try XCTUnwrap(c["pins"] as? [String], "pins")
    }

    private func spelled(_ reason: EnetWrapRefusal) -> String {
        switch reason {
        case .malformed: "malformed"
        case .unsigned: "unsigned"
        case .unpinned: "unpinned"
        case .unopened: "unopened"
        case .signature: "signature"
        }
    }

    private func accept(_ f: [String: Any], _ c: [String: Any]) throws -> EnetWrapVerdict {
        let recipient = try object(f, "recipient")
        return try acceptWrap(
            wrap: EnetServedWrap(
                rootId: try text(c, "rootId"),
                epoch: try epoch(c),
                wrapped: try text(c, "wrapped"),
                signature: c["signature"] as? String,
                signerEndpointId: c["signerEndpointId"] as? String
            ),
            identity: try SealedContentFixtureTests.hex(recipient["identity"]),
            installSecret: try SealedContentFixtureTests.hex(recipient["installSecret"]),
            pins: try pins(c),
            required: try XCTUnwrap(c["required"] as? Bool, "required")
        )
    }

    func testTheInstallsEndpointIdsAreTheFixtures() throws {
        let f = try fixture()
        for install in ["signer", "recipient", "other"] {
            let o = try object(f, install)
            XCTAssertEqual(try endpointIdOf(identity: try SealedContentFixtureTests.hex(o["identity"])), try text(o, "endpointId"), install)
        }
    }

    func testSignsTheVectorByteExact() throws {
        let f = try fixture()
        let v = try object(f, "vector")
        let statement = EnetWrapStatement(
            rootId: try text(v, "rootId"),
            epoch: try epoch(v),
            treePublicKey: try text(v, "treePublicKey"),
            recipientEndpointId: try text(v, "recipientEndpointId"),
            wrapped: try text(v, "wrapped")
        )
        XCTAssertEqual(try wrapSignatureMessage(statement: statement), try SealedContentFixtureTests.hex(v["message"]))
        let identity = try SealedContentFixtureTests.hex(try object(f, "signer")["identity"])
        let signature = try signWrap(identity: identity, statement: statement)
        XCTAssertEqual(signature, try text(v, "signature"))
        let signer = try text(try object(f, "signer"), "endpointId")
        XCTAssertTrue(try verifyWrapSignature(statement: statement, signature: signature, signerEndpointId: signer))
        XCTAssertFalse(try verifyWrapSignature(statement: statement, signature: signature, signerEndpointId: try text(try object(f, "other"), "endpointId")))
        var otherEpoch = statement
        otherEpoch.epoch = 2
        XCTAssertFalse(try verifyWrapSignature(statement: otherEpoch, signature: signature, signerEndpointId: signer))
        var malformed = statement
        malformed.epoch = 0
        XCTAssertThrowsError(try signWrap(identity: identity, statement: malformed)) { error in
            guard case EnetError.Invalid = error else { return XCTFail("\(error)") }
        }
    }

    func testReplaysAccepted() throws {
        let f = try fixture()
        let tree = try object(f, "tree")
        let all = try cases(f, "accepted")
        XCTAssertEqual(all.count, 5)
        for c in all {
            let name = try text(c, "name")
            let status: String, secret: Data, publicKey: String
            switch try accept(f, c) {
            case let .verified(s, p, signer):
                (status, secret, publicKey) = ("verified", s, p)
                XCTAssertEqual(signer, try text(c, "signerEndpointId"), name)
            case let .unsigned(s, p):
                (status, secret, publicKey) = ("unsigned", s, p)
            case let .refused(reason):
                XCTFail("\(name): refused \(spelled(reason))")
                continue
            }
            XCTAssertEqual(status, try text(c, "status"), name)
            XCTAssertEqual(secret, try SealedContentFixtureTests.hex(tree["secret"]), name)
            XCTAssertEqual(publicKey, try text(tree, "publicKey"), name)
        }
    }

    func testReplaysRefusedEachWithItsReason() throws {
        let f = try fixture()
        let all = try cases(f, "refused")
        XCTAssertEqual(all.count, 20)
        var seen = Set<String>()
        for c in all {
            let name = try text(c, "name")
            guard case let .refused(reason) = try accept(f, c) else {
                XCTFail("\(name): kept")
                continue
            }
            XCTAssertEqual(spelled(reason), try text(c, "reason"), name)
            seen.insert(spelled(reason))
        }
        XCTAssertEqual(seen, Set(try XCTUnwrap(f["refusals"] as? [String])))
    }

    func testReplaysTheVouchCases() throws {
        let f = try fixture()
        let all = try cases(f, "vouchCases")
        XCTAssertEqual(all.count, 6)
        for c in all {
            let got = try acceptVouch(
                statement: EnetWrapStatement(
                    rootId: try text(c, "rootId"),
                    epoch: try epoch(c),
                    treePublicKey: try text(c, "treePublicKey"),
                    recipientEndpointId: try text(c, "recipientEndpointId"),
                    wrapped: try text(c, "wrapped")
                ),
                signature: c["signature"] as? String,
                signerEndpointId: c["signerEndpointId"] as? String,
                pins: try pins(c)
            )
            let result: String
            switch got {
            case .verified: result = "verified"
            case let .refused(reason): result = spelled(reason)
            }
            let name = try text(c, "name")
            XCTAssertEqual(result, try text(c, "result"), name)
        }
    }
    #else
    func testTheBindingIsBuilt() throws {
        throw XCTSkip("the full ExponentialNetFFI.xcframework was not built (build-ios.sh)")
    }
    #endif
}

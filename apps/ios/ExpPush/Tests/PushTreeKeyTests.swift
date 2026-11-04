import ExpPush
import Foundation
import XCTest

// EXP-1635: the sealed-content writes the share extension reaches through the
// push binding: a tree secret wrapped for an install opens with its key, and
// a body seals into the envelope the server takes.
final class PushTreeKeyTests: XCTestCase {
    func testAWrappedTreeSecretOpensAndABodySeals() throws {
        guard PushCrypto.isAvailable else { throw XCTSkip("the push binding was not built (build-ios.sh)") }
        let install = try XCTUnwrap(PushCrypto.deriveKeypair(identity: Data(repeating: 7, count: 32)))
        let tree = try XCTUnwrap(PushCrypto.generateTree())
        XCTAssertEqual(tree.secret.count, 32)
        let wrapped = try PushCrypto.wrapTree(installPublic: install.publicKeyBase64Url, treeSecret: tree.secret)
        XCTAssertEqual(try PushCrypto.unwrapTree(installSecret: install.secret, wrapped: wrapped), tree.secret)
        let body = try PushCrypto.sealTreeBody(treePublicKey: tree.publicKey, epoch: 1, text: "hello")
        XCTAssertTrue(body.hasPrefix("exp-sealed:"))
        XCTAssertFalse(body.contains("hello"))
    }
}

import ExpPush
import Foundation
import XCTest

// EXP-1435: an opened sealed push becomes the content a plaintext FCM push
// shows: title/body displayed, `data` merged into `userInfo` top-level (so
// NotificationRouting reads the same keys), the sealed blob dropped.
final class SealedPushMappingTests: XCTestCase {
    private let placeholder: [AnyHashable: Any] = [
        "aps": ["alert": ["title": "New message"], "mutable-content": 1],
        "s": "AQ",
    ]

    func testDataMergesIntoUserInfoLikeFcm() throws {
        let plaintext = Data(#"{"title":"APP-12 Fix login","body":"Anna commented","data":{"userId":"u1","notificationId":"n1","type":"issue_comment","issueId":"i1"}}"#.utf8)
        let opened = try XCTUnwrap(SealedPush.map(plaintext: plaintext, userInfo: placeholder))
        XCTAssertEqual(opened.title, "APP-12 Fix login")
        XCTAssertEqual(opened.body, "Anna commented")
        XCTAssertEqual(opened.userInfo["userId"] as? String, "u1")
        XCTAssertEqual(opened.userInfo["notificationId"] as? String, "n1")
        XCTAssertEqual(opened.userInfo["type"] as? String, "issue_comment")
        XCTAssertEqual(opened.userInfo["issueId"] as? String, "i1")
        // The /send shim copied the display copy into the data keys too.
        XCTAssertEqual(opened.userInfo["title"] as? String, "APP-12 Fix login")
        XCTAssertEqual(opened.userInfo["body"] as? String, "Anna commented")
        XCTAssertNil(opened.userInfo["s"])
        XCTAssertNotNil(opened.userInfo["aps"])
    }

    func testCallerDataWinsOverTheDisplayCopy() throws {
        let plaintext = Data(#"{"title":"T","body":"B","data":{"title":"explicit"}}"#.utf8)
        let opened = try XCTUnwrap(SealedPush.map(plaintext: plaintext, userInfo: [:]))
        XCTAssertEqual(opened.userInfo["title"] as? String, "explicit")
        XCTAssertEqual(opened.title, "T")
    }

    func testBodylessAndDegradedPayloads() throws {
        let opened = try XCTUnwrap(SealedPush.map(plaintext: Data(#"{"title":"","data":{"userId":"u1","sessionId":"s1","type":"session_blocked"}}"#.utf8), userInfo: [:]))
        XCTAssertEqual(opened.title, "")
        XCTAssertNil(opened.body)
        XCTAssertEqual(opened.userInfo["sessionId"] as? String, "s1")
    }

    func testNonContractPlaintextIsRefused() {
        XCTAssertNil(SealedPush.map(plaintext: Data("from rust".utf8), userInfo: [:]))
        XCTAssertNil(SealedPush.map(plaintext: Data(#"{"body":"no title"}"#.utf8), userInfo: [:]))
        XCTAssertNil(SealedPush.map(plaintext: Data(#"["title"]"#.utf8), userInfo: [:]))
    }

    func testNoSealedKeyOpensNothing() {
        XCTAssertNil(SealedPush.open(userInfo: ["aps": [:]], secret: Data(repeating: 1, count: 32)))
        XCTAssertNil(SealedPush.open(userInfo: ["s": "not base64url!"], secret: Data(repeating: 1, count: 32)))
    }

    func testBase64UrlRoundTripsWithoutPadding() {
        let data = Data((0..<33).map { UInt8($0 &* 7) })
        let encoded = Base64Url.encode(data)
        XCTAssertFalse(encoded.contains("="))
        XCTAssertFalse(encoded.contains("+"))
        XCTAssertFalse(encoded.contains("/"))
        XCTAssertEqual(Base64Url.decode(encoded), data)
        XCTAssertNil(Base64Url.decode("A"))
    }
}

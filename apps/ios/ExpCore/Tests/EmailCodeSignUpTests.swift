import Foundation
import XCTest
@testable import ExpCore

// EXP-1026: the email-code sign-up name step. The copy is byte-identical with
// Android `EmailCodeSignUpCopy` (its `EmailCodeSignUpTest` locks the same
// literals); the wire contract is the one the server pins: `X-Exp-Ask-Name: 1`
// on every verify, 400 `NAME_REQUIRED` for an unknown address sent without a
// name, then `{email, otp, name}`.
final class EmailCodeSignUpTests: XCTestCase {
    func testCopyLiterals() {
        XCTAssertEqual(EmailCodeSignUpCopy.title, "What should we call you?")
        XCTAssertEqual(EmailCodeSignUpCopy.body, "This is how your teammates see you.")
        XCTAssertEqual(EmailCodeSignUpCopy.fieldLabel, "Name")
        XCTAssertEqual(EmailCodeSignUpCopy.placeholder, "Your name")
        XCTAssertEqual(EmailCodeSignUpCopy.button, "Create account")
        XCTAssertEqual(EmailCodeSignUpCopy.emptyNameError, "Enter your name to continue.")
    }

    func testAskNameHeader() {
        XCTAssertEqual(EmailCodeSignUp.askNameHeader, "X-Exp-Ask-Name")
        XCTAssertEqual(EmailCodeSignUp.askNameValue, "1")
    }

    func testRequestBodyCarriesTheNameOnlyOnceGiven() {
        XCTAssertEqual(
            EmailCodeSignUp.requestBody(email: "a@b.c", otp: "123456", name: nil),
            ["email": "a@b.c", "otp": "123456"]
        )
        XCTAssertEqual(
            EmailCodeSignUp.requestBody(email: "a@b.c", otp: "123456", name: "   "),
            ["email": "a@b.c", "otp": "123456"]
        )
        XCTAssertEqual(
            EmailCodeSignUp.requestBody(email: "a@b.c", otp: "123456", name: "  Ada Lovelace "),
            ["email": "a@b.c", "otp": "123456", "name": "Ada Lovelace"]
        )
    }

    func testNameRequiredIsOnlyThe400Code() {
        XCTAssertTrue(EmailCodeSignUp.isNameRequired(status: 400, code: "NAME_REQUIRED"))
        XCTAssertFalse(EmailCodeSignUp.isNameRequired(status: 400, code: "INVALID_OTP"))
        XCTAssertFalse(EmailCodeSignUp.isNameRequired(status: 401, code: "NAME_REQUIRED"))
        XCTAssertFalse(EmailCodeSignUp.isNameRequired(status: 400, code: nil))
    }
}

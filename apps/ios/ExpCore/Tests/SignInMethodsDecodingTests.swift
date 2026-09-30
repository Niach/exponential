import Foundation
import XCTest
@testable import ExpCore

// EXP-1126: `users.signInMethods` decodes permissively and the unlink rule
// matches the server's (`linked && waysIn > 1`).
final class SignInMethodsDecodingTests: XCTestCase {
    private let payload = """
    {
      "email": "a@x.com",
      "emailVerified": true,
      "emailOtpEnabled": true,
      "passwordEnabled": true,
      "passkeyEnabled": true,
      "providers": [
        {"id": "apple", "name": "Apple", "kind": "apple", "available": true, "linked": false, "linkedAt": null},
        {"id": "google", "name": "Google", "kind": "google", "available": true, "linked": true,
         "linkedAt": "2026-09-01T10:00:00.000Z"},
        {"id": "credential", "name": "Password", "kind": "password", "available": true, "linked": true,
         "linkedAt": "2026-08-01T10:00:00.000Z"},
        {"id": "old-sso", "name": "old-sso", "kind": "oidc", "available": false, "linked": true, "linkedAt": null}
      ],
      "passkeys": [
        {"id": "pk1", "name": null, "createdAt": "2026-09-02T10:00:00.000Z", "backedUp": true},
        {"id": "pk2", "name": "Work laptop", "createdAt": null, "backedUp": false}
      ],
      "waysIn": 4
    }
    """

    func testDecodesThePayload() throws {
        let methods = try JSONDecoder().decode(SignInMethods.self, from: Data(payload.utf8))
        XCTAssertEqual(methods.email, "a@x.com")
        XCTAssertTrue(methods.emailOtpEnabled)
        XCTAssertEqual(methods.providers.map(\.id), ["apple", "google", "credential", "old-sso"])
        XCTAssertEqual(methods.providers[1].linkedAt, "2026-09-01T10:00:00.000Z")
        XCTAssertTrue(methods.providers[2].isPassword)
        XCTAssertFalse(methods.providers[3].available)
        XCTAssertEqual(methods.passkeys.count, 2)
        XCTAssertEqual(methods.passkeys[0].displayName, "Passkey")
        XCTAssertTrue(methods.passkeys[0].backedUp)
        XCTAssertEqual(methods.passkeys[1].displayName, "Work laptop")
        XCTAssertEqual(methods.waysIn, 4)
    }

    func testMissingOptionalFieldsDefault() throws {
        let methods = try JSONDecoder().decode(
            SignInMethods.self,
            from: Data(#"{"email":"a@x.com","providers":[{"id":"google"}]}"#.utf8)
        )
        XCTAssertFalse(methods.emailOtpEnabled)
        XCTAssertEqual(methods.providers.first?.name, "google")
        XCTAssertEqual(methods.providers.first?.linked, false)
        XCTAssertEqual(methods.passkeys, [])
        XCTAssertEqual(methods.waysIn, 0)
    }

    func testCanUnlink() throws {
        let methods = try JSONDecoder().decode(SignInMethods.self, from: Data(payload.utf8))
        XCTAssertFalse(SignInMethods.canUnlink(methods.providers[0], in: methods), "not linked")
        XCTAssertTrue(SignInMethods.canUnlink(methods.providers[1], in: methods))
        XCTAssertTrue(methods.canRemovePasskey)

        let onlyWay = SignInMethods(
            email: "a@x.com", emailVerified: true, emailOtpEnabled: false, passwordEnabled: false,
            passkeyEnabled: false, providers: [methods.providers[1]], passkeys: [], waysIn: 1
        )
        XCTAssertFalse(SignInMethods.canUnlink(onlyWay.providers[0], in: onlyWay), "last way in")
        XCTAssertFalse(onlyWay.canRemovePasskey)
    }

    func testLinkTicketDecodes() throws {
        let ticket = try JSONDecoder().decode(
            SignInLinkTicket.self, from: Data(#"{"ticket":"abc","expiresInSeconds":120}"#.utf8)
        )
        XCTAssertEqual(ticket.ticket, "abc")
        XCTAssertEqual(ticket.expiresInSeconds, 120)
    }
}

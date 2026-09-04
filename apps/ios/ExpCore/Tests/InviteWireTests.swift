import XCTest
@testable import ExpCore

// EXP-725: the invite-link creator's two wire calls. `teamInvites.create` is a
// POST whose body the app builds by hand (the role is sent explicitly, never
// left to the server default), and `teams.inviteCapacity` answers with a
// NULLABLE seat count — `null` means unlimited and must decode as nil, not as
// zero, or the control would vanish on every self-hosted instance.
final class InviteWireTests: XCTestCase {
    private func json(_ value: some Encodable) throws -> [String: Any] {
        let data = try JSONEncoder().encode(value)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    // MARK: - teamInvites.create

    func testCreateInviteSendsTheTeamAndTheMemberRole() throws {
        let object = try json(CreateInviteInput(teamId: "team-1"))
        XCTAssertEqual(object["teamId"] as? String, "team-1")
        // The app never offers the owner role from the invite creator.
        XCTAssertEqual(object["role"] as? String, "member")
        // Link-only: emailed invites stay a web surface.
        XCTAssertNil(object.index(forKey: "email"))
    }

    func testCreateInviteDecodesOnlyTheToken() throws {
        // The server also returns the invite row; the app reads the token
        // alone (the shape's column allowlist drops it, so this is the only
        // place it ever appears).
        let body = """
        {"invite":{"id":"inv-1","teamId":"team-1","role":"member"},"token":"tok_abc","emailDelivered":null}
        """
        let result = try JSONDecoder().decode(
            CreateInviteResult.self, from: XCTUnwrap(body.data(using: .utf8))
        )
        XCTAssertEqual(result.token, "tok_abc")
    }

    // MARK: - teams.inviteCapacity

    func testInviteCapacityInputCarriesTheTeam() throws {
        let object = try json(TeamIdInput(teamId: "team-1"))
        XCTAssertEqual(object["teamId"] as? String, "team-1")
    }

    func testInviteCapacityDecodesARemainingCount() throws {
        let body = #"{"remaining":2}"#
        let result = try JSONDecoder().decode(
            InviteCapacityResult.self, from: XCTUnwrap(body.data(using: .utf8))
        )
        XCTAssertEqual(result.remaining, 2)
    }

    func testInviteCapacityDecodesNullAsUnlimited() throws {
        let body = #"{"remaining":null}"#
        let result = try JSONDecoder().decode(
            InviteCapacityResult.self, from: XCTUnwrap(body.data(using: .utf8))
        )
        XCTAssertNil(result.remaining)
    }

    func testInviteCapacityDecodesZero() throws {
        // Zero is the ONE value that removes the control entirely — it must
        // never round-trip as nil.
        let body = #"{"remaining":0}"#
        let result = try JSONDecoder().decode(
            InviteCapacityResult.self, from: XCTUnwrap(body.data(using: .utf8))
        )
        XCTAssertEqual(result.remaining, 0)
    }
}

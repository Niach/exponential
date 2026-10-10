import Foundation
import XCTest
@testable import ExpCore

// The settings rules ×4: members in join order, the pending-invite rows'
// filter + "Expires Mon D" caption, and the Email code row's off subtitle.
final class TeamSettingsRulesTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_790_244_000)   // 2026-09-24T10:00:00Z

    private func member(_ id: String, createdAt: String) -> TeamMemberEntity {
        TeamMemberEntity(
            id: id, teamId: "team-1", userId: "u-\(id)", role: "member",
            createdAt: createdAt, updatedAt: createdAt
        )
    }

    private func invite(
        _ id: String,
        expiresAt: String,
        acceptedAt: String? = nil,
        email: String? = nil
    ) -> TeamInviteEntity {
        TeamInviteEntity(
            id: id, teamId: "team-1", role: "member", token: nil, email: email,
            expiresAt: expiresAt, acceptedAt: acceptedAt,
            createdAt: "2026-09-19T10:00:00Z", updatedAt: "2026-09-19T10:00:00Z"
        )
    }

    func testMembersSortOldestFirstWithIdTieBreak() {
        let sorted = TeamMemberOrder.joinOrder([
            member("c", createdAt: "2026-09-03T10:00:00Z"),
            member("b", createdAt: "2026-09-01T10:00:00Z"),
            member("a", createdAt: "2026-09-01T10:00:00Z"),
            member("z", createdAt: "garbage"),
            member("d", createdAt: "2026-08-30 08:00:00+00"),
        ])
        XCTAssertEqual(sorted.map(\.id), ["d", "a", "b", "c", "z"])
    }

    func testPendingKeepsOnlyUnacceptedUnexpiredInvites() {
        let pending = PendingInvites.pending([
            invite("live", expiresAt: "2026-10-01T10:00:00Z"),
            invite("expired", expiresAt: "2026-09-20T10:00:00Z"),
            invite("accepted", expiresAt: "2026-10-01T10:00:00Z", acceptedAt: "2026-09-21T10:00:00Z"),
            invite("bad", expiresAt: ""),
        ], now: now)
        XCTAssertEqual(pending.map(\.id), ["live"])
    }

    func testExpiryLabelIsShortMonthDay() {
        let utc = TimeZone(identifier: "UTC")!
        XCTAssertEqual(PendingInvites.expiryLabel("2026-10-09T10:00:00Z", timeZone: utc), "Expires Oct 9")
        XCTAssertNil(PendingInvites.expiryLabel("", timeZone: utc))
        XCTAssertEqual(PendingInvites.linkInviteLabel, "Link invite")
    }

    func testEmailCodeSubtitle() {
        func methods(otp: Bool) -> SignInMethods {
            SignInMethods(
                email: "alex@acme.dev", emailVerified: true, emailOtpEnabled: otp,
                passwordEnabled: true, passkeyEnabled: false, providers: [], passkeys: [], waysIn: 1
            )
        }
        XCTAssertEqual(methods(otp: true).emailCodeSubtitle, "alex@acme.dev")
        XCTAssertEqual(methods(otp: false).emailCodeSubtitle, "alex@acme.dev · Off on this server")
    }
}

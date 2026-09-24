import Foundation
import XCTest
@testable import ExpCore

// EXP-630: what the Members list reads off the synced invites — a member is
// "invited, not joined" while an invite bound to it is unaccepted. Same cases
// as web's `placeholder-status.test.ts`.
final class PlaceholderStatusTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_790_244_000)   // 2026-09-24T10:00:00Z
    private let later = "2026-10-01T10:00:00Z"
    private let earlier = "2026-09-20T10:00:00Z"

    private func invite(
        _ id: String,
        placeholderUserId: String?,
        acceptedAt: String?,
        expiresAt: String
    ) -> TeamInviteEntity {
        TeamInviteEntity(
            id: id,
            teamId: "team-1",
            role: "member",
            token: nil,
            email: "invitee@example.com",
            placeholderUserId: placeholderUserId,
            expiresAt: expiresAt,
            acceptedAt: acceptedAt,
            createdAt: "2026-09-19T10:00:00Z",
            updatedAt: "2026-09-19T10:00:00Z"
        )
    }

    func testBadgesAPendingInviteAnExpiredOneAndNothingOnceAccepted() {
        let statuses = placeholderStatuses(
            invites: [
                invite("i1", placeholderUserId: "p-pending", acceptedAt: nil, expiresAt: later),
                invite("i2", placeholderUserId: "p-expired", acceptedAt: nil, expiresAt: earlier),
                invite("i3", placeholderUserId: "p-joined", acceptedAt: earlier, expiresAt: later),
                // A link invite (no placeholder member) never badges anyone.
                invite("i4", placeholderUserId: nil, acceptedAt: nil, expiresAt: later),
            ],
            now: now
        )
        XCTAssertEqual(statuses, ["p-pending": .pending, "p-expired": .expired])
    }

    func testLetsAFreshLinkOutrankTheExpiredOneItSupersededInEitherOrder() {
        let rows = [
            invite("i1", placeholderUserId: "p", acceptedAt: nil, expiresAt: earlier),
            invite("i2", placeholderUserId: "p", acceptedAt: nil, expiresAt: later),
        ]
        XCTAssertEqual(placeholderStatuses(invites: rows, now: now)["p"], .pending)
        XCTAssertEqual(placeholderStatuses(invites: rows.reversed(), now: now)["p"], .pending)
    }

    func testLabelsBothStates() {
        XCTAssertEqual(PlaceholderStatus.pending.label, "Invited")
        XCTAssertEqual(PlaceholderStatus.expired.label, "Invite expired")
    }

    // Electric delivers timestamps in the Postgres text form, so the badge has
    // to read `2026-10-01 10:00:00+00` exactly like the ISO one (WireTimestamps);
    // an unparseable value fails closed to `expired`.
    func testReadsThePostgresWireFormAndFailsClosedOnGarbage() {
        XCTAssertEqual(
            placeholderStatuses(
                invites: [invite("i1", placeholderUserId: "p", acceptedAt: nil,
                                 expiresAt: "2026-10-01 10:00:00.123456+00")],
                now: now
            )["p"],
            .pending
        )
        XCTAssertEqual(
            placeholderStatuses(
                invites: [invite("i1", placeholderUserId: "p", acceptedAt: nil, expiresAt: "")],
                now: now
            )["p"],
            .expired
        )
    }
}

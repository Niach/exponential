import Foundation
import XCTest
@testable import ExpCore

// EXP-630: what the Members list reads off the synced invites — a member is
// "invited, not joined" while an invite bound to it is unaccepted. EXP-1076:
// an imported roster row nobody was ever sent a link for reads "Not invited",
// never "Invite expired". Same cases as web's `placeholder-status.test.ts`.
final class PlaceholderStatusTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_790_244_000)   // 2026-09-24T10:00:00Z
    private let later = "2026-10-01T10:00:00Z"
    private let earlier = "2026-09-20T10:00:00Z"

    private func invite(
        _ id: String,
        placeholderUserId: String?,
        acceptedAt: String?,
        expiresAt: String,
        sentAt: String? = "2026-09-19T10:00:00Z"
    ) -> TeamInviteEntity {
        TeamInviteEntity(
            id: id,
            teamId: "team-1",
            role: "member",
            token: nil,
            email: "invitee@example.com",
            placeholderUserId: placeholderUserId,
            sentAt: sentAt,
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

    // EXP-1076: the import stamps expires_at = created_at, so the row is
    // "expired" by date from the moment it exists — the label must not say so.
    // An empty string (a healed pre-v54 column) reads the same as nil.
    func testReadsANeverSentRowAsUnsentWhateverItsExpirySays() {
        let statuses = placeholderStatuses(
            invites: [
                invite("i1", placeholderUserId: "p-import", acceptedAt: nil, expiresAt: earlier, sentAt: nil),
                invite("i2", placeholderUserId: "p-future", acceptedAt: nil, expiresAt: later, sentAt: nil),
                invite("i3", placeholderUserId: "p-healed", acceptedAt: nil, expiresAt: later, sentAt: ""),
            ],
            now: now
        )
        XCTAssertEqual(statuses, ["p-import": .unsent, "p-future": .unsent, "p-healed": .unsent])
    }

    func testRanksPendingOverExpiredOverUnsentInEitherOrder() {
        let unsent = invite("i-u", placeholderUserId: "p", acceptedAt: nil, expiresAt: earlier, sentAt: nil)
        let expired = invite("i-e", placeholderUserId: "p", acceptedAt: nil, expiresAt: earlier)
        let pending = invite("i-p", placeholderUserId: "p", acceptedAt: nil, expiresAt: later)
        XCTAssertEqual(placeholderStatuses(invites: [unsent, expired], now: now)["p"], .expired)
        XCTAssertEqual(placeholderStatuses(invites: [expired, unsent], now: now)["p"], .expired)
        XCTAssertEqual(placeholderStatuses(invites: [unsent, pending], now: now)["p"], .pending)
        XCTAssertEqual(placeholderStatuses(invites: [pending, unsent], now: now)["p"], .pending)
        XCTAssertEqual(placeholderStatuses(invites: [pending, expired, unsent], now: now)["p"], .pending)
    }

    func testLabelsAllThreeStates() {
        XCTAssertEqual(PlaceholderStatus.unsent.label, "Not invited")
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

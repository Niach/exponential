import Foundation
import XCTest
@testable import ExpCore

/// EXP-1373: the phone inbox's Mark all read clears the ACTIVE team only —
/// `notifications.markAllRead({ teamId })`.
final class NotificationsApiWireTests: XCTestCase {
    func testMarkAllReadInputCarriesTheTeam() throws {
        let data = try JSONEncoder().encode(MarkAllReadInput(teamId: "t1"))
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        XCTAssertEqual(object as NSDictionary, ["teamId": "t1"] as NSDictionary)
    }
}

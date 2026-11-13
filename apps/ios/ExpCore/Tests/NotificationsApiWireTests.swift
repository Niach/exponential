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

    /// #EXP-1815: the source switches go out WHOLE, absent keys omitted
    /// (absent = on); `emailPrefs` decodes them.
    func testSourcePrefsWire() throws {
        let input = UpdateSourcePrefsInput(sourcePrefs: NotificationSourcePrefs(assistant: false, agents: ["a2": false]))
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: try JSONEncoder().encode(input)) as? [String: Any])
        XCTAssertEqual(object as NSDictionary, ["sourcePrefs": ["assistant": false, "agents": ["a2": false]]] as NSDictionary)
        let decoded = try JSONDecoder().decode(
            NotificationEmailPrefs.self,
            from: Data(#"{"emailEnabled":true,"sourcePrefs":{"temporary":false},"transportConfigured":true}"#.utf8)
        )
        XCTAssertEqual(decoded.sourcePrefs, NotificationSourcePrefs(temporary: false))
    }
}

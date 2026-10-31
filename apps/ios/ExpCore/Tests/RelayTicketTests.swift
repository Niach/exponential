@testable import ExpCore
import XCTest

// EXP-1471: the same vector as apps/desktop/crates/enet-core/tests/net.rs
// `relay_ticket_vector` (identity [7; 32] on the production relay).
final class RelayTicketTests: XCTestCase {
    private let id = "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c"
    private let ticket = "enet15jfgyy7ctrjavpxvkb5rglwf7gkuo5vox27hxescd3vgsfcg2iwachlior2ha4z2f4xxezlmmf4s4zlyobxw4zlooruwc3bomf2c6aa"

    func testEncodesTheEnetCoreTicketOfARelayHomedPeer() {
        XCTAssertEqual(RelayTicket.encode(endpointId: id), ticket)
        XCTAssertEqual(RelayTicket.encode(endpointId: id, relayUrl: "https://relay.exponential.at/"), ticket)
        XCTAssertEqual(RelayTicket.encode(endpointId: id.uppercased()), ticket)
    }

    func testRefusesAMalformedEndpointId() {
        XCTAssertNil(RelayTicket.encode(endpointId: "a1"))
    }
}

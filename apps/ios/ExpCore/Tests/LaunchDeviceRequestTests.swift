import Foundation
import XCTest

@testable import ExpCore

// EXP-836: the composer's ▶ machine REQUEST explains itself instead of falling
// back in silence. Byte-locked to web's `deviceRequestNote`
// (`hooks/use-launch-composer.ts`), which these strings are copied from.
final class LaunchDeviceRequestTests: XCTestCase {

    private func device(
        _ id: String,
        label: String? = nil,
        agents: [String]? = ["claude"],
        online: Bool = true
    ) -> SteerDevice {
        SteerDevice(
            deviceId: id,
            deviceLabel: label ?? id,
            agents: agents,
            online: online
        )
    }

    func testASettledRequestSaysNothing() {
        let mint = device("mint")
        let mac = device("mac", label: "macbook")
        // The requested machine IS the resolved one.
        XCTAssertNil(LaunchDeviceRequest.note(
            requested: "mint", resolved: "mint", devices: [mac, mint]
        ))
        // No request at all.
        XCTAssertNil(LaunchDeviceRequest.note(
            requested: nil, resolved: "mac", devices: [mac, mint]
        ))
        XCTAssertNil(LaunchDeviceRequest.note(
            requested: "", resolved: "mac", devices: [mac, mint]
        ))
    }

    func testAPendingPoolIsNotAMissingMachine() {
        // The devices shape has not synced: the request still outranks the
        // default, so there is nothing to accuse it of yet.
        XCTAssertNil(LaunchDeviceRequest.note(
            requested: "mint", resolved: nil, devices: nil
        ))
        // It synced, and the machine is not in it.
        XCTAssertEqual(
            LaunchDeviceRequest.note(
                requested: "mint", resolved: "mac", devices: [device("mac")]
            ),
            "That machine is no longer in your registry."
        )
    }

    func testAnUnusableMachineNamesItsReason() {
        let mac = device("mac")
        XCTAssertEqual(
            LaunchDeviceRequest.note(
                requested: "mint",
                resolved: "mac",
                devices: [mac, device("mint", label: "mint", online: false)]
            ),
            "mint is offline."
        )
        // EXP-409: online, every installed agent signed out there.
        XCTAssertEqual(
            LaunchDeviceRequest.note(
                requested: "mint",
                resolved: "mac",
                devices: [mac, device("mint", label: "mint", agents: [])]
            ),
            "No agent is signed in on mint."
        )
        // Offline wins over the sign-in reason — it is the nearer fact.
        XCTAssertEqual(
            LaunchDeviceRequest.note(
                requested: "mint",
                resolved: "mac",
                devices: [mac, device("mint", label: "mint", agents: [], online: false)]
            ),
            "mint is offline."
        )
        // A label-less row falls back to the id, like every other sentence
        // about a machine does.
        XCTAssertEqual(
            LaunchDeviceRequest.note(
                requested: "mint",
                resolved: "mac",
                devices: [mac, device("mint", label: "", online: false)]
            ),
            "mint is offline."
        )
    }

    func testAStartableRequestedMachineCarriesNoReason() {
        // Registered, online, agents signed in — whatever kept it from being
        // the resolved machine is not this note's business.
        XCTAssertNil(LaunchDeviceRequest.note(
            requested: "mint", resolved: "mac", devices: [device("mac"), device("mint")]
        ))
    }
}

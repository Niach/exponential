import Foundation
import XCTest
import ExpCore
import ExpUI

// EXP-924: the ONE device glyph resolver. A pick wins, anything else falls
// back to the KIND default — the fallback is what every machine drew before
// the column existed, so an unpicked row must keep drawing exactly that.
final class DeviceIconDisplayTests: XCTestCase {

    func testAPickFromTheDeviceSetWins() {
        XCTAssertEqual(
            DeviceIconDisplay.iconName(icon: "laptop", kind: "desktop"), "laptop"
        )
        // The OS marks are pickable for either kind — a server may wear one.
        XCTAssertEqual(
            DeviceIconDisplay.iconName(icon: "os-linux", kind: "server"), "os-linux"
        )
        // And a pick may deliberately contradict the kind.
        XCTAssertEqual(
            DeviceIconDisplay.iconName(icon: "server", kind: "desktop"), "server"
        )
    }

    func testNoPickFallsBackToTheKindDefault() {
        XCTAssertEqual(
            DeviceIconDisplay.iconName(icon: nil, kind: "server"), AppIcons.uiServer
        )
        XCTAssertEqual(
            DeviceIconDisplay.iconName(icon: nil, kind: "desktop"), AppIcons.uiDevice
        )
        // A relay-only row carries no kind at all: a desktop by construction.
        XCTAssertEqual(
            DeviceIconDisplay.iconName(icon: nil, kind: nil), AppIcons.uiDevice
        )
        // An empty string is not a pick either.
        XCTAssertEqual(
            DeviceIconDisplay.iconName(icon: "", kind: "server"), AppIcons.uiServer
        )
    }

    // A name outside the DEVICE set — a board-set glyph, or one a newer client
    // knows and this build does not — is not drawable here: fall back rather
    // than render something arbitrary next to the machines that didn't pick.
    func testANameOutsideTheDeviceSetFallsBack() {
        XCTAssertEqual(
            DeviceIconDisplay.iconName(icon: "rocket", kind: "desktop"), AppIcons.uiDevice
        )
        XCTAssertEqual(
            DeviceIconDisplay.iconName(icon: "square-kanban", kind: "server"), AppIcons.uiServer
        )
        XCTAssertFalse(AppIcons.devicePickable.contains("rocket"))
    }

    // The two kind defaults are themselves in the offered set, so a fresh
    // machine's resolved glyph shows as SELECTED in the picker (no blank).
    func testTheKindDefaultsAreThemselvesPickable() {
        XCTAssertTrue(AppIcons.devicePickable.contains(AppIcons.uiDevice))
        XCTAssertTrue(AppIcons.devicePickable.contains(AppIcons.uiServer))
        XCTAssertEqual(AppIcons.devicePickable.first, AppIcons.uiDevice)
    }

    // Both row shapes resolve identically — the picker reads a `SteerDevice`,
    // a list may hold the synced entity.
    func testResolvesFromEitherRowShape() {
        let device = SteerDevice(
            deviceId: "dev-1", deviceLabel: "buildbox", kind: "server", icon: "os-apple"
        )
        XCTAssertEqual(DeviceIconDisplay.iconName(for: device), "os-apple")
        let unpicked = SteerDevice(deviceId: "dev-2", deviceLabel: "box", kind: "server")
        XCTAssertEqual(DeviceIconDisplay.iconName(for: unpicked), AppIcons.uiServer)

        let entity = DeviceEntity(
            id: "row-1", userId: "u1", deviceId: "dev-1", label: "buildbox",
            kind: "server", icon: "os-windows"
        )
        XCTAssertEqual(DeviceIconDisplay.iconName(for: entity), "os-windows")
    }

    // Every offered name has to ship as an asset, or the picker draws holes.
    func testEveryDeviceIconShipsAsAnAsset() {
        for name in AppIcons.devicePickable {
            XCTAssertTrue(AppIcons.allNames.contains(name), "missing asset for \(name)")
        }
        XCTAssertEqual(AppIcons.devicePickable, DomainContract.deviceIconValues)
    }
}

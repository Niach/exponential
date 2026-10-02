import Foundation
import XCTest
@testable import ExpCore

// EXP-1169: the device-setup server card's rules, byte-for-byte with web's
// `components/device-setup.tsx` and `lib/auth/device-code.ts`.
final class DeviceSetupRulesTests: XCTestCase {
    private let origin = "https://app.exponential.at"

    func testTheCopiedCommandIsOneLineWithTheToken() {
        XCTAssertEqual(
            ServerInstallCommand.copied(origin: origin, token: "expi_abc"),
            "curl -fsSL https://exponential.at/install.sh | EXP_INSTANCE=https://app.exponential.at EXP_INSTALL_TOKEN=expi_abc sh"
        )
    }

    func testTheCopiedCommandWithoutATokenIsThePlainOneLiner() {
        XCTAssertEqual(
            ServerInstallCommand.copied(origin: origin, token: nil),
            "curl -fsSL https://exponential.at/install.sh | EXP_INSTANCE=https://app.exponential.at sh"
        )
    }

    func testTheDisplayedCommandWithATokenIsThreeLines() {
        XCTAssertEqual(
            ServerInstallCommand.displayed(origin: origin, token: "expi_abc"),
            "curl -fsSL https://exponential.at/install.sh |\n  EXP_INSTANCE=https://app.exponential.at \\\n  EXP_INSTALL_TOKEN=expi_abc sh"
        )
    }

    func testTheDisplayedCommandWithoutATokenIsTwoLines() {
        XCTAssertEqual(
            ServerInstallCommand.displayed(origin: origin, token: nil),
            "curl -fsSL https://exponential.at/install.sh |\n  EXP_INSTANCE=https://app.exponential.at sh"
        )
    }

    func testNormalizeUserCodeVectors() {
        XCTAssertEqual(DeviceUserCode.normalize("zp3hv7hk"), "ZP3H-V7HK")
        XCTAssertEqual(DeviceUserCode.normalize("zp3h"), "ZP3H")
        XCTAssertEqual(DeviceUserCode.normalize("ZP3H-"), "ZP3H")
        XCTAssertEqual(DeviceUserCode.normalize("zp3hv"), "ZP3H-V")
        XCTAssertEqual(DeviceUserCode.normalize("ZP3H-V7HK99"), "ZP3H-V7HK")
        XCTAssertEqual(DeviceUserCode.normalize(" zp 3h_v7.hk "), "ZP3H-V7HK")
        XCTAssertEqual(DeviceUserCode.normalize(""), "")
    }

    func testACompleteCodeIsEightCharactersWithoutTheDash() {
        XCTAssertTrue(DeviceUserCode.isComplete("ZP3H-V7HK"))
        XCTAssertTrue(DeviceUserCode.isComplete("ZP3HV7HK"))
        XCTAssertFalse(DeviceUserCode.isComplete("ZP3H-V7H"))
        XCTAssertFalse(DeviceUserCode.isComplete("ZP3H"))
    }

    func testDeviceErrorCodesMapToTheCardsCopy() {
        XCTAssertEqual(DeviceCodeError.from(errorCode: "expired_token"), .expired)
        XCTAssertEqual(DeviceCodeError.from(errorCode: "invalid_request"), .invalid)
        XCTAssertEqual(DeviceCodeError.from(errorCode: "invalid_grant"), .invalid)
        XCTAssertEqual(DeviceCodeError.from(errorCode: "access_denied"), .otherAccount)
        XCTAssertEqual(DeviceCodeError.from(errorCode: "slow_down"), .failed)
        XCTAssertEqual(DeviceCodeError.from(errorCode: nil), .failed)
    }

    func testTheInstallTokenDecodesAndParsesItsExpiry() throws {
        let json = Data(#"{"token":"expi_abc","expiresAt":"2026-10-02T12:15:00.000Z"}"#.utf8)
        let minted = try JSONDecoder().decode(CliInstallToken.self, from: json)
        XCTAssertEqual(minted.token, "expi_abc")
        XCTAssertEqual(minted.expiresAtDate, ISO8601DateFormatter().date(from: "2026-10-02T12:15:00Z"))
    }
}

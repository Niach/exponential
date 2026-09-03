import Foundation
import SwiftUI
import XCTest
import ExpUI

// EXP-698 r4: the avatar fallback palette is a CONTRACT, not a list of eight
// pretty colours. `avatarHueIndex` (ExpCore) hashes a user id into a slot of
// this array on every client, so its LENGTH is what the modulo lands in and its
// ORDER is which person reads which colour. A ninth hue, or a reorder, recolours
// everyone on iOS alone.
final class AvatarHuesTests: XCTestCase {
    func testThePaletteHasTheContractSize() {
        XCTAssertEqual(DesignTokens.Avatar.hues.count, 8)
    }

    func testThePaletteIsInTheGeneratedOrder() {
        XCTAssertEqual(
            DesignTokens.Avatar.hues,
            [
                DesignTokens.Avatar.red,
                DesignTokens.Avatar.orange,
                DesignTokens.Avatar.yellow,
                DesignTokens.Avatar.green,
                DesignTokens.Avatar.teal,
                DesignTokens.Avatar.blue,
                DesignTokens.Avatar.violet,
                DesignTokens.Avatar.pink,
            ]
        )
    }
}

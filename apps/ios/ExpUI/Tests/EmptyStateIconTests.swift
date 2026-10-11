import ExpCore
import XCTest
@testable import ExpUI

// EXP-1274: every empty-state concept resolves to a registry glyph.
final class EmptyStateIconTests: XCTestCase {
    func testEveryContractConceptResolves() {
        for state in EmptyStateCopy.all {
            let glyph = EmptyStateIcon.glyph(state.icon)
            XCTAssertNotNil(glyph, "\(state.id): \(state.icon) has no glyph")
            XCTAssertNotNil(glyph.flatMap(AppIcons.assetName), state.id)
        }
    }
}

import XCTest
import ExpUI

// EXP-1029 contract — sub-shell navigation on iOS, skipped until EXP-1020
// (web, IDE and Android carry the same case names).
final class SubShellContractTests: XCTestCase {

    func testTappingTheRowSlidesTheChildPageInPlaceOfTheWholeCard() throws {
        throw XCTSkip("EXP-1029 contract: EXP-1020 implements sub-shell navigation")
    }

    func testTheChildPageCarriesABackButtonOnTopThatReturnsToTheCard() throws {
        throw XCTSkip("EXP-1029 contract: EXP-1020 implements sub-shell navigation")
    }

    func testASubShellInsideTheChildPageSlidesOneLevelDeeper() throws {
        throw XCTSkip("EXP-1029 contract: EXP-1020 implements sub-shell navigation")
    }

    func testADisabledRowNeverOpens() throws {
        throw XCTSkip("EXP-1029 contract: EXP-1020 implements sub-shell navigation")
    }
}

import XCTest
import ExpUI

// EXP-1676: a sheet's scrim is web's overlay (`bg-black/60`) over a blur, the
// alert's number, on every sheet (GlassSheetChrome installs it).
final class SheetBackdropTests: XCTestCase {

    func testTheScrimIsWebsSixtyPercent() {
        XCTAssertEqual(SheetBackdropMetrics.scrimOpacity, 0.6, accuracy: 0.0001)
        XCTAssertEqual(Double(SheetBackdropMetrics.scrimOpacity), GlassAlertMetrics.scrimOpacity, accuracy: 0.0001)
    }
}

import ExpCore
@testable import ExpUI
import XCTest

/// #FEED-191 (results-tab.json `tile`): a thumbnail's width and text line.
final class ResultsTabMetricsTests: XCTestCase {
    private func picture(_ width: Double?, _ height: Double?, caption: String? = nil) -> ResultsTab.Picture {
        ResultsTab.Picture(label: "label", attachmentId: "a", width: width, height: height, caption: caption)
    }

    func testWidthFollowsTheAspectCappedAtTheMax() {
        XCTAssertEqual(ResultsTabMetrics.thumbWidth(picture(1179, 2556)), 111)
        XCTAssertEqual(ResultsTabMetrics.thumbWidth(picture(1600, 1000)), 320)
        XCTAssertEqual(ResultsTabMetrics.thumbWidth(picture(4000, 1000)), 320)
    }

    func testAnUnknownSizeIsFourThree() {
        XCTAssertEqual(ResultsTabMetrics.thumbWidth(picture(nil, nil)), 320)
        XCTAssertEqual(ResultsTabMetrics.thumbWidth(picture(0, 100)), 320)
    }

    func testTheLineIsTheCaptionElseTheLabel() {
        XCTAssertEqual(ResultsTabMetrics.pictureText(picture(1, 1, caption: "Mixed")), "Mixed")
        XCTAssertEqual(ResultsTabMetrics.pictureText(picture(1, 1)), "label")
    }
}

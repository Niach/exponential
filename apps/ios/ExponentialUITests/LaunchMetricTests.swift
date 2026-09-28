import XCTest

/// VAPP-3 spike (throwaway): cold-start metric for the with/without Rust peer core comparison.
/// Skipped unless the runner env has VAPP3_LAUNCH_METRIC=1 (xcodebuild: TEST_RUNNER_VAPP3_LAUNCH_METRIC=1),
/// so fastlane's only_testing lanes and any stray full run never pay for it.
final class LaunchMetricTests: XCTestCase {
    @MainActor
    func testColdLaunch() throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["VAPP3_LAUNCH_METRIC"] == "1", "VAPP-3 launch metric only")
        let options = XCTMeasureOptions()
        options.iterationCount = 5
        measure(metrics: [XCTApplicationLaunchMetric()], options: options) {
            XCUIApplication().launch()
        }
    }
}

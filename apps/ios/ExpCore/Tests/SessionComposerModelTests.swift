import Foundation
import XCTest

@testable import ExpCore

// The session composer footer's model pill reads the engine's `model` option.
final class SessionComposerModelTests: XCTestCase {
    func testReadsTheSessionModelOffTheConfigOption() {
        XCTAssertNil(AgentSessionConfig(options: []).modelValue)
        XCTAssertNil(AgentSessionConfig(options: [
            AgentConfigOption(id: "model", label: "Model", value: ""),
        ]).modelValue)
        XCTAssertEqual(
            AgentSessionConfig(options: [
                AgentConfigOption(id: "model", label: "Model", value: "opus"),
            ]).modelValue,
            "opus"
        )
    }
}

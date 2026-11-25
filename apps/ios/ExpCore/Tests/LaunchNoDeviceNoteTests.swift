import XCTest
@testable import ExpCore

/// #EXP-2135: the composer's "nothing to start on" hint names only the PICKED
/// agent's missing sign-in.
final class LaunchNoDeviceNoteTests: XCTestCase {
    private func device(signedOut: String) -> SteerDevice {
        let seen = ISO8601DateFormatter().string(from: Date())
        return SteerDevice(
            entity: DeviceEntity(
                id: "r", userId: "me", deviceId: "d", label: "Mac", agents: "[]", unauthedAgents: signedOut,
                acpAgents: "[]", lastSeenAt: seen
            ),
            currentUserId: "me"
        )
    }

    func testAnotherAgentsSignInIsNotNamed() {
        let device = device(signedOut: #"["codex"]"#)
        XCTAssertTrue(device.isOnline)
        XCTAssertEqual(
            LaunchVocabulary.noDeviceNote(agent: "claude", devices: [device]),
            "No desktop online. Open the Exponential desktop app to start a session."
        )
        XCTAssertEqual(
            LaunchVocabulary.noDeviceNote(agent: "codex", devices: [device]),
            "codex not signed in on your machines. Sign in on the machine first."
        )
    }
}

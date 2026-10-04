import Foundation
import XCTest
@testable import ExpCore

// SLOP-7/SLOP-26: the GitHub connection block + Add-repository picker copy is
// byte-identical on all four clients. These literals mirror web
// `github-connect-copy.ts` (`github-connect-copy.test.ts`) — change both
// together.
final class GithubCopyTests: XCTestCase {
    func testConnectionBlockLiterals() {
        XCTAssertEqual(GithubCopy.sectionTitle, "Repositories")
        XCTAssertEqual(GithubCopy.addRepository, "Add repository")
        XCTAssertEqual(GithubCopy.statusFailed, "Couldn’t reach GitHub connect state.")
        XCTAssertEqual(GithubCopy.notConfigured, "GitHub isn’t configured on this server.")
        XCTAssertEqual(GithubCopy.notLinked, "No GitHub account connected")
        XCTAssertEqual(GithubCopy.connectGithub, "Connect GitHub")
        XCTAssertEqual(GithubCopy.connectedAs("octocat"), "Connected as octocat")
        XCTAssertEqual(GithubCopy.connected, "GitHub connected")
        XCTAssertEqual(GithubCopy.reconnectNeeded, "Your GitHub connection expired.")
        XCTAssertEqual(GithubCopy.reconnect, "Reconnect")
        XCTAssertEqual(GithubCopy.reconnectGithub, "Reconnect GitHub")
        XCTAssertEqual(GithubCopy.disconnectTitle, "Disconnect GitHub")
        XCTAssertEqual(
            GithubCopy.disconnectBody,
            "This unlinks GitHub from your account. Repositories already added keep working; adding more needs a reconnect."
        )
        XCTAssertEqual(
            GithubCopy.notInstalled,
            "The Exponential app isn’t installed on any of your GitHub accounts yet."
        )
        XCTAssertEqual(GithubCopy.installApp, "Install the app")
        XCTAssertEqual(GithubCopy.installAnother, "Install on another account")
        XCTAssertEqual(GithubCopy.accountsHeader, "GitHub accounts with the app installed")
        XCTAssertEqual(GithubCopy.configure, "Configure")
        XCTAssertEqual(
            GithubCopy.configureTitle("acme"),
            "Configure which repositories acme grants on GitHub"
        )
        XCTAssertEqual(
            GithubCopy.installationCaption,
            "Repositories come from these accounts. Configure one to grant more."
        )
        XCTAssertEqual(
            GithubCopy.suspendedLine(["acme", "octocat"]),
            "GitHub suspended the Exponential app for acme, octocat. Unsuspend it on GitHub."
        )
        XCTAssertEqual(GithubCopy.manage, "Manage")
        XCTAssertEqual(GithubCopy.noRepositories, "No repositories added yet.")
        XCTAssertEqual(GithubCopy.linkFailed, "GitHub didn’t finish connecting. Try again.")
    }

    func testFallbackLabelIsLowerCase() {
        XCTAssertEqual(GithubCopy.installationLabel(login: nil, installationId: 42), "installation 42")
        XCTAssertEqual(GithubCopy.installationLabel(login: "", installationId: 42), "installation 42")
        XCTAssertEqual(GithubCopy.installationLabel(login: "acme", installationId: 42), "acme")
    }

    func testPickerLiterals() {
        XCTAssertEqual(GithubCopy.pickerTitle, "Add repository")
        XCTAssertEqual(GithubCopy.pickerLoading, "Loading your GitHub repositories…")
        XCTAssertEqual(
            GithubCopy.pickerNotConfigured,
            "GitHub isn’t configured on this server, so repositories can’t be added."
        )
        XCTAssertEqual(
            GithubCopy.pickerNotLinked,
            "Connect your GitHub account to pick a repository. You’ll come right back here."
        )
        XCTAssertEqual(
            GithubCopy.pickerNotInstalled,
            "Install the Exponential app on the GitHub account that owns the repository. You’ll come right back here."
        )
        XCTAssertEqual(GithubCopy.pickerConnectedCheck, "I’ve done that")
        XCTAssertEqual(
            GithubCopy.pickerSuspended([nil]),
            "GitHub suspended the Exponential app for a connected account. Its repositories can’t be added until you unsuspend it on GitHub."
        )
        XCTAssertEqual(
            GithubCopy.pickerSuspended(["acme", ""]),
            "GitHub suspended the Exponential app for acme, a connected account. Its repositories can’t be added until you unsuspend it on GitHub."
        )
        XCTAssertEqual(
            GithubCopy.pickerReconnectBanner,
            "Your GitHub connection expired. Reconnect to list your repositories."
        )
        XCTAssertEqual(GithubCopy.searchPlaceholder, "Search repositories…")
        XCTAssertEqual(GithubCopy.noMatch, "No repositories found.")
        XCTAssertEqual(
            GithubCopy.nonePushable,
            "The app is installed, but none of its repositories lets you push. Grant one on GitHub, then refresh."
        )
        XCTAssertEqual(
            GithubCopy.footerExplain,
            "Only repositories you can push to, on accounts where the app is installed, appear here. Missing one? Grant it on GitHub, then refresh."
        )
        XCTAssertEqual(
            GithubCopy.capNote,
            "Showing the first 500 repositories per account — use the field below for the rest."
        )
        XCTAssertEqual(GithubCopy.refresh, "Refresh")
        XCTAssertEqual(GithubCopy.lookupPlaceholder, "owner/name")
        XCTAssertEqual(GithubCopy.lookupAccessibility, "Add repository by name")
        XCTAssertEqual(GithubCopy.lookUp, "Look up")
        XCTAssertEqual(
            GithubCopy.addForbidden,
            "GitHub says you can’t push to this repository, or your connection expired. Reconnect GitHub and try again."
        )
    }

    func testGrantForbiddenArm() {
        XCTAssertTrue(GithubCopy.isGrantForbidden(code: "FORBIDDEN", message: "No access. Reconnect GitHub to refresh."))
        XCTAssertFalse(GithubCopy.isGrantForbidden(code: "FORBIDDEN", message: "Only owners can do that."))
        XCTAssertFalse(GithubCopy.isGrantForbidden(code: "PRECONDITION_FAILED", message: "Reconnect GitHub"))
    }
}

import Foundation
import XCTest
@testable import ExpCore

// FEED-42: the GitHub connection block + Add-repository picker copy is
// byte-identical on all four clients. These literals mirror web
// `github-connect-copy.ts` — change both together.
final class GithubCopyTests: XCTestCase {
    func testConnectionBlockLiterals() {
        XCTAssertEqual(GithubCopy.addRepository, "Add repository")
        XCTAssertEqual(
            GithubCopy.intro,
            "Connect a GitHub account or organization first, then add its repositories to share them with the team — everyone can code on a shared repo. Point a board at one to make it the clone target for “Start coding”."
        )
        XCTAssertEqual(GithubCopy.statusFailed, "Couldn’t reach GitHub connect state.")
        XCTAssertEqual(GithubCopy.notConfigured, "GitHub isn’t configured on this server.")
        XCTAssertEqual(GithubCopy.notInstalled, "No GitHub account connected")
        XCTAssertEqual(GithubCopy.accountsHeader, "GitHub accounts connected to this team")
        XCTAssertEqual(
            GithubCopy.installationsCaption,
            "An installation is per GitHub account or organization. Repositories come from the accounts listed here."
        )
        XCTAssertEqual(GithubCopy.noRepositories, "No repositories connected yet.")
        XCTAssertEqual(
            GithubCopy.suspendedLine(["acme", "installation 7"]),
            "GitHub suspended the Exponential app for acme, installation 7. Unsuspend it on GitHub."
        )
        XCTAssertEqual(
            GithubCopy.reauthLine(["acme"]),
            "Reconnect GitHub to refresh which repositories you can access from acme."
        )
        XCTAssertEqual(
            GithubCopy.staleLine("acme"),
            "No one’s GitHub connection covers acme anymore — reconnecting can’t refresh it."
        )
    }

    func testFallbackLabelIsLowerCase() {
        XCTAssertEqual(GithubCopy.installationLabel(login: nil, installationId: 42), "installation 42")
        XCTAssertEqual(GithubCopy.installationLabel(login: "acme", installationId: 42), "acme")
    }

    func testConfirmCopy() {
        XCTAssertEqual(GithubCopy.disconnectTitle, "Disconnect GitHub account")
        XCTAssertEqual(
            GithubCopy.unlinkConfirm("acme"),
            "This disconnects acme from the team. Repositories connected through it must be removed first."
        )
        XCTAssertEqual(
            GithubCopy.staleConfirm("acme"),
            "This removes acme from the team. Nobody’s GitHub connection covers it, so no repositories are lost."
        )
    }

    func testPickerLiterals() {
        XCTAssertEqual(GithubCopy.loadingRepos, "Loading your GitHub repositories…")
        XCTAssertEqual(
            GithubCopy.pickerNotConfigured,
            "GitHub isn’t configured on this server, so repositories can’t be connected."
        )
        XCTAssertEqual(
            GithubCopy.pickerNotInstalled,
            "Connect the Exponential GitHub App to pick a repository. You’ll come right back here."
        )
        XCTAssertEqual(GithubCopy.iveConnected, "I’ve connected")
        XCTAssertEqual(
            GithubCopy.pickerSuspended(["acme", nil]),
            "GitHub suspended the Exponential app for acme, a connected account. Its repositories can’t be connected until you unsuspend it on GitHub."
        )
        XCTAssertEqual(
            GithubCopy.reauthBanner(accounts: ["a", "b"], emptyList: false),
            "Reconnect GitHub (a, b) to refresh. Repos created or shared with you since your last connect won’t appear until you do."
        )
        XCTAssertEqual(
            GithubCopy.reauthBanner(accounts: [], emptyList: false),
            "Reconnect GitHub to refresh. Repos created or shared with you since your last connect won’t appear until you do."
        )
        XCTAssertEqual(
            GithubCopy.reauthBanner(accounts: ["a", "b"], emptyList: true),
            "Reconnect GitHub to load the repositories you can access from a, b."
        )
        XCTAssertEqual(GithubCopy.noneGranted, "None of your connected GitHub accounts grants a repository yet.")
        XCTAssertEqual(
            GithubCopy.footerSentence,
            "Only repositories your GitHub installation grants appear here. Missing one? Grant it on GitHub, then refresh."
        )
        XCTAssertEqual(
            GithubCopy.capNote,
            "Showing the first 500 repositories per account — use the field below for the rest."
        )
        XCTAssertEqual(GithubCopy.lookupAccessibility, "Add repository by name")
        XCTAssertEqual(
            GithubCopy.addForbidden,
            "GitHub says you don’t have access to this repository, or your connection is stale. Reconnect GitHub and try again."
        )
    }

    func testGrantForbiddenArm() {
        XCTAssertTrue(GithubCopy.isGrantForbidden(code: "FORBIDDEN", message: "No access. Reconnect GitHub to refresh."))
        XCTAssertFalse(GithubCopy.isGrantForbidden(code: "FORBIDDEN", message: "Only owners can do that."))
        XCTAssertFalse(GithubCopy.isGrantForbidden(code: "PRECONDITION_FAILED", message: "Reconnect GitHub"))
    }
}

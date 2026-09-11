import Foundation
import XCTest
@testable import ExpCore

// FEED-30/32: the "Add by name" shape check mirrors the web REPO_FULL_NAME_RE,
// and the board form's repository label is never blank for a linked board
// whose repo the local list can't resolve (web board-repo-field.tsx parity).
final class RepoFullNameTests: XCTestCase {
    func testAcceptsExactlyOwnerSlashName() {
        for ok in ["acme/web", "a/b", "org-name/repo.name", "Niach/exponential"] {
            XCTAssertTrue(RepoFullName.isValid(ok), ok)
        }
    }

    func testRejectsEverythingElse() {
        for bad in ["", "acme", "acme/", "/web", "acme/web/extra", "acme /web", "acme/we b", " acme/web", "acme/web\n"] {
            XCTAssertFalse(RepoFullName.isValid(bad), bad.debugDescription)
        }
    }

    func testUnlinkedBoardReadsNoRepository() {
        XCTAssertEqual(
            BoardRepoLabel.trigger(selectedName: nil, repositoryId: nil, loading: false, resolving: false),
            "No repository"
        )
        XCTAssertEqual(
            BoardRepoLabel.trigger(selectedName: nil, repositoryId: nil, loading: true, resolving: false),
            "Loading…"
        )
    }

    func testResolvedRepoWinsOverEveryFallback() {
        XCTAssertEqual(
            BoardRepoLabel.trigger(selectedName: "acme/web", repositoryId: "repo-1", loading: true, resolving: true),
            "acme/web"
        )
    }

    func testUnknownLinkedRepoNeverRendersBlank() {
        XCTAssertEqual(
            BoardRepoLabel.trigger(selectedName: nil, repositoryId: "repo-1", loading: true, resolving: false),
            "Loading repository…"
        )
        XCTAssertEqual(
            BoardRepoLabel.trigger(selectedName: nil, repositoryId: "repo-1", loading: false, resolving: true),
            "Loading repository…"
        )
        XCTAssertEqual(
            BoardRepoLabel.trigger(selectedName: nil, repositoryId: "repo-1", loading: false, resolving: false),
            "Repository unavailable"
        )
    }
}

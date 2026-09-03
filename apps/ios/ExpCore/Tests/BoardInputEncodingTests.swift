import XCTest
@testable import ExpCore

// The board mutations' wire payloads (EXP-712). Absent vs null is
// load-bearing here: the server reads an ABSENT `defaultBranch` as "leave it"
// and an explicit NULL as "follow the repo's default branch again", so the
// encoders are pinned rather than left to Codable's defaults.
final class BoardInputEncodingTests: XCTestCase {
    private func json(_ value: some Encodable) throws -> [String: Any] {
        let data = try JSONEncoder().encode(value)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    // MARK: - boards.create

    func testCreateOmitsRepositoryAndBranchWhenUnset() throws {
        let object = try json(CreateBoardInput(teamId: "t1", name: "Backend", prefix: "API"))
        XCTAssertEqual(object["name"] as? String, "Backend")
        // The server's repository union would reject a JSON null.
        XCTAssertNil(object.index(forKey: "repository"))
        XCTAssertNil(object.index(forKey: "defaultBranch"))
    }

    func testCreateCarriesRegistryRepoAndBranch() throws {
        let object = try json(CreateBoardInput(
            teamId: "t1",
            name: "Backend",
            prefix: "API",
            repository: .repositoryId("repo-1"),
            defaultBranch: "release/1.x"
        ))
        let repository = try XCTUnwrap(object["repository"] as? [String: Any])
        XCTAssertEqual(repository["repositoryId"] as? String, "repo-1")
        XCTAssertEqual(object["defaultBranch"] as? String, "release/1.x")
    }

    func testCreateCarriesInlineRepoAlongsideTheBoardBranch() throws {
        // The inline repo's `defaultBranch` seeds the REGISTRY row (GitHub's
        // default); the board's own branch is the top-level key.
        let object = try json(CreateBoardInput(
            teamId: "t1",
            name: "Backend",
            prefix: "API",
            repository: .fullName("acme/app", defaultBranch: "main", isPrivate: true),
            defaultBranch: "develop"
        ))
        let repository = try XCTUnwrap(object["repository"] as? [String: Any])
        XCTAssertEqual(repository["fullName"] as? String, "acme/app")
        XCTAssertEqual(repository["defaultBranch"] as? String, "main")
        XCTAssertEqual(repository["private"] as? Bool, true)
        XCTAssertEqual(object["defaultBranch"] as? String, "develop")
    }

    // MARK: - boards.setRepository

    func testSetRepositorySendsAnExplicitNullRepository() throws {
        let object = try json(SetBoardRepositoryInput(boardId: "b1", repositoryId: nil))
        XCTAssertEqual(object["boardId"] as? String, "b1")
        XCTAssertTrue(object["repositoryId"] is NSNull)
        // Absent: a retarget RESETS the branch server-side unless one is sent.
        XCTAssertNil(object.index(forKey: "defaultBranch"))
    }

    func testSetRepositoryCanCarryTheNewRepoBranch() throws {
        let object = try json(SetBoardRepositoryInput(
            boardId: "b1",
            repositoryId: "repo-2",
            defaultBranch: "release/2.x"
        ))
        XCTAssertEqual(object["repositoryId"] as? String, "repo-2")
        XCTAssertEqual(object["defaultBranch"] as? String, "release/2.x")
    }

    // MARK: - boards.update

    func testBranchPatchSendsNullToFollowTheRepoAgain() throws {
        let object = try json(UpdateBoardInput(boardId: "b1", defaultBranch: nil))
        XCTAssertEqual(object["boardId"] as? String, "b1")
        XCTAssertTrue(object["defaultBranch"] is NSNull)
        XCTAssertNil(object.index(forKey: "name"))
    }

    func testBranchPatchSendsThePinnedBranch() throws {
        let object = try json(UpdateBoardInput(boardId: "b1", defaultBranch: "release/1.x"))
        XCTAssertEqual(object["defaultBranch"] as? String, "release/1.x")
    }

    func testNonBranchPatchLeavesTheBranchAlone() throws {
        let object = try json(UpdateBoardInput(boardId: "b1", name: "Renamed"))
        XCTAssertEqual(object["name"] as? String, "Renamed")
        XCTAssertNil(object.index(forKey: "defaultBranch"))
    }
}

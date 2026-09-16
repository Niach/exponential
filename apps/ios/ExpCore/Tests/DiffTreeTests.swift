import Foundation
import XCTest
@testable import ExpCore

// EXP-916: the fixture is the contract. Every mirror (TS `@exp/domain-contract`
// `diff-tree.test.ts`, web `@exp/ui` `FileDiffTree`, desktop
// `domain::diff_tree`, Android `DiffTreeTest`, iOS here) replays
// `fixtures/diff/tree.json` with THESE test names.
//
// A case: `files` (path + counts; status is irrelevant to the tree and defaults
// to `modified`), an optional `query`, and `expected` =
// `DiffTree.render(DiffTree.fileTree(files, query:))`.
final class DiffTreeTests: XCTestCase {
    private struct FixtureFile: Decodable {
        let path: String
        let additions: Int
        let deletions: Int
    }

    private struct FixtureCase: Decodable {
        let name: String
        let files: [FixtureFile]
        let query: String?
        let expected: [String]
    }

    /// The committed contract fixture, read through `#filePath` because the
    /// unit-test bundle carries no repo resources.
    private func cases() throws -> [FixtureCase] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/diff/tree.json")
        return try JSONDecoder().decode([FixtureCase].self, from: Data(contentsOf: url))
    }

    private func file(_ entry: FixtureFile) -> Diff.File {
        Diff.File(
            path: entry.path,
            status: .modified,
            additions: entry.additions,
            deletions: entry.deletions
        )
    }

    private func file(_ path: String, _ additions: Int = 1, _ deletions: Int = 0) -> Diff.File {
        Diff.File(path: path, status: .modified, additions: additions, deletions: deletions)
    }

    /// `every fixture case renders byte-exact`
    func testEveryFixtureCaseRendersByteExact() throws {
        for entry in try cases() {
            let tree = DiffTree.fileTree(entry.files.map(file), query: entry.query ?? "")
            XCTAssertEqual(DiffTree.render(tree), entry.expected, entry.name)
        }
    }

    /// `the fixture covers a compaction, a query and an empty input`
    func testTheFixtureCoversACompactionAQueryAndAnEmptyInput() throws {
        let names = try cases().map(\.name).joined(separator: "\n")
        XCTAssertTrue(names.contains("compacts"), names)
        XCTAssertTrue(names.contains("query"), names)
        XCTAssertTrue(names.contains("no files"), names)
    }

    /// `a file node keeps its input index and full path`
    func testAFileNodeKeepsItsInputIndexAndFullPath() {
        let tree = DiffTree.fileTree(["src/b.ts", "src/a.ts", "top.ts"].map { file($0) })
        XCTAssertEqual(tree.map(\.kind), [.dir, .file])
        XCTAssertEqual(tree.map(\.path), ["src", "top.ts"])
        XCTAssertEqual(tree.map(\.index), [-1, 2])
        XCTAssertEqual(tree[0].children.map(\.path), ["src/a.ts", "src/b.ts"])
        XCTAssertEqual(tree[0].children.map(\.index), [1, 0])
    }

    /// `a compacted directory's path is the deepest segment's`
    func testACompactedDirectorysPathIsTheDeepestSegments() {
        let tree = DiffTree.fileTree([file("apps/web/src/a.ts")])
        XCTAssertEqual(tree[0].name, "apps/web/src")
        XCTAssertEqual(tree[0].path, "apps/web/src")
    }
}

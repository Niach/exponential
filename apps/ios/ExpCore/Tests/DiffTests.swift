import Foundation
import XCTest
@testable import ExpCore

// EXP-895: the fixture is the contract. Every mirror (TS `@exp/domain-contract`
// `diff.test.ts`, desktop `domain::diff`, Android `DiffTest`, iOS here) replays
// `fixtures/diff/cases.json` + `summary.json` with THESE four test names, so a
// rule that moves here moves everywhere or four suites go red at once.
//
// How a case is parsed (the fixture's own contract):
//   - `form: "pullFile"` → `Diff.fromPullFile(…)`: GitHub's raw PullFile
//     (`filename`, `previous_filename`, raw `status`, `additions`, `deletions`,
//     `patch`), the ONE mapping every client's PR diff runs.
//   - `form: "hunks"` WITH a `path` → `Diff.parsePatch(path:status:patch:)`
//     (GitHub's PullFile shape: the path and status arrive beside the patch).
//   - every other case, `form: "hunks"` WITHOUT a path included →
//     `Diff.parse(_:)`, which auto-detects the form. A pathless `hunks` case is
//     exactly the auto-detect path: one file, empty path, `modified`.
//   - `expected` is `Diff.render(…)`, `summary` is
//     `Diff.summaryLabel(Diff.totals(…))`, and `unchanged` is the FIRST file's
//     `[unchangedBefore(h0), unchangedBetween(h0, h1), …]` (empty when it has no
//     hunks, or when there is no file at all).
//   - `expectedMerged`, when present, is `render` over `mergeFilesByPath(files)`.
final class DiffTests: XCTestCase {
    private struct FixtureCase: Decodable {
        let name: String
        let form: String
        let input: String?
        let pullFile: FixturePullFile?
        let path: String?
        let status: String?
        let expected: [String]
        let expectedMerged: [String]?
        let summary: String
        let unchanged: [Int]
    }

    /// GitHub's raw PullFile keys, snake_case as the API sends them.
    private struct FixturePullFile: Decodable {
        let filename: String
        let previous_filename: String?
        let status: String
        let additions: Int
        let deletions: Int
        let patch: String?
    }

    private struct SummaryCase: Decodable {
        let files: Int
        let additions: Int
        let deletions: Int
        let expected: String
    }

    /// The committed contract fixtures, read through `#filePath` because the
    /// unit-test bundle carries no repo resources.
    private func fixtureURL(_ name: String) -> URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/diff/\(name)")
    }

    private func cases() throws -> [FixtureCase] {
        let data = try Data(contentsOf: fixtureURL("cases.json"))
        return try JSONDecoder().decode([FixtureCase].self, from: data)
    }

    private func summaries() throws -> [SummaryCase] {
        let data = try Data(contentsOf: fixtureURL("summary.json"))
        return try JSONDecoder().decode([SummaryCase].self, from: data)
    }

    /// The fixture's dispatch rule: a `pullFile` case is a whole GitHub
    /// PullFile, a `hunks` case that carries a path is its patch alone;
    /// everything else auto-detects.
    private func parse(_ entry: FixtureCase) -> Diff.Parsed {
        if entry.form == "pullFile", let file = entry.pullFile {
            return Diff.Parsed(files: [Diff.fromPullFile(
                filename: file.filename,
                previousFilename: file.previous_filename,
                status: file.status,
                additions: file.additions,
                deletions: file.deletions,
                patch: file.patch
            )])
        }
        if entry.form == "hunks", let path = entry.path {
            let status = Diff.Status(rawValue: entry.status ?? "modified") ?? .modified
            return Diff.Parsed(files: [Diff.parsePatch(path: path, status: status, patch: entry.input ?? "")])
        }
        return Diff.parse(entry.input ?? "")
    }

    private func unchangedRun(_ parsed: Diff.Parsed) -> [Int] {
        guard let first = parsed.files.first else { return [] }
        return first.hunks.enumerated().map { index, hunk in
            index == 0
                ? Diff.unchangedBefore(hunk)
                : Diff.unchangedBetween(first.hunks[index - 1], hunk)
        }
    }

    /// `every fixture case parses byte exact`
    func testEveryFixtureCaseParsesByteExact() throws {
        for entry in try cases() {
            let parsed = parse(entry)
            XCTAssertEqual(Diff.render(parsed), entry.expected, entry.name)
            if let merged = entry.expectedMerged {
                let folded = Diff.Parsed(
                    files: Diff.mergeFilesByPath(parsed.files),
                    truncatedLines: parsed.truncatedLines
                )
                XCTAssertEqual(Diff.render(folded), merged, "\(entry.name) (merged)")
            }
        }
    }

    /// `the fixture covers every input form`
    func testTheFixtureCoversEveryInputForm() throws {
        let fixture = try cases()
        XCTAssertGreaterThanOrEqual(fixture.count, 18)
        XCTAssertEqual(Set(fixture.map(\.form)).sorted(), ["bare", "git", "hunks", "none", "pullFile"])
        // A `none` case is the empty parse; every other form yields files.
        for entry in fixture {
            let files = parse(entry).files
            if entry.form == "none" {
                XCTAssertTrue(files.isEmpty, entry.name)
            } else {
                XCTAssertGreaterThan(files.count, 0, entry.name)
            }
        }
    }

    /// `summary label matches every fixture case`
    func testSummaryLabelMatchesEveryFixtureCase() throws {
        for entry in try cases() {
            let totals = Diff.totals(parse(entry).files)
            XCTAssertEqual(
                Diff.summaryLabel(
                    files: totals.files,
                    additions: totals.additions,
                    deletions: totals.deletions
                ),
                entry.summary,
                entry.name
            )
        }
        let rows = try summaries()
        for row in rows {
            XCTAssertEqual(
                Diff.summaryLabel(files: row.files, additions: row.additions, deletions: row.deletions),
                row.expected,
                "\(row.files)/\(row.additions)/\(row.deletions)"
            )
        }
        XCTAssertGreaterThanOrEqual(rows.count, 4)
    }

    /// `unchanged line counts match every fixture case`
    func testUnchangedLineCountsMatchEveryFixtureCase() throws {
        for entry in try cases() {
            XCTAssertEqual(unchangedRun(parse(entry)), entry.unchanged, entry.name)
        }
    }

    // ── mergeFilesByPath ────────────────────────────────────────────────────

    private func file(
        _ path: String,
        status: Diff.Status = .modified,
        binary: Bool = false,
        previousPath: String? = nil
    ) -> Diff.File {
        Diff.File(
            path: path,
            previousPath: previousPath,
            status: status,
            additions: 1,
            deletions: 1,
            binary: binary
        )
    }

    func testMergeFoldsRepeatsInOrderOfFirstAppearance() {
        let merged = Diff.mergeFilesByPath([file("b.ts"), file("a.ts"), file("b.ts")])
        XCTAssertEqual(merged.map(\.path), ["b.ts", "a.ts"])
        XCTAssertEqual(merged[0].additions, 2)
        XCTAssertEqual(merged[0].deletions, 2)
    }

    func testMergeKeepsTheLaterStatusBinaryAndPreviousPath() {
        let merged = Diff.mergeFilesByPath([
            file("a.ts"),
            file("a.ts", status: .renamed, binary: true, previousPath: "old.ts"),
        ])
        XCTAssertEqual(merged.count, 1)
        XCTAssertEqual(merged[0].status, .renamed)
        XCTAssertTrue(merged[0].binary)
        XCTAssertEqual(merged[0].previousPath, "old.ts")
    }

    func testMergeConcatenatesHunksAndNeverMutatesItsInput() {
        let one = Diff.parsePatch(path: "a.ts", status: .modified, patch: "@@ -1 +1 @@\n-a\n+A\n")
        let two = Diff.parsePatch(path: "a.ts", status: .modified, patch: "@@ -9 +9 @@\n-b\n+B\n")
        let merged = Diff.mergeFilesByPath([one, two])
        XCTAssertEqual(merged[0].hunks.count, 2)
        XCTAssertEqual(one.hunks.count, 1)
        XCTAssertEqual(one.additions, 1)
    }

    // ── parsePatch ──────────────────────────────────────────────────────────

    func testAnAbsentPatchIsAFileWithNoHunks() {
        for patch in [nil, ""] as [String?] {
            let file = Diff.parsePatch(path: "logo.png", status: .modified, patch: patch)
            XCTAssertEqual(file, Diff.File(path: "logo.png", status: .modified))
        }
    }

    func testThePatchNeverRenamesTheFileItWasHanded() {
        let file = Diff.parsePatch(
            path: "given.ts",
            status: .added,
            patch: "diff --git a/other.ts b/other.ts\n--- /dev/null\n+++ b/other.ts\n@@ -0,0 +1 @@\n+x\n"
        )
        XCTAssertEqual(file.path, "given.ts")
        XCTAssertEqual(file.status, .added)
        XCTAssertEqual(file.additions, 1)
    }

    func testGithubsStatusVocabularyMapsOntoOurs() {
        XCTAssertEqual(Diff.Status.fromPullFile("added"), .added)
        XCTAssertEqual(Diff.Status.fromPullFile("removed"), .removed)
        XCTAssertEqual(Diff.Status.fromPullFile("renamed"), .renamed)
        XCTAssertEqual(Diff.Status.fromPullFile("copied"), .copied)
        XCTAssertEqual(Diff.Status.fromPullFile("changed"), .modified)
        XCTAssertEqual(Diff.Status.fromPullFile("unchanged"), .modified)
        XCTAssertEqual(Diff.Status.fromPullFile("something-new"), .modified)
    }

    // ── Labels ──────────────────────────────────────────────────────────────

    func testAdditionsDeletionsAndUnchangedLabels() {
        XCTAssertEqual(Diff.additionsLabel(0), "+0")
        XCTAssertEqual(Diff.additionsLabel(12), "+12")
        // U+2212 MINUS SIGN, never an ASCII hyphen.
        XCTAssertEqual(Diff.deletionsLabel(4), "\u{2212}4")
        XCTAssertEqual(Diff.deletionsLabel(4).unicodeScalars.first?.value, 0x2212)
        XCTAssertEqual(Diff.unchangedLabel(1), "1 unchanged line")
        XCTAssertEqual(Diff.unchangedLabel(12), "12 unchanged lines")
    }

    func testTheSummaryLabel() {
        XCTAssertEqual(Diff.summaryLabel(files: 0, additions: 0, deletions: 0), "No changes")
        XCTAssertEqual(Diff.summaryLabel(files: 0, additions: 9, deletions: 9), "No changes")
        XCTAssertEqual(Diff.summaryLabel(files: 1, additions: 2, deletions: 0), "1 file +2 \u{2212}0")
        XCTAssertEqual(Diff.summaryLabel(files: 3, additions: 12, deletions: 4), "3 files +12 \u{2212}4")
    }

    func testUnchangedRunsNeverGoNegative() {
        func hunk(_ newStart: Int, _ newLines: Int) -> Diff.Hunk {
            Diff.Hunk(
                oldStart: newStart,
                oldLines: newLines,
                newStart: newStart,
                newLines: newLines,
                header: ""
            )
        }
        XCTAssertEqual(Diff.unchangedBefore(hunk(1, 3)), 0)
        XCTAssertEqual(Diff.unchangedBefore(hunk(0, 0)), 0)
        XCTAssertEqual(Diff.unchangedBefore(hunk(40, 3)), 39)
        XCTAssertEqual(Diff.unchangedBetween(hunk(1, 3), hunk(20, 3)), 16)
        XCTAssertEqual(Diff.unchangedBetween(hunk(1, 30), hunk(20, 3)), 0)
    }
}

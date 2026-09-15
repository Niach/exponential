import Foundation
import XCTest
@testable import ExpCore

/// EXP-895: the view-side decisions of the ONE diff view. The parse contract is
/// fixture-locked in `DiffTests`; these are the strings and the filter the file
/// sheet, the card header and the file rows are built from.
final class DiffPresentationTests: XCTestCase {
    private func file(
        _ path: String,
        status: Diff.Status = .modified,
        binary: Bool = false,
        hunks: [Diff.Hunk] = []
    ) -> Diff.File {
        Diff.File(path: path, status: status, binary: binary, hunks: hunks)
    }

    func testPathSplitting() {
        XCTAssertEqual(DiffPresentation.pathBase("apps/web/src/file.tsx"), "file.tsx")
        XCTAssertEqual(DiffPresentation.pathDir("apps/web/src/file.tsx"), "apps/web/src")
        XCTAssertEqual(DiffPresentation.pathDirPrefix("apps/web/src/file.tsx"), "apps/web/src/")
        // A bare filename has no directory at all.
        XCTAssertEqual(DiffPresentation.pathBase("README.md"), "README.md")
        XCTAssertEqual(DiffPresentation.pathDir("README.md"), "")
        XCTAssertEqual(DiffPresentation.pathDirPrefix("README.md"), "")
        // A trailing slash leaves an empty basename rather than trimming it —
        // the same thing `lastIndexOf('/')` does on the web.
        XCTAssertEqual(DiffPresentation.pathBase("dir/"), "")
        XCTAssertEqual(DiffPresentation.pathDir("dir/"), "dir")
        // An empty path (a hunks-only patch) is not a crash.
        XCTAssertEqual(DiffPresentation.pathBase(""), "")
        XCTAssertEqual(DiffPresentation.pathDir(""), "")
    }

    func testStatusLetters() {
        XCTAssertEqual(DiffPresentation.statusLetter(.added), "A")
        XCTAssertEqual(DiffPresentation.statusLetter(.removed), "D")
        XCTAssertEqual(DiffPresentation.statusLetter(.modified), "M")
        XCTAssertEqual(DiffPresentation.statusLetter(.renamed), "R")
        XCTAssertEqual(DiffPresentation.statusLetter(.copied), "C")
    }

    /// Byte parity with web `noHunksNote` — `binary` outranks the status.
    func testNoHunksNotes() {
        XCTAssertEqual(
            DiffPresentation.noHunksNote(file("a.png", status: .added, binary: true)),
            "Binary file"
        )
        XCTAssertEqual(DiffPresentation.noHunksNote(file("a.txt", status: .added)), "Empty file added")
        XCTAssertEqual(DiffPresentation.noHunksNote(file("a.txt", status: .removed)), "File removed")
        XCTAssertEqual(
            DiffPresentation.noHunksNote(file("a.txt", status: .renamed)),
            "Renamed without content changes"
        )
        XCTAssertEqual(
            DiffPresentation.noHunksNote(file("a.txt", status: .copied)),
            "Copied without content changes"
        )
        XCTAssertEqual(
            DiffPresentation.noHunksNote(file("a.txt", status: .modified)),
            "No textual diff (binary or too large)"
        )
    }

    /// `PrFile.diffFile` mirrors web `fromPullFile`: the patch is parsed, the
    /// rename source is carried over, and GitHub's own counts survive ONLY when
    /// there are no hunks to count.
    func testPullFileConversion() {
        let modified = PrFile(
            filename: "a.ts",
            previousFilename: nil,
            status: "modified",
            additions: 99,
            deletions: 99,
            patch: "@@ -1 +1 @@\n-a\n+A\n"
        )
        let parsed = modified.diffFile
        XCTAssertEqual(parsed.path, "a.ts")
        XCTAssertEqual(parsed.status, .modified)
        XCTAssertEqual(parsed.hunks.count, 1)
        // The patch's own counts win over GitHub's when there IS a patch.
        XCTAssertEqual(parsed.additions, 1)
        XCTAssertEqual(parsed.deletions, 1)

        let renamed = PrFile(
            filename: "b.ts",
            previousFilename: "a.ts",
            status: "renamed",
            additions: 0,
            deletions: 0,
            patch: nil
        )
        let rename = renamed.diffFile
        XCTAssertEqual(rename.status, .renamed)
        XCTAssertEqual(rename.previousPath, "a.ts")
        XCTAssertTrue(rename.hunks.isEmpty)

        // No hunks: GitHub's counts are the only ones there are.
        let binary = PrFile(
            filename: "logo.png",
            previousFilename: nil,
            status: "changed",
            additions: 12,
            deletions: 3,
            patch: nil
        )
        let blob = binary.diffFile
        XCTAssertEqual(blob.status, .modified, "an unknown PullFile status folds into modified")
        XCTAssertEqual(blob.additions, 12)
        XCTAssertEqual(blob.deletions, 3)
    }

    func testFilterIsCaseInsensitiveOverTheWholePath() {
        let files = [
            file("apps/web/src/Board.tsx"),
            file("apps/ios/ExpCore/Sources/Domain/Diff.swift"),
            file("README.md"),
        ]
        XCTAssertEqual(DiffPresentation.filter(files, query: "").map(\.path), files.map(\.path))
        // Whitespace only is an empty query, not a needle nothing matches.
        XCTAssertEqual(DiffPresentation.filter(files, query: "   ").count, 3)
        XCTAssertEqual(
            DiffPresentation.filter(files, query: "board").map(\.path),
            ["apps/web/src/Board.tsx"]
        )
        // The DIRECTORY is searchable too, and order is preserved.
        XCTAssertEqual(
            DiffPresentation.filter(files, query: "APPS/").map(\.path),
            ["apps/web/src/Board.tsx", "apps/ios/ExpCore/Sources/Domain/Diff.swift"]
        )
        XCTAssertTrue(DiffPresentation.filter(files, query: "nothing").isEmpty)
    }
}

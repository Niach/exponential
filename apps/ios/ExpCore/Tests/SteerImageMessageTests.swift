import Foundation
import XCTest
@testable import ExpCore

// EXP-511: the steer image message shape is a cross-client contract — the host
// rewrites each embed to a local file path and reverse-rewrites the echo, so
// these fixtures are byte-identical to the web
// (apps/web/src/lib/steer-image-message.test.ts) and Android twins.
final class SteerImageMessageTests: XCTestCase {
    private let idA = "11111111-1111-4111-8111-111111111111"
    private let idB = "22222222-2222-4222-8222-222222222222"

    func testTextAndImagesJoinWithABlankLine() {
        XCTAssertEqual(
            SteerImageMessage.build(text: "fix the header", attachmentIds: [idA, idB]),
            """
            fix the header

            ![image](/api/attachments/11111111-1111-4111-8111-111111111111)
            ![image](/api/attachments/22222222-2222-4222-8222-222222222222)
            """
        )
    }

    func testBlankTextYieldsEmbedsOnly() {
        XCTAssertEqual(
            SteerImageMessage.build(text: "  \n ", attachmentIds: [idA]),
            "![image](/api/attachments/11111111-1111-4111-8111-111111111111)"
        )
    }

    func testNoImagesTrimsTheText() {
        XCTAssertEqual(SteerImageMessage.build(text: "  hello  ", attachmentIds: []), "hello")
    }

    func testEmptyTextAndNoImagesIsEmpty() {
        XCTAssertEqual(SteerImageMessage.build(text: "", attachmentIds: []), "")
    }

    func testMaxImagesMatchesTheOtherClients() {
        XCTAssertEqual(SteerImageMessage.maxImages, 4)
    }
}

// EXP-698: the positional `[Image #N]` markers. Same fixture rule — these
// cases mirror `steer-image-message.test.ts` and the Android twin.
final class SteerImageMarkerTests: XCTestCase {
    private let idA = "11111111-1111-4111-8111-111111111111"
    private let idB = "22222222-2222-4222-8222-222222222222"

    // MARK: - insertImageMarker

    func testInsertSpacesTheMarkerOffTheTextItLandsAgainst() {
        let result = SteerImageMessage.insertImageMarker(text: "crop", caret: 4, index: 1)
        XCTAssertEqual(result.text, "crop [Image #1]")
        XCTAssertEqual(result.caret, 15)
    }

    func testInsertMidTextGetsOneSpaceOnEachSide() {
        let result = SteerImageMessage.insertImageMarker(text: "crop this", caret: 5, index: 2)
        XCTAssertEqual(result.text, "crop [Image #2] this")
        // Behind the trailing space, ready for more typing.
        XCTAssertEqual(result.caret, 16)
    }

    func testInsertAddsNoSpaceWhereOneIsAlreadyThere() {
        XCTAssertEqual(
            SteerImageMessage.insertImageMarker(text: "crop ", caret: 5, index: 1).text,
            "crop [Image #1]"
        )
        XCTAssertEqual(
            SteerImageMessage.insertImageMarker(text: " this", caret: 0, index: 1).text,
            "[Image #1] this"
        )
    }

    func testInsertStandsAloneInAnEmptyDraft() {
        let result = SteerImageMessage.insertImageMarker(text: "", caret: 0, index: 1)
        XCTAssertEqual(result.text, "[Image #1]")
        XCTAssertEqual(result.caret, 10)
    }

    func testInsertClampsAnOutOfRangeCaretToTheEnd() {
        XCTAssertEqual(
            SteerImageMessage.insertImageMarker(text: "crop", caret: 99, index: 1).text,
            "crop [Image #1]"
        )
    }

    // MARK: - renumberImageMarkers

    func testRenumberDropsTheRemovedMarkerAndSlidesTheHigherOnesDown() {
        XCTAssertEqual(
            SteerImageMessage.renumberImageMarkers(
                "crop [Image #1] and [Image #2] and [Image #3]", removedIndex: 2
            ),
            "crop [Image #1] and and [Image #2]"
        )
    }

    func testRenumberTidiesTheGapTheDroppedMarkerLeft() {
        XCTAssertEqual(
            SteerImageMessage.renumberImageMarkers("crop [Image #1] please", removedIndex: 1),
            "crop please"
        )
        XCTAssertEqual(
            SteerImageMessage.renumberImageMarkers("crop [Image #1]", removedIndex: 1),
            "crop"
        )
        XCTAssertEqual(
            SteerImageMessage.renumberImageMarkers("[Image #1] crop", removedIndex: 1),
            "crop"
        )
    }

    func testRenumberLeavesLowerMarkersAndUntouchedLinesAlone() {
        XCTAssertEqual(
            SteerImageMessage.renumberImageMarkers(
                "[Image #1]  keep\ncrop [Image #3]", removedIndex: 2
            ),
            "[Image #1]  keep\ncrop [Image #2]"
        )
    }

    func testRenumberRemovesEveryOccurrenceOfTheSameMarker() {
        XCTAssertEqual(
            SteerImageMessage.renumberImageMarkers("a [Image #2] b [Image #2] c", removedIndex: 2),
            "a b c"
        )
    }

    // MARK: - parse

    func testParseSplitsTheProseFromTheTrailingEmbeds() {
        let parsed = SteerImageMessage.parse(
            SteerImageMessage.build(text: "fix [Image #1]", attachmentIds: [idA, idB])
        )
        XCTAssertEqual(parsed.text, "fix [Image #1]")
        XCTAssertEqual(parsed.attachmentIds, [idA, idB])
        XCTAssertEqual(parsed.markers, [1])
    }

    func testParseReadsEmbedsSentWithoutText() {
        let parsed = SteerImageMessage.parse(
            SteerImageMessage.build(text: "", attachmentIds: [idA])
        )
        XCTAssertEqual(parsed.text, "")
        XCTAssertEqual(parsed.attachmentIds, [idA])
        XCTAssertEqual(parsed.markers, [])
    }

    func testParseLeavesAPlainMessageUntouched() {
        let parsed = SteerImageMessage.parse("just words")
        XCTAssertEqual(parsed.text, "just words")
        XCTAssertEqual(parsed.attachmentIds, [])
        XCTAssertEqual(parsed.markers, [])
    }

    func testParseReportsMarkersInTextOrderDeduped() {
        XCTAssertEqual(
            SteerImageMessage.parse("[Image #2] then [Image #1] then [Image #2]").markers,
            [2, 1]
        )
    }

    // EXP-698 review: the pattern is ASCII-only and a number nobody can
    // resolve stays prose — both are places ICU/NSRegularExpression would
    // otherwise drift away from the JS reference.

    func testNonAsciiDigitsAreNotAMarker() {
        // ICU's `\d` matches Arabic-Indic digits; JS's does not, so `[0-9]`
        // is what keeps the two clients agreeing.
        let arabicIndic = "[Image #\u{0662}]"
        XCTAssertEqual(SteerImageMessage.markers(in: arabicIndic), [])
        XCTAssertEqual(SteerImageMessage.segments(of: arabicIndic), [.text(arabicIndic)])
    }

    func testANumberTooBigForAnIntStaysProse() {
        let absurd = "crop [Image #99999999999999999999999] please"
        XCTAssertEqual(SteerImageMessage.markers(in: absurd), [])
        XCTAssertEqual(SteerImageMessage.segments(of: absurd), [.text(absurd)])
        XCTAssertEqual(SteerImageMessage.renumberImageMarkers(absurd, removedIndex: 1), absurd)
    }

    func testTrailingCarriageReturnsStillCloseTheEmbedBlock() {
        // A CRLF transcript leaves a `\r` on each line; JS `trim()` eats it and
        // so must ours, or the embeds read as prose.
        let parsed = SteerImageMessage.parse(
            "fix it\r\n\r\n![image](/api/attachments/\(idA))\r\n"
        )
        XCTAssertEqual(parsed.text, "fix it")
        XCTAssertEqual(parsed.attachmentIds, [idA])
    }

    func testSegmentsSplitTheProseAroundEveryMarker() {
        XCTAssertEqual(
            SteerImageMessage.segments(of: "crop [Image #2] tighter"),
            [.text("crop "), .marker(2), .text(" tighter")]
        )
    }

    func testTheMarkerBuilderMatchesThePattern() {
        XCTAssertEqual(SteerImageMessage.imageMarker(3), "[Image #3]")
        XCTAssertEqual(SteerImageMessage.parse(SteerImageMessage.imageMarker(3)).markers, [3])
    }
}

// Wave D: "Add file or image" takes ANY file ×4. Byte-identical cases in
// steer-image-message.test.ts (web), image_message.rs (desktop) and
// SteerImageMessageTest.kt (Android): change all four together.
final class SteerFileMessageTests: XCTestCase {
    private let idA = "11111111-1111-4111-8111-111111111111"
    private let idB = "22222222-2222-4222-8222-222222222222"
    private let idC = "33333333-3333-4333-8333-333333333333"

    // MARK: - buildMessage

    func testPutsOneFileLinkPerLineAfterTheImageEmbeds() {
        XCTAssertEqual(
            SteerImageMessage.buildMessage(
                text: "fix the header", imageIds: [idA], files: [.init(id: idB, name: "notes.pdf")]
            ),
            "fix the header\n\n![image](/api/attachments/\(idA))\n[notes.pdf](/api/attachments/\(idB))"
        )
    }

    func testSeparatesFilesFromTheProseByABlankLineWithoutImages() {
        XCTAssertEqual(
            SteerImageMessage.buildMessage(
                text: "see the log",
                imageIds: [],
                files: [.init(id: idB, name: "build.log"), .init(id: idC, name: "trace.zip")]
            ),
            "see the log\n\n[build.log](/api/attachments/\(idB))\n[trace.zip](/api/attachments/\(idC))"
        )
    }

    func testSendsFilesAloneWhenTheTextIsWhitespace() {
        XCTAssertEqual(
            SteerImageMessage.buildMessage(text: "  ", imageIds: [], files: [.init(id: idB, name: "notes.pdf")]),
            "[notes.pdf](/api/attachments/\(idB))"
        )
    }

    func testEscapesAClosingBracketAndABackslashInTheName() {
        XCTAssertEqual(SteerImageMessage.escapeFileName("a]b\\c.txt"), "a\\]b\\\\c.txt")
        XCTAssertEqual(
            SteerImageMessage.buildMessage(text: "", imageIds: [], files: [.init(id: idC, name: "a]b\\c.txt")]),
            "[a\\]b\\\\c.txt](/api/attachments/\(idC))"
        )
    }

    func testEqualsTheImageBuilderWithoutFiles() {
        XCTAssertEqual(
            SteerImageMessage.buildMessage(text: "fix", imageIds: [idA, idB], files: []),
            SteerImageMessage.build(text: "fix", attachmentIds: [idA, idB])
        )
        XCTAssertEqual(SteerImageMessage.buildMessage(text: "", imageIds: [], files: []), "")
    }

    func testCapsAtFourFiles() {
        XCTAssertEqual(SteerImageMessage.maxFiles, 4)
    }

    func testTheCopyIsTheContracts() {
        XCTAssertEqual(
            SteerImageMessage.attachmentRejectedMessage,
            "Images up to 10 MB and files up to 50 MB can be attached"
        )
        XCTAssertEqual(SteerImageMessage.filesFullMessage, "Up to 4 files per message")
    }

    // MARK: - parse

    func testPeelsTheFileLinesBeforeTheImageEmbeds() {
        XCTAssertEqual(
            SteerImageMessage.parse(
                "fix [Image #1]\n\n![image](/api/attachments/\(idA))\n[notes.pdf](/api/attachments/\(idB))"
            ),
            SteerImageMessage.Parsed(
                text: "fix [Image #1]",
                attachmentIds: [idA],
                markers: [1],
                files: [.init(id: idB, name: "notes.pdf")]
            )
        )
    }

    func testReadsFilesWithNoImagesAndNoText() {
        XCTAssertEqual(
            SteerImageMessage.parse(
                "[build.log](/api/attachments/\(idB))\n[trace.zip](/api/attachments/\(idC))"
            ),
            SteerImageMessage.Parsed(
                text: "",
                attachmentIds: [],
                markers: [],
                files: [.init(id: idB, name: "build.log"), .init(id: idC, name: "trace.zip")]
            )
        )
    }

    func testUnescapesTheName() {
        XCTAssertEqual(
            SteerImageMessage.parse("[a\\]b\\\\c.txt](/api/attachments/\(idC))").files,
            [.init(id: idC, name: "a]b\\c.txt")]
        )
    }

    func testNeverReadsAnImageEmbedAsAFile() {
        XCTAssertEqual(SteerImageMessage.parse("![image](/api/attachments/\(idA))").files, [])
    }

    func testRoundTripsTheBuilder() {
        let files: [SteerImageMessage.File] = [
            .init(id: idB, name: "a]b\\c.txt"),
            .init(id: idC, name: "report (final).pdf"),
        ]
        XCTAssertEqual(
            SteerImageMessage.parse(
                SteerImageMessage.buildMessage(text: "look [Image #1]", imageIds: [idA], files: files)
            ),
            SteerImageMessage.Parsed(text: "look [Image #1]", attachmentIds: [idA], markers: [1], files: files)
        )
    }

    // iOS extras.

    func testAnImagesOnlyMessageParsesExactlyAsBefore() {
        XCTAssertEqual(
            SteerImageMessage.parse(SteerImageMessage.build(text: "crop it", attachmentIds: [idA, idB])),
            SteerImageMessage.Parsed(text: "crop it", attachmentIds: [idA, idB], markers: [], files: [])
        )
    }

    func testAFileLineAboveTheImagesIsProse() {
        let parsed = SteerImageMessage.parse(
            "see\n[notes.txt](/api/attachments/\(idB))\n![image](/api/attachments/\(idA))"
        )
        XCTAssertEqual(parsed.text, "see\n[notes.txt](/api/attachments/\(idB))")
        XCTAssertEqual(parsed.attachmentIds, [idA])
        XCTAssertEqual(parsed.files, [])
    }
}

import XCTest
@testable import ExpCore

/// EXP-297 — the attachment classification contract. A row is an INLINE IMAGE
/// iff its content type is one of the five raster types the markdown pipeline
/// accepts; everything else (including other `image/*` types) is a file and
/// belongs in the issue's Files section.
final class AttachmentFilesTests: XCTestCase {
    func testInlineImageSetIsExactlyTheFiveAcceptedTypes() {
        XCTAssertEqual(
            AttachmentFiles.inlineImageContentTypes,
            ["image/png", "image/jpeg", "image/webp", "image/gif", "image/avif"]
        )
    }

    func testInlineImageClassification() {
        for type in ["image/png", "image/jpeg", "image/webp", "image/gif", "image/avif"] {
            XCTAssertTrue(AttachmentFiles.isInlineImage(contentType: type), type)
        }
        // Other image types are deliberately files — no invisible gap.
        for type in ["image/tiff", "image/svg+xml", "image/heic"] {
            XCTAssertFalse(AttachmentFiles.isInlineImage(contentType: type), type)
        }
        for type in ["application/pdf", "application/zip", "video/mp4", "text/plain", ""] {
            XCTAssertFalse(AttachmentFiles.isInlineImage(contentType: type), type)
        }
    }

    func testInlineImageClassificationIsAnExactMatchLikeEveryOtherClient() {
        // Non-canonical stored types are Files rows on server/web/desktop —
        // classifying them inline here would hide them on iOS only.
        XCTAssertFalse(AttachmentFiles.isInlineImage(contentType: "IMAGE/PNG"))
        XCTAssertFalse(AttachmentFiles.isInlineImage(contentType: "image/jpeg; charset=binary"))
    }

    // EXP-824: `video/*` and `audio/*` are the inline MEDIA class — embedded
    // as a plain link and rendered as a player; they leave the Files rail
    // like inline images. Prefix match on the canonical essence (mirrors the
    // server's isVideoContentType / isAudioContentType).
    func testInlineMediaClassification() {
        for type in ["video/mp4", "video/quicktime", "video/webm", "VIDEO/MP4", "video/mp4; codecs=avc1"] {
            XCTAssertTrue(AttachmentFiles.isInlineVideo(contentType: type), type)
            XCTAssertTrue(AttachmentFiles.isInlineMedia(contentType: type), type)
            XCTAssertFalse(AttachmentFiles.isInlineAudio(contentType: type), type)
            XCTAssertFalse(AttachmentFiles.isFile(contentType: type), type)
        }
        for type in ["audio/mpeg", "audio/mp4", "audio/x-m4a", "AUDIO/OGG"] {
            XCTAssertTrue(AttachmentFiles.isInlineAudio(contentType: type), type)
            XCTAssertTrue(AttachmentFiles.isInlineMedia(contentType: type), type)
            XCTAssertFalse(AttachmentFiles.isInlineVideo(contentType: type), type)
            XCTAssertFalse(AttachmentFiles.isFile(contentType: type), type)
        }
        for type in ["image/png", "application/pdf", "text/plain", "", "videox/mp4"] {
            XCTAssertFalse(AttachmentFiles.isInlineMedia(contentType: type), type)
        }
        // Inline images stay the EXACT five-type match — media never widens it.
        XCTAssertFalse(AttachmentFiles.isInlineImage(contentType: "video/mp4"))
        XCTAssertTrue(AttachmentFiles.isFile(contentType: "application/pdf"))
        XCTAssertFalse(AttachmentFiles.isFile(contentType: "image/png"))
    }

    func testMaxPosterUploadBytesIs2MB() {
        XCTAssertEqual(AttachmentFiles.maxPosterUploadBytes, 2_097_152)
    }

    func testCanonicalContentTypeNormalizesPickerTypes() {
        XCTAssertEqual(AttachmentFiles.canonicalContentType("IMAGE/PNG"), "image/png")
        XCTAssertEqual(AttachmentFiles.canonicalContentType("image/jpeg; charset=binary"), "image/jpeg")
        XCTAssertEqual(AttachmentFiles.canonicalContentType("  image/webp  "), "image/webp")
        XCTAssertEqual(AttachmentFiles.canonicalContentType(nil), "application/octet-stream")
        XCTAssertEqual(AttachmentFiles.canonicalContentType("  ;foo=bar"), "application/octet-stream")
    }

    func testMaxFileUploadBytesIs50MB() {
        XCTAssertEqual(AttachmentFiles.maxFileUploadBytes, 52_428_800)
    }

    func testSymbolPerContentType() {
        XCTAssertEqual(AttachmentFiles.sfSymbolName(forContentType: "application/pdf"), "doc.richtext")
        XCTAssertEqual(AttachmentFiles.sfSymbolName(forContentType: "application/zip"), "doc.zipper")
        XCTAssertEqual(AttachmentFiles.sfSymbolName(forContentType: "application/x-7z-compressed"), "doc.zipper")
        XCTAssertEqual(AttachmentFiles.sfSymbolName(forContentType: "video/quicktime"), "film")
        XCTAssertEqual(AttachmentFiles.sfSymbolName(forContentType: "audio/mpeg"), "waveform")
        XCTAssertEqual(AttachmentFiles.sfSymbolName(forContentType: "text/csv"), "doc.text")
        XCTAssertEqual(AttachmentFiles.sfSymbolName(forContentType: "application/octet-stream"), "doc")
        XCTAssertEqual(AttachmentFiles.sfSymbolName(forContentType: ""), "doc")
    }

    func testSanitizedFilenameCannotEscapeItsFolder() {
        XCTAssertEqual(AttachmentFiles.sanitizedFilename("report.pdf"), "report.pdf")
        XCTAssertEqual(AttachmentFiles.sanitizedFilename("../../etc/passwd"), ".._.._etc_passwd")
        XCTAssertEqual(AttachmentFiles.sanitizedFilename("a/b\\c:d.txt"), "a_b_c_d.txt")
        XCTAssertEqual(AttachmentFiles.sanitizedFilename("   "), "file")
        XCTAssertEqual(AttachmentFiles.sanitizedFilename(".."), "file")
        XCTAssertEqual(AttachmentFiles.sanitizedFilename("line\nbreak.txt"), "line_break.txt")
        XCTAssertLessThanOrEqual(
            AttachmentFiles.sanitizedFilename(String(repeating: "x", count: 400)).count,
            120
        )
    }

    // MARK: - EXP-1003 markdown preview

    /// The web's `attachment-files.test.ts` table, mirrored ×4.
    func testIsMarkdownMirrorsTheWebTable() {
        let cases: [(String, String, Bool)] = [
            ("text/markdown", "notes.md", true),
            ("text/markdown; charset=utf-8", "x", true),
            ("text/x-markdown", "x.txt", true),
            ("", "README.md", true),
            ("application/octet-stream", "a.MD", true),
            ("text/plain", "spec.markdown", true),
            ("text/plain", "spec.txt", false),
            ("application/pdf", "spec.md", false),
            ("image/png", "x.md", false),
            ("text/csv", "x.md", false),
        ]
        for (contentType, filename, expected) in cases {
            XCTAssertEqual(
                AttachmentFiles.isMarkdown(contentType: contentType, filename: filename),
                expected,
                "\(contentType) / \(filename)"
            )
        }
    }

    func testIsMarkdownTrimsTheFilenameAndLowercasesTheType() {
        XCTAssertTrue(AttachmentFiles.isMarkdown(contentType: "", filename: "  notes.md  "))
        XCTAssertTrue(AttachmentFiles.isMarkdown(contentType: "TEXT/MARKDOWN", filename: "x"))
        XCTAssertTrue(AttachmentFiles.isMarkdown(contentType: " Text/Plain ", filename: "a.Markdown"))
        XCTAssertFalse(AttachmentFiles.isMarkdown(contentType: "", filename: "md"))
    }

    func testMarkdownPreviewCeilingIsOneMebibyte() {
        XCTAssertEqual(AttachmentFiles.markdownPreviewMaxBytes, 1_048_576)
    }

    func testMarkdownPreviewSkipsFetchOnlyAboveTheCeiling() {
        let max = AttachmentFiles.markdownPreviewMaxBytes
        XCTAssertFalse(AttachmentFiles.markdownPreviewSkipsFetch(sizeBytes: max))
        XCTAssertTrue(AttachmentFiles.markdownPreviewSkipsFetch(sizeBytes: max + 1))
        XCTAssertFalse(AttachmentFiles.markdownPreviewSkipsFetch(sizeBytes: 0))
    }

    func testMarkdownPreviewOutcomeMeasuresTheFetchedText() {
        let max = AttachmentFiles.markdownPreviewMaxBytes
        let atMax = String(repeating: "a", count: max)
        XCTAssertEqual(
            AttachmentFiles.markdownPreviewOutcome(data: Data(atMax.utf8)),
            .ready(atMax)
        )
        XCTAssertEqual(
            AttachmentFiles.markdownPreviewOutcome(data: Data(String(repeating: "a", count: max + 1).utf8)),
            .tooLarge
        )
        XCTAssertEqual(
            AttachmentFiles.markdownPreviewOutcome(data: Data("# Hi".utf8)),
            .ready("# Hi")
        )
    }

    func testMarkdownPreviewOutcomeDecodesInvalidUtf8Lossily() {
        XCTAssertEqual(
            AttachmentFiles.markdownPreviewOutcome(data: Data([0x61, 0xFF, 0x62])),
            .ready("a\u{FFFD}b")
        )
    }

    func testMarkdownPreviewErrorMessagesMatchTheWebCopy() {
        XCTAssertEqual(
            AttachmentFiles.markdownPreviewErrorMessage(AttachmentsError.httpError(404, "")),
            "This file is no longer available."
        )
        XCTAssertEqual(
            AttachmentFiles.markdownPreviewErrorMessage(AttachmentsError.httpError(500, "boom")),
            "Couldn't load this file (HTTP 500)."
        )
        XCTAssertEqual(
            AttachmentFiles.markdownPreviewErrorMessage(AttachmentsError.invalidUrl),
            "Invalid attachment URL"
        )
    }

    func testMarkdownPreviewSubtitle() {
        XCTAssertEqual(AttachmentFiles.markdownPreviewSubtitle(sizeBytes: 0), "Markdown")
        XCTAssertEqual(
            AttachmentFiles.markdownPreviewSubtitle(sizeBytes: 2048),
            "Markdown · \(Int64(2048).formatted(.byteCount(style: .file)))"
        )
    }

    // EXP-1247: the row-aware rule — an `asFile` row is never inline, so it
    // always lists under Files (web `attachment-files.test.ts`).
    func testAnAsFileRowIsAFileAndNeverInline() {
        XCTAssertTrue(AttachmentFiles.isFile(contentType: "image/png", asFile: true))
        XCTAssertFalse(AttachmentFiles.isInlineImage(contentType: "image/png", asFile: true))
        XCTAssertFalse(AttachmentFiles.isInlineMedia(contentType: "video/mp4", asFile: true))
        XCTAssertTrue(AttachmentFiles.isFile(contentType: "video/mp4", asFile: true))
        // Without the marker the content type decides, as before.
        XCTAssertFalse(AttachmentFiles.isFile(contentType: "image/png", asFile: false))
        XCTAssertTrue(AttachmentFiles.isInlineImage(contentType: "image/png", asFile: false))
        XCTAssertTrue(AttachmentFiles.isFile(contentType: "application/pdf", asFile: false))
        let draftRow = DraftAttachmentDto(
            id: "a1", filename: "shot.png", contentType: "image/png", sizeBytes: 1, url: "/api/attachments/a1",
            asFile: true
        )
        XCTAssertTrue(AttachmentFiles.isFile(draftRow))
        let inlineRow = DraftAttachmentDto(
            id: "a2", filename: "shot.png", contentType: "image/png", sizeBytes: 1, url: "/api/attachments/a2"
        )
        XCTAssertFalse(AttachmentFiles.isFile(inlineRow))
    }

    func testAFileButtonUploadMarksItsRoute() {
        XCTAssertEqual(AttachmentsApi.withAsFile("/api/issues/i1/files", true), "/api/issues/i1/files?asFile=1")
        XCTAssertEqual(AttachmentsApi.withAsFile("/api/issues/i1/files", false), "/api/issues/i1/files")
    }

    func testTheDraftRowDecodesAsFileTolerantly() throws {
        let marked = try JSONDecoder().decode(
            DraftAttachmentDto.self,
            from: Data(#"{"id":"a","filename":"x.png","contentType":"image/png","sizeBytes":1,"url":"/u","asFile":true}"#.utf8)
        )
        XCTAssertTrue(marked.asFile)
        let legacy = try JSONDecoder().decode(
            DraftAttachmentDto.self,
            from: Data(#"{"id":"a","filename":"x.png","contentType":"image/png","sizeBytes":1,"url":"/u"}"#.utf8)
        )
        XCTAssertFalse(legacy.asFile)
    }
}

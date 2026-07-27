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
}

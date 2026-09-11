import XCTest
@testable import ExpCore

/// EXP-824 — the multipart layout of a media upload: the `file` part first
/// (the EXP-61 quoted-disposition contract, unchanged), then an optional
/// `poster` image part and the plain `width`/`height`/`durationMs` fields the
/// server reads as bounded ints. Non-media uploads emit the file part alone,
/// byte-identical to before.
final class AttachmentUploadBodyTests: XCTestCase {
    private func body(_ media: MediaUploadParts?, filename: String = "clip.mp4") -> String {
        let data = AttachmentsApi.multipartBody(
            boundary: "B",
            data: Data("VIDEO".utf8),
            filename: filename,
            contentType: "video/mp4",
            media: media
        )
        return String(decoding: data, as: UTF8.self)
    }

    func testFileOnlyBodyIsUnchanged() {
        XCTAssertEqual(
            body(nil),
            "--B\r\nContent-Disposition: form-data; name=\"file\"; filename=\"clip.mp4\"\r\n"
                + "Content-Type: video/mp4\r\n\r\nVIDEO\r\n--B--\r\n"
        )
        // An empty parts struct adds nothing either.
        XCTAssertEqual(body(MediaUploadParts()), body(nil))
    }

    func testMediaPartsFollowTheFilePart() {
        let text = body(MediaUploadParts(
            poster: Data("JPEG".utf8), width: 1280, height: 720, durationMs: 7250
        ))
        XCTAssertEqual(
            text,
            "--B\r\nContent-Disposition: form-data; name=\"file\"; filename=\"clip.mp4\"\r\n"
                + "Content-Type: video/mp4\r\n\r\nVIDEO"
                + "\r\n--B\r\nContent-Disposition: form-data; name=\"poster\"; filename=\"poster.jpg\"\r\n"
                + "Content-Type: image/jpeg\r\n\r\nJPEG"
                + "\r\n--B\r\nContent-Disposition: form-data; name=\"width\"\r\n\r\n1280"
                + "\r\n--B\r\nContent-Disposition: form-data; name=\"height\"\r\n\r\n720"
                + "\r\n--B\r\nContent-Disposition: form-data; name=\"durationMs\"\r\n\r\n7250"
                + "\r\n--B--\r\n"
        )
    }

    func testNonPositiveFieldsAndEmptyPosterAreOmitted() {
        let text = body(MediaUploadParts(poster: Data(), width: 0, height: -1, durationMs: 5000))
        XCTAssertFalse(text.contains("name=\"poster\""))
        XCTAssertFalse(text.contains("name=\"width\""))
        XCTAssertFalse(text.contains("name=\"height\""))
        XCTAssertTrue(text.contains("name=\"durationMs\"\r\n\r\n5000"))
    }

    func testFilenameIsSanitizedInTheDisposition() {
        XCTAssertTrue(body(nil, filename: "a\"b\\c\r\nd.mp4").contains("filename=\"a_b_c__d.mp4\""))
    }
}

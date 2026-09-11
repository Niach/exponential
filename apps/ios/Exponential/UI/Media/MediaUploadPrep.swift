import AVFoundation
import CoreMedia
import CoreTransferable
import ExpCore
import ExpUI
import Foundation
import UIKit
import UniformTypeIdentifiers
import os

private let log = Logger(subsystem: "com.exponential", category: "MediaUploadPrep")

/// EXP-824 — turn a picked video/audio file into the `PendingImage` the
/// editor queues: video is normalised to H.264 + AAC MP4 CLAMPED TO 720p
/// (`AVAssetExportPreset1280x720`; passthrough only when the source already
/// is H.264/AAC MP4 at ≤720p), a JPEG poster is generated from the OUTPUT
/// (rotation applied), and the rotation-aware size + duration are probed for
/// the multipart fields. Audio uploads as-is with its duration probed. The
/// 50 MB cap is enforced AFTER export — the whole point of the transcode is
/// that a phone recording lands under it.
enum MediaUploadPrep {
    enum PrepError: Error, LocalizedError {
        case unreadable
        case exportFailed
        case tooLarge

        var errorDescription: String? {
            switch self {
            case .unreadable: "Couldn't read this file."
            case .exportFailed: "Couldn't process this video."
            case .tooLarge: "Videos must be 50 MB or smaller after compression."
            }
        }
    }

    /// Longest side / shortest side of "720p" — a 1280×720 landscape or a
    /// 720×1280 portrait clip both pass through untouched.
    private static let maxLongSide = 1280
    private static let maxShortSide = 720

    /// Probed facts about a video track, rotation applied.
    struct VideoProbe: Sendable {
        var width: Int?
        var height: Int?
        var durationMs: Int?
        var isH264: Bool
        var audioIsAacOrAbsent: Bool
    }

    /// Prepare the file at `fileURL` (a temp copy the caller owns; it is left
    /// in place) for upload. `contentType` is the canonical picker type.
    nonisolated static func prepare(
        fileURL: URL,
        filename: String,
        contentType: String
    ) async throws -> PendingImage {
        if AttachmentFiles.isInlineAudio(contentType: contentType) {
            return try await prepareAudio(fileURL: fileURL, filename: filename, contentType: contentType)
        }
        return try await prepareVideo(fileURL: fileURL, filename: filename, contentType: contentType)
    }

    // MARK: - Audio

    private nonisolated static func prepareAudio(
        fileURL: URL, filename: String, contentType: String
    ) async throws -> PendingImage {
        guard let data = try? Data(contentsOf: fileURL) else { throw PrepError.unreadable }
        guard data.count <= AttachmentFiles.maxFileUploadBytes else { throw PrepError.tooLarge }
        let asset = AVURLAsset(url: fileURL)
        let durationMs = (try? await asset.load(.duration)).flatMap(milliseconds)
        return PendingImage(
            data: data, filename: filename, contentType: contentType, durationMs: durationMs
        )
    }

    // MARK: - Video

    private nonisolated static func prepareVideo(
        fileURL: URL, filename: String, contentType: String
    ) async throws -> PendingImage {
        let source = AVURLAsset(url: fileURL)
        let probe = try await probeVideo(source)

        let outputURL: URL
        let outputName: String
        let outputType: String
        if canPassThrough(probe, contentType: contentType, fileURL: fileURL) {
            outputURL = fileURL
            outputName = filename
            outputType = "video/mp4"
        } else {
            outputURL = try await export(source)
            outputName = mp4Filename(filename)
            outputType = "video/mp4"
        }
        defer { if outputURL != fileURL { try? FileManager.default.removeItem(at: outputURL) } }

        guard let data = try? Data(contentsOf: outputURL) else { throw PrepError.unreadable }
        guard data.count <= AttachmentFiles.maxFileUploadBytes else { throw PrepError.tooLarge }

        // Poster + size from the OUTPUT, so a transcoded portrait clip's
        // poster and dimensions are the rotated, clamped ones the row keeps.
        let output = outputURL == fileURL ? source : AVURLAsset(url: outputURL)
        let final = (try? await probeVideo(output)) ?? probe
        let poster = await poster(for: output, durationMs: final.durationMs)

        return PendingImage(
            data: data,
            filename: outputName,
            contentType: outputType,
            width: final.width,
            height: final.height,
            durationMs: final.durationMs,
            poster: poster
        )
    }

    nonisolated static func probeVideo(_ asset: AVURLAsset) async throws -> VideoProbe {
        let duration = try await asset.load(.duration)
        let videoTracks = try await asset.loadTracks(withMediaType: .video)
        let audioTracks = try await asset.loadTracks(withMediaType: .audio)
        guard let video = videoTracks.first else { throw PrepError.unreadable }
        let (naturalSize, transform, formats) = try await video.load(.naturalSize, .preferredTransform, .formatDescriptions)
        let size = rotatedSize(naturalSize, transform: transform)
        let isH264 = formats.contains { CMFormatDescriptionGetMediaSubType($0) == kCMVideoCodecType_H264 }
        var audioOk = true
        if let audio = audioTracks.first {
            let audioFormats = (try? await audio.load(.formatDescriptions)) ?? []
            audioOk = audioFormats.contains { CMFormatDescriptionGetMediaSubType($0) == kAudioFormatMPEG4AAC }
        }
        return VideoProbe(
            width: size.width > 0 ? size.width : nil,
            height: size.height > 0 ? size.height : nil,
            durationMs: milliseconds(duration),
            isH264: isH264,
            audioIsAacOrAbsent: audioOk
        )
    }

    /// Rotation-aware pixel size: `naturalSize` is the encoded frame, the
    /// preferred transform is what a portrait recording carries instead of
    /// rotated pixels (`appliesPreferredTrackTransform` on the generator does
    /// the same for the poster).
    static func rotatedSize(_ naturalSize: CGSize, transform: CGAffineTransform) -> (width: Int, height: Int) {
        let rect = CGRect(origin: .zero, size: naturalSize).applying(transform)
        return (Int(abs(rect.width).rounded()), Int(abs(rect.height).rounded()))
    }

    /// Already H.264 + AAC (or silent) MP4 at ≤720p: send the bytes as they
    /// are. Anything else (HEVC from an iPhone, a .mov container, 1080p/4K) is
    /// exported.
    static func canPassThrough(_ probe: VideoProbe, contentType: String, fileURL: URL) -> Bool {
        guard probe.isH264, probe.audioIsAacOrAbsent else { return false }
        let ext = fileURL.pathExtension.lowercased()
        guard contentType == "video/mp4" || ext == "mp4" || ext == "m4v" else { return false }
        guard let width = probe.width, let height = probe.height else { return false }
        return max(width, height) <= maxLongSide && min(width, height) <= maxShortSide
    }

    private nonisolated static func export(_ asset: AVURLAsset) async throws -> URL {
        guard let session = AVAssetExportSession(asset: asset, presetName: AVAssetExportPreset1280x720) else {
            throw PrepError.exportFailed
        }
        let outputURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("exp-media-\(UUID().uuidString)")
            .appendingPathExtension("mp4")
        session.shouldOptimizeForNetworkUse = true
        if #available(iOS 18, *) {
            do {
                try await session.export(to: outputURL, as: .mp4)
            } catch {
                log.error("Video export failed: \(error.localizedDescription, privacy: .public)")
                throw PrepError.exportFailed
            }
        } else {
            session.outputURL = outputURL
            session.outputFileType = .mp4
            await session.export()
            guard session.status == .completed else {
                log.error("Video export failed: \(session.error?.localizedDescription ?? "unknown", privacy: .public)")
                throw PrepError.exportFailed
            }
        }
        return outputURL
    }

    /// A JPEG poster (≤ `AttachmentFiles.maxPosterUploadBytes`) from an early
    /// frame — 10 % in, capped at one second, so a fade-in still shows
    /// something. Nil when the frame can't be read; the block then paints a
    /// neutral box and the server stores no poster.
    private nonisolated static func poster(for asset: AVURLAsset, durationMs: Int?) async -> Data? {
        let generator = AVAssetImageGenerator(asset: asset)
        generator.appliesPreferredTrackTransform = true
        generator.maximumSize = CGSize(width: 1280, height: 1280)
        let seconds = min(1.0, Double(durationMs ?? 0) / 1000 * 0.1)
        let time = CMTime(seconds: seconds, preferredTimescale: 600)
        guard let frame = try? await generator.image(at: time) else { return nil }
        let image = UIImage(cgImage: frame.image)
        for quality in [0.8, 0.6, 0.4, 0.25] {
            if let jpeg = image.jpegData(compressionQuality: quality),
               jpeg.count <= AttachmentFiles.maxPosterUploadBytes {
                return jpeg
            }
        }
        return nil
    }

    private static func milliseconds(_ time: CMTime) -> Int? {
        let seconds = time.seconds
        guard seconds.isFinite, seconds > 0 else { return nil }
        return Int((seconds * 1000).rounded())
    }

    static func mp4Filename(_ filename: String) -> String {
        let stem = (filename as NSString).deletingPathExtension
        return (stem.isEmpty ? "clip" : stem) + ".mp4"
    }

    // MARK: - Picker plumbing

    /// A picked media file, copied into the app's temp folder so the security
    /// scope of a Files pick (or the photo library's transient export) can end
    /// before the (slow) export runs. The caller deletes it when done.
    static func copyToTemp(_ url: URL) throws -> URL {
        let destination = FileManager.default.temporaryDirectory
            .appendingPathComponent("exp-pick-\(UUID().uuidString)")
            .appendingPathExtension(url.pathExtension.isEmpty ? "bin" : url.pathExtension)
        try FileManager.default.copyItem(at: url, to: destination)
        return destination
    }

    /// Whether a picker item's types say video/audio — routes the pick to
    /// the media path instead of the image one.
    static func isMedia(_ types: [UTType]) -> Bool {
        types.contains { $0.conforms(to: .movie) || $0.conforms(to: .audio) }
    }
}

/// EXP-824 — the `Transferable` a `PhotosPickerItem` video loads as: a FILE,
/// not `Data` (a 4K clip must never be buffered whole before the export
/// shrinks it). The received file is copied to our temp folder because the
/// picker's copy is gone the moment the closure returns.
struct PickedMediaFile: Transferable {
    let url: URL

    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(contentType: .movie) { picked in
            SentTransferredFile(picked.url)
        } importing: { received in
            PickedMediaFile(url: try MediaUploadPrep.copyToTemp(received.file))
        }
    }
}

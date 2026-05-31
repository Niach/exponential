import Foundation
import UIKit
import UniformTypeIdentifiers

/// Pulls images / text / URLs out of a share invocation's `NSExtensionItem`s and
/// shapes them into a [SharedPayload]. Runs on the main actor so the
/// (non-Sendable) `NSExtensionItem`s never cross an isolation boundary.
@MainActor
enum ShareItemExtractor {
    private static let maxImages = 10
    // The server's attachment endpoint only accepts these; anything else
    // (notably HEIC from Photos) is transcoded to JPEG before upload.
    private static let acceptedMime: Set<String> = ["image/png", "image/jpeg", "image/gif", "image/webp"]

    static func extract(from items: [NSExtensionItem]) async -> SharedPayload? {
        var texts: [String] = []
        var webURL: URL?
        var images: [SharedImage] = []

        for item in items {
            for provider in item.attachments ?? [] {
                if images.count < maxImages, provider.hasItemConformingToTypeIdentifier(UTType.image.identifier) {
                    if let image = await loadImage(provider) { images.append(image) }
                } else if provider.hasItemConformingToTypeIdentifier(UTType.url.identifier) {
                    if let url = await loadURL(provider) {
                        if url.isFileURL {
                            if images.count < maxImages, let image = loadImageFromFile(url) { images.append(image) }
                        } else {
                            webURL = url
                        }
                    }
                } else if provider.hasItemConformingToTypeIdentifier(UTType.text.identifier) {
                    if let text = await loadText(provider) { texts.append(text) }
                }
            }
        }

        if texts.isEmpty, webURL == nil, images.isEmpty { return nil }

        let (title, description) = composeText(texts: texts, webURL: webURL, hasImages: !images.isEmpty)
        return SharedPayload(title: title, descriptionText: description, images: images)
    }

    // MARK: - Title / description

    private static func composeText(texts: [String], webURL: URL?, hasImages: Bool) -> (String, String) {
        let joined = texts.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
        let urlString = webURL?.absoluteString

        let firstLine = joined.split(separator: "\n", maxSplits: 1).first
            .map(String.init)?.trimmingCharacters(in: .whitespaces)

        let title: String
        if let firstLine, !firstLine.isEmpty {
            title = String(firstLine.prefix(120))
        } else if let urlString {
            title = urlString
        } else if hasImages {
            title = "Shared image"
        } else {
            title = "Shared"
        }

        var description: String
        if joined == title {
            description = ""
        } else if joined.hasPrefix(title) {
            description = String(joined.dropFirst(title.count)).trimmingCharacters(in: .whitespacesAndNewlines)
        } else {
            description = joined
        }
        if let urlString, urlString != title, !description.contains(urlString) {
            description = description.isEmpty ? urlString : description + "\n\n" + urlString
        }
        return (title, description)
    }

    // MARK: - Loaders

    private static func loadImage(_ provider: NSItemProvider) async -> SharedImage? {
        // Prefer the most specific registered image type to preserve the format.
        let typeId = provider.registeredTypeIdentifiers.first {
            UTType($0)?.conforms(to: .image) == true
        } ?? UTType.image.identifier

        let data: Data? = await withCheckedContinuation { continuation in
            provider.loadDataRepresentation(forTypeIdentifier: typeId) { data, _ in
                continuation.resume(returning: data)
            }
        }
        guard let data else { return nil }
        return normalizeImage(data: data, utType: UTType(typeId) ?? .image, suggestedName: provider.suggestedName)
    }

    private static func loadImageFromFile(_ url: URL) -> SharedImage? {
        guard let data = try? Data(contentsOf: url) else { return nil }
        let utType = UTType(filenameExtension: url.pathExtension) ?? .image
        return normalizeImage(data: data, utType: utType, suggestedName: url.lastPathComponent)
    }

    private static func loadURL(_ provider: NSItemProvider) async -> URL? {
        await withCheckedContinuation { continuation in
            provider.loadItem(forTypeIdentifier: UTType.url.identifier, options: nil) { item, _ in
                continuation.resume(returning: item as? URL)
            }
        }
    }

    private static func loadText(_ provider: NSItemProvider) async -> String? {
        await withCheckedContinuation { continuation in
            provider.loadItem(forTypeIdentifier: UTType.text.identifier, options: nil) { item, _ in
                if let string = item as? String {
                    continuation.resume(returning: string)
                } else if let data = item as? Data, let string = String(data: data, encoding: .utf8) {
                    continuation.resume(returning: string)
                } else if let url = item as? URL {
                    continuation.resume(returning: url.absoluteString)
                } else {
                    continuation.resume(returning: nil)
                }
            }
        }
    }

    /// Ensure the image is in a server-accepted format, transcoding to JPEG if not.
    private static func normalizeImage(data: Data, utType: UTType, suggestedName: String?) -> SharedImage? {
        let base = suggestedName.map { ($0 as NSString).deletingPathExtension }
            .flatMap { $0.isEmpty ? nil : $0 } ?? "shared-image"
        let mime = utType.preferredMIMEType ?? ""

        if acceptedMime.contains(mime) {
            let ext = utType.preferredFilenameExtension ?? "img"
            return SharedImage(data: data, filename: "\(base).\(ext)", contentType: mime)
        }

        // Transcode unsupported formats (e.g. HEIC) to JPEG so the server accepts them.
        guard let image = UIImage(data: data), let jpeg = image.jpegData(compressionQuality: 0.9) else {
            return nil
        }
        return SharedImage(data: jpeg, filename: "\(base).jpg", contentType: "image/jpeg")
    }
}

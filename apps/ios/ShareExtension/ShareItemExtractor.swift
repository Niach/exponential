import Foundation
import UIKit
import UniformTypeIdentifiers

/// Pulls images / files / text / URLs out of a share invocation's `NSExtensionItem`s and
/// shapes them into a [SharedPayload]. Runs on the main actor so the
/// (non-Sendable) `NSExtensionItem`s never cross an isolation boundary.
@MainActor
enum ShareItemExtractor {
    private static let maxImages = 10
    /// The composer's and the issue page's file cap (`SteerImageMessage.maxFiles`).
    private static let maxFiles = 4
    /// The server's file limit (composer-menu.json `add-file`).
    private static let maxFileBytes = 50 * 1024 * 1024
    // The server's attachment endpoint only accepts these; anything else
    // (notably HEIC from Photos) is transcoded to JPEG before upload.
    private static let acceptedMime: Set<String> = ["image/png", "image/jpeg", "image/gif", "image/webp"]

    static func extract(from items: [NSExtensionItem]) async -> SharedPayload? {
        var texts: [String] = []
        var webURL: URL?
        var images: [SharedImage] = []
        var files: [SharedFile] = []
        var subject: String?
        var moreFiles = false

        for item in items {
            if subject == nil {
                subject = [item.attributedTitle?.string, item.attributedContentText?.string]
                    .compactMap { $0?.trimmingCharacters(in: .whitespacesAndNewlines) }
                    .first { !$0.isEmpty }
            }
            for provider in item.attachments ?? [] {
                if images.count < maxImages, provider.hasItemConformingToTypeIdentifier(UTType.image.identifier) {
                    if let image = await loadImage(provider) { images.append(image) }
                } else if provider.hasItemConformingToTypeIdentifier(UTType.url.identifier) {
                    if let url = await loadURL(provider) {
                        if url.isFileURL {
                            if UTType(filenameExtension: url.pathExtension)?.conforms(to: .image) == true {
                                if images.count < maxImages, let image = loadImageFromFile(url) { images.append(image) }
                            } else if files.count >= maxFiles {
                                moreFiles = true
                            } else if let file = loadFile(url) {
                                files.append(file)
                            }
                        } else {
                            webURL = url
                        }
                    }
                } else if provider.hasItemConformingToTypeIdentifier(UTType.text.identifier) {
                    if let text = await loadText(provider) { texts.append(text) }
                } else if !provider.hasItemConformingToTypeIdentifier(UTType.image.identifier),
                          provider.hasItemConformingToTypeIdentifier(UTType.data.identifier) {
                    // Over the cap: left unread (memory), but the composer says so.
                    if files.count >= maxFiles {
                        moreFiles = true
                    } else if let file = await loadFile(provider) {
                        files.append(file)
                    }
                }
            }
        }

        if texts.isEmpty, webURL == nil, images.isEmpty, files.isEmpty, !moreFiles { return nil }

        let (title, description) = composeText(texts: texts, webURL: webURL)
        return SharedPayload(
            title: title,
            descriptionText: description,
            images: images,
            files: files,
            moreFiles: moreFiles,
            text: texts.joined(separator: "\n"),
            subject: subject,
            url: webURL?.absoluteString
        )
    }

    // MARK: - Title / description

    private static func composeText(texts: [String], webURL: URL?) -> (String, String) {
        let joined = texts.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
        let urlString = webURL?.absoluteString

        let firstLine = joined.split(separator: "\n", maxSplits: 1).first
            .map(String.init)?.trimmingCharacters(in: .whitespaces)

        // Image-only shares get an empty title so the user types their own.
        let title: String
        if let firstLine, !firstLine.isEmpty {
            title = String(firstLine.prefix(120))
        } else if let urlString {
            title = urlString
        } else {
            title = ""
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
        if let data {
            return normalizeImage(data: data, utType: UTType(typeId) ?? .image, suggestedName: provider.suggestedName)
        }
        // A provider registered through the legacy `loadItem` API (the
        // screenshot editor's Share, many apps' "share image") has no data
        // representation: it hands over a file URL, a UIImage or raw Data.
        // Without this fallback such an image never reached the composer.
        // Read inside the callback: only Sendable values leave it.
        let item: LegacyImageItem? = await withCheckedContinuation { continuation in
            provider.loadItem(forTypeIdentifier: typeId, options: nil) { item, _ in
                switch item {
                case let url as URL where url.isFileURL:
                    // The file is readable while the callback runs.
                    continuation.resume(returning: (try? Data(contentsOf: url)).map { .file($0, url) })
                case let image as UIImage:
                    continuation.resume(returning: image.pngData().map(LegacyImageItem.png))
                case let data as Data:
                    continuation.resume(returning: .data(data))
                default:
                    continuation.resume(returning: nil)
                }
            }
        }
        switch item {
        case let .file(data, url):
            let utType = UTType(filenameExtension: url.pathExtension) ?? UTType(typeId) ?? .image
            return normalizeImage(data: data, utType: utType, suggestedName: url.lastPathComponent)
        case let .png(png):
            return SharedImage(data: png, filename: "\(baseName(provider.suggestedName)).png", contentType: "image/png")
        case let .data(data):
            return normalizeImage(data: data, utType: UTType(typeId) ?? .image, suggestedName: provider.suggestedName)
        case nil:
            return nil
        }
    }

    private enum LegacyImageItem: Sendable {
        case file(Data, URL)
        case png(Data)
        case data(Data)
    }

    private static func baseName(_ suggestedName: String?) -> String {
        suggestedName.map { ($0 as NSString).deletingPathExtension }
            .flatMap { $0.isEmpty ? nil : $0 } ?? "shared-image"
    }

    private static func loadImageFromFile(_ url: URL) -> SharedImage? {
        guard let data = try? Data(contentsOf: url) else { return nil }
        let utType = UTType(filenameExtension: url.pathExtension) ?? .image
        return normalizeImage(data: data, utType: utType, suggestedName: url.lastPathComponent)
    }

    private static func loadFile(_ url: URL) -> SharedFile? {
        let size = (try? url.resourceValues(forKeys: [.fileSizeKey]))?.fileSize ?? 0
        guard size <= maxFileBytes, let data = try? Data(contentsOf: url) else { return nil }
        return sharedFile(data: data, utType: UTType(filenameExtension: url.pathExtension), filename: url.lastPathComponent)
    }

    private static func loadFile(_ provider: NSItemProvider) async -> SharedFile? {
        let typeId = provider.registeredTypeIdentifiers.first {
            UTType($0)?.conforms(to: .data) == true
        } ?? UTType.data.identifier
        let data: Data? = await withCheckedContinuation { continuation in
            provider.loadDataRepresentation(forTypeIdentifier: typeId) { data, _ in
                continuation.resume(returning: data)
            }
        }
        guard let data, data.count <= maxFileBytes else { return nil }
        let utType = UTType(typeId)
        let name = provider.suggestedName ?? "shared-file"
        let filename = (name as NSString).pathExtension.isEmpty
            ? [name, utType?.preferredFilenameExtension].compactMap(\.self).joined(separator: ".")
            : name
        return sharedFile(data: data, utType: utType, filename: filename)
    }

    private static func sharedFile(data: Data, utType: UTType?, filename: String) -> SharedFile {
        SharedFile(data: data, filename: filename, contentType: utType?.preferredMIMEType ?? "application/octet-stream")
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

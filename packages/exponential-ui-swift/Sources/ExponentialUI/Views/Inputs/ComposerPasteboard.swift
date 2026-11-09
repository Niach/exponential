import Foundation
import UniformTypeIdentifiers

/// #EDE-65 R (interactions-round1.json composerPaste): the files a paste
/// into a Composer carries, read off the general pasteboard: copied files
/// (by their type) and raw images (a clipboard image has no file name:
/// `image.<ext of its type>`, `image.png`). Text-only items are no files
/// (the field's own paste). `SurfaceModel.composerPaste` decides.
enum ComposerPasteboard {
    /// A raw pasteboard image as an upload file.
    static func image(_ data: Data, type: UTType) -> SurfaceUploadFile {
        let ext = type.preferredFilenameExtension ?? "png"
        return SurfaceUploadFile(name: "image.\(ext)", size: data.count, type: type.preferredMIMEType ?? "image/\(ext)", data: data)
    }

    /// A copied file as an upload file.
    static func file(_ url: URL) -> SurfaceUploadFile {
        let data = try? Data(contentsOf: url)
        return SurfaceUploadFile(name: url.lastPathComponent, size: data?.count ?? 0, type: mimeType(of: url), url: url, data: data)
    }
}

#if canImport(UIKit)
import UIKit

extension ComposerPasteboard {
    /// The general pasteboard's files, in item order.
    static func files(_ pb: UIPasteboard = .general) -> [SurfaceUploadFile] {
        var out: [SurfaceUploadFile] = []
        for item in pb.items {
            if let url = item[UTType.fileURL.identifier].flatMap({ ($0 as? URL) ?? ($0 as? Data).flatMap { URL(dataRepresentation: $0, relativeTo: nil) } }), url.isFileURL {
                out.append(file(url))
                continue
            }
            // The item's first image representation, else its first
            // non-text one (a copied PDF: a file, never attached).
            let types = item.keys.compactMap { UTType($0) }
            if let t = types.first(where: { $0.conforms(to: .image) }) {
                let value = item[t.identifier]
                if let data = (value as? Data) ?? (value as? UIImage)?.pngData() {
                    out.append(image(data, type: (value is UIImage) ? .png : t))
                }
            } else if let t = types.first(where: { !$0.conforms(to: .text) && !$0.conforms(to: .url) }), let data = item[t.identifier] as? Data {
                let ext = t.preferredFilenameExtension ?? "bin"
                out.append(SurfaceUploadFile(name: "file.\(ext)", size: data.count, type: t.preferredMIMEType ?? "application/octet-stream", data: data))
            }
        }
        return out
    }

    /// Does the pasteboard hold something a Composer takes (an image or a file)?
    static func hasFiles(_ pb: UIPasteboard = .general) -> Bool {
        pb.hasImages || pb.items.contains { $0.keys.contains { k in UTType(k).map { !$0.conforms(to: .text) && !$0.conforms(to: .url) } ?? false } }
    }
}

#elseif canImport(AppKit)
import AppKit

extension ComposerPasteboard {
    /// The general pasteboard's files, in item order.
    static func files(_ pb: NSPasteboard = .general) -> [SurfaceUploadFile] {
        var out: [SurfaceUploadFile] = []
        for item in pb.pasteboardItems ?? [] {
            if let s = item.string(forType: .fileURL), let url = URL(string: s), url.isFileURL {
                out.append(file(url))
            } else if let data = item.data(forType: .png) {
                out.append(image(data, type: .png))
            } else if let data = item.data(forType: .tiff), let png = NSBitmapImageRep(data: data)?.representation(using: .png, properties: [:]) {
                out.append(image(png, type: .png))
            }
        }
        return out
    }
}
#endif

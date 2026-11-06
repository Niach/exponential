// #VAPP-265, #VAPP-268: the files seam on UIDocumentPickerViewController. Its own module so an iOS app links
// the document picker only when it declares the capability (ITMS-90683).
#if os(iOS)
import Foundation
import UniformTypeIdentifiers
import UIKit
import ExponentialEngineCapabilityCore

/// UIDocumentPickerViewController: open (copies) and save (export a temp file).
public final class SystemDocumentPicker: DocumentPicker, @unchecked Sendable {
    let config: CapabilityConfig
    public init(_ config: CapabilityConfig) { self.config = config }

    public func open(types: [UTType], multiple: Bool) throws -> [PickedFile] {
        let urls: [URL] = try Blocking.onMain { [config] finish in
            let picker = UIDocumentPickerViewController(forOpeningContentTypes: types, asCopy: true)
            picker.allowsMultipleSelection = multiple
            Self.run(config, picker, finish)
        }
        return try urls.map { url in
            let scoped = url.startAccessingSecurityScopedResource()
            defer {
                if scoped { url.stopAccessingSecurityScopedResource() }
                try? FileManager.default.removeItem(at: url) // our copy
            }
            let mime = (try? url.resourceValues(forKeys: [.contentTypeKey]))?.contentType?.preferredMIMEType
            return PickedFile(name: url.lastPathComponent, mimeType: mime, data: try Data(contentsOf: url))
        }
    }

    public func save(_ file: PickedFile) throws -> Bool {
        try withTempFile(file) { url in
            let urls: [URL] = try Blocking.onMain { [config] finish in
                Self.run(config, UIDocumentPickerViewController(forExporting: [url!], asCopy: true), finish)
            }
            return !urls.isEmpty
        }
    }

    @MainActor static func run(_ config: CapabilityConfig, _ picker: UIDocumentPickerViewController, _ finish: @escaping (Result<[URL], Error>) -> Void) {
        let delegate = DocumentDelegate { finish(.success($0)) }
        picker.delegate = delegate
        picker.presentationController?.delegate = delegate // a swipe-down = cancelled
        keep(delegate, alive: picker)
        present(config, picker, finish)
    }
}

@MainActor final class DocumentDelegate: NSObject, UIDocumentPickerDelegate, UIAdaptivePresentationControllerDelegate {
    let done: ([URL]) -> Void
    init(_ done: @escaping ([URL]) -> Void) { self.done = done }

    func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) { done(urls) }
    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) { done([]) }
    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) { done([]) }
}
#endif

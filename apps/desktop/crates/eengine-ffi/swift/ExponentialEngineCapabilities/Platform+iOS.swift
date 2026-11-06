// #VAPP-265: the iOS seams on UIKit and PhotoKit. Pickers, sheets and
// dialogs are the platform's own, presented on the main queue from
// `CapabilityConfig.presenter` while the calling worker thread waits.
#if os(iOS)
import Foundation
import LinkPresentation
import ObjectiveC
import Photos
import PhotosUI
import UIKit
import UniformTypeIdentifiers

/// Where the capabilities present their UI.
public struct CapabilityConfig {
    /// The view controller to present from (called on the main thread); nil
    /// answers `failed: no view controller to present from`.
    public var presenter: @MainActor () -> UIViewController?

    public init(presenter: @escaping @MainActor () -> UIViewController? = { CapabilityConfig.topViewController() }) {
        self.presenter = presenter
    }

    /// The foreground scene's key window's topmost presented view controller.
    @MainActor public static func topViewController() -> UIViewController? {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        let scene = scenes.first { $0.activationState == .foregroundActive } ?? scenes.first
        let window = scene?.windows.first { $0.isKeyWindow } ?? scene?.windows.first
        var top = window?.rootViewController
        while let next = top?.presentedViewController, !next.isBeingDismissed { top = next }
        return top
    }
}

private final class RetainKey {}
private let retainKey = RetainKey()

/// Keeps a (weakly held) delegate alive as long as its controller.
@MainActor func keep(_ object: AnyObject, alive owner: AnyObject) {
    objc_setAssociatedObject(owner, Unmanaged.passUnretained(retainKey).toOpaque(), object, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
}

@MainActor func present<T>(_ config: CapabilityConfig, _ controller: UIViewController, _ finish: @escaping (Result<T, Error>) -> Void) {
    guard let top = config.presenter() else {
        finish(.failure(CapabilityFailure.failed("no view controller to present from")))
        return
    }
    if let popover = controller.popoverPresentationController { // iPad: a centred popover
        popover.sourceView = top.view
        popover.sourceRect = CGRect(x: top.view.bounds.midX, y: top.view.bounds.midY, width: 0, height: 0)
        popover.permittedArrowDirections = []
    }
    top.present(controller, animated: true)
}

/// A temp copy of `file` for the duration of `body` (save, share).
func withTempFile<T>(_ file: PickedFile?, _ body: (URL?) throws -> T) throws -> T {
    guard let file else { return try body(nil) }
    let dir = FileManager.default.temporaryDirectory.appendingPathComponent("exponential-engine-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: dir) }
    let url = dir.appendingPathComponent(FileNames.safe(file.name))
    try file.data.write(to: url)
    return try body(url)
}

// MARK: photos

/// PhotoKit's library: images newest first, ids = `PHAsset.localIdentifier`.
public final class PhotoKitLibrary: PhotoLibrary, @unchecked Sendable {
    public init() {}

    public func authorize() throws -> Bool {
        var status = PHPhotoLibrary.authorizationStatus(for: .readWrite)
        if status == .notDetermined {
            status = try Blocking.wait { finish in
                PHPhotoLibrary.requestAuthorization(for: .readWrite) { finish(.success($0)) }
            }
        }
        return status == .authorized || status == .limited
    }

    private func fetch() -> PHFetchResult<PHAsset> {
        let options = PHFetchOptions()
        options.sortDescriptors = [NSSortDescriptor(key: "creationDate", ascending: false)]
        options.predicate = NSPredicate(format: "mediaType == %d", PHAssetMediaType.image.rawValue)
        return PHAsset.fetchAssets(with: options)
    }

    public func count() throws -> Int { fetch().count }

    public func photos(offset: Int, limit: Int) throws -> [PhotoRef] {
        let result = fetch()
        guard offset < result.count else { return [] }
        return result.objects(at: IndexSet(integersIn: offset..<min(offset + limit, result.count))).map(Self.ref)
    }

    public func photo(id: String) throws -> PhotoFile? {
        guard let asset = PHAsset.fetchAssets(withLocalIdentifiers: [id], options: nil).firstObject else { return nil }
        let options = PHImageRequestOptions()
        options.isNetworkAccessAllowed = true // iCloud originals
        options.deliveryMode = .highQualityFormat
        options.version = .current
        let (data, uti): (Data, String?) = try Blocking.wait { finish in
            PHImageManager.default().requestImageDataAndOrientation(for: asset, options: options) { data, uti, _, info in
                if let data { finish(.success((data, uti))) } else {
                    finish(.failure((info?[PHImageErrorKey] as? Error) ?? CapabilityFailure.failed("the photo has no data")))
                }
            }
        }
        var ref = Self.ref(asset)
        if let uti, let mime = MimeTypes.forTypeIdentifier(uti) { // an edit may render as JPEG
            ref.mimeType = mime
            ref.name = FileNames.withExtension(ref.name, UTType(uti)?.preferredFilenameExtension)
        }
        return PhotoFile(ref: ref, data: data)
    }

    static func ref(_ asset: PHAsset) -> PhotoRef {
        let resources = PHAssetResource.assetResources(for: asset)
        let primary = resources.first { $0.type == .photo } ?? resources.first
        let name = FileNames.clamp(primary?.originalFilename, fallback: "photo")
        let mime = MimeTypes.forTypeIdentifier(primary?.uniformTypeIdentifier) ?? MimeTypes.forName(name)
        return PhotoRef(id: asset.localIdentifier, name: name, mimeType: mime, takenAt: asset.creationDate)
    }

    /// A picked photo's date, only when the library is already readable (pick never asks).
    static func creationDate(_ id: String) -> Date? {
        let status = PHPhotoLibrary.authorizationStatus(for: .readWrite)
        guard status == .authorized || status == .limited else { return nil }
        return PHAsset.fetchAssets(withLocalIdentifiers: [id], options: nil).firstObject?.creationDate
    }
}

/// PHPickerViewController: no library permission needed; HEIC stays HEIC.
public final class SystemPhotoPicker: PhotoPicker, @unchecked Sendable {
    let config: CapabilityConfig
    public init(_ config: CapabilityConfig) { self.config = config }

    public func pick(limit: Int) throws -> [PhotoFile] {
        let config = self.config
        let results: [PHPickerResult] = try Blocking.onMain { finish in
            var configuration = PHPickerConfiguration(photoLibrary: .shared())
            configuration.filter = .images
            configuration.selectionLimit = limit
            configuration.preferredAssetRepresentationMode = .current
            let picker = PHPickerViewController(configuration: configuration)
            let delegate = PickerDelegate { finish(.success($0)) }
            picker.delegate = delegate
            keep(delegate, alive: picker)
            present(config, picker, finish)
        }
        return try results.map(Self.load)
    }

    static func load(_ result: PHPickerResult) throws -> PhotoFile {
        let provider = result.itemProvider
        let typeId = provider.registeredTypeIdentifiers.first { UTType($0)?.conforms(to: .image) == true } ?? UTType.image.identifier
        let data: Data = try Blocking.wait { finish in
            _ = provider.loadDataRepresentation(forTypeIdentifier: typeId) { data, error in
                if let data { finish(.success(data)) } else { finish(.failure(error ?? CapabilityFailure.failed("the photo has no data"))) }
            }
        }
        let name = FileNames.withExtension(FileNames.clamp(provider.suggestedName, fallback: "photo"), UTType(typeId)?.preferredFilenameExtension)
        let mime = MimeTypes.forTypeIdentifier(typeId) ?? MimeTypes.forName(name)
        let takenAt = result.assetIdentifier.flatMap(PhotoKitLibrary.creationDate)
        return PhotoFile(ref: PhotoRef(id: result.assetIdentifier ?? "picked-\(UUID().uuidString)", name: name, mimeType: mime, takenAt: takenAt), data: data)
    }
}

@MainActor final class PickerDelegate: NSObject, PHPickerViewControllerDelegate {
    let done: ([PHPickerResult]) -> Void
    init(_ done: @escaping ([PHPickerResult]) -> Void) { self.done = done }

    func picker(_ picker: PHPickerViewController, didFinishPicking results: [PHPickerResult]) {
        let done = self.done
        picker.dismiss(animated: true) { done(results) } // [] = cancelled
    }
}

// MARK: files

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

// MARK: share

/// UIActivityViewController; `shared` = its completion's `completed`.
public final class SystemShareSheet: ShareSheet, @unchecked Sendable {
    let config: CapabilityConfig
    public init(_ config: CapabilityConfig) { self.config = config }

    public func share(_ items: ShareItems) throws -> Bool {
        try withTempFile(items.file) { fileURL in
            try Blocking.onMain { [config] finish in
                var activity: [Any] = []
                if let text = items.text { activity.append(text) }
                if let url = items.url { activity.append(url) }
                if let fileURL { activity.append(fileURL) }
                if let title = items.title, !activity.isEmpty { activity[0] = TitledItem(activity[0], title: title) }
                let controller = UIActivityViewController(activityItems: activity, applicationActivities: nil)
                controller.completionWithItemsHandler = { _, completed, _, error in
                    finish(error.map { .failure($0) } ?? .success(completed))
                }
                present(config, controller, finish)
            }
        }
    }
}

/// The share title: the subject (mail) and the sheet's header.
final class TitledItem: NSObject, UIActivityItemSource, @unchecked Sendable { // nonisolated: UIKit may ask off the main thread
    let item: Any
    let title: String
    init(_ item: Any, title: String) { self.item = item; self.title = title }

    func activityViewControllerPlaceholderItem(_ activityViewController: UIActivityViewController) -> Any { item }
    func activityViewController(_ activityViewController: UIActivityViewController, itemForActivityType activityType: UIActivity.ActivityType?) -> Any? { item }
    func activityViewController(_ activityViewController: UIActivityViewController, subjectForActivityType activityType: UIActivity.ActivityType?) -> String { title }
    func activityViewControllerLinkMetadata(_ activityViewController: UIActivityViewController) -> LPLinkMetadata? {
        let metadata = LPLinkMetadata()
        metadata.title = title
        if let url = item as? URL { metadata.originalURL = url; metadata.url = url }
        return metadata
    }
}

// MARK: print

/// UIPrintInteractionController with the bytes; `printed` = its completion's `completed`.
public final class SystemPrintDialog: PrintDialog, @unchecked Sendable {
    let config: CapabilityConfig
    public init(_ config: CapabilityConfig) { self.config = config }

    public func print(_ file: PickedFile) throws -> Bool {
        try Blocking.onMain { [config] finish in
            guard UIPrintInteractionController.isPrintingAvailable else { return finish(.failure(CapabilityFailure.unavailable)) }
            guard UIPrintInteractionController.canPrint(file.data) else { return finish(.failure(CapabilityFailure.failed("this file cannot be printed"))) }
            guard let top = config.presenter() else { return finish(.failure(CapabilityFailure.failed("no view controller to present from"))) }
            let controller = UIPrintInteractionController.shared
            let info = UIPrintInfo(dictionary: nil)
            info.jobName = file.name
            info.outputType = (file.mimeType ?? "").hasPrefix("image/") ? .photo : .general
            controller.printInfo = info
            controller.printingItem = file.data
            let completion: UIPrintInteractionController.CompletionHandler = { _, completed, error in
                finish(error.map { .failure($0) } ?? .success(completed))
            }
            let shown = top.traitCollection.userInterfaceIdiom == .pad
                ? controller.present(from: CGRect(x: top.view.bounds.midX, y: top.view.bounds.midY, width: 0, height: 0), in: top.view, animated: true, completionHandler: completion)
                : controller.present(animated: true, completionHandler: completion)
            if !shown { finish(.failure(CapabilityFailure.failed("the print dialog did not open"))) }
        }
    }
}
#endif

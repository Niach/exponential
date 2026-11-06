// #VAPP-265, #VAPP-268: the photos seams on PhotoKit and PHPicker. Its own module so an iOS app links
// Photos/PhotosUI only when it declares the capability (ITMS-90683).
#if os(iOS)
import Foundation
import Photos
import PhotosUI
import UniformTypeIdentifiers
import UIKit
import ExponentialEngineCapabilityCore

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
#endif

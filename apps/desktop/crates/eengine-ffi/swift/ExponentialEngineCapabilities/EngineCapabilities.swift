// #VAPP-265: the platform's set, ready for `EngineProviders(capabilities:)`.
// #VAPP-268: the umbrella over the five capability modules. An iOS app that
// ships to the App Store links only the modules it declares instead
// (ITMS-90683: every linked privacy API needs its purpose string), as the
// runner does (apps/runner/ios).
@_exported import ExponentialEngineCapabilityCore
@_exported import ExponentialEngineFiles
@_exported import ExponentialEngineNotifications
@_exported import ExponentialEnginePhotos
@_exported import ExponentialEnginePrint
@_exported import ExponentialEngineShare
import Foundation
import ExponentialEngine

public enum EngineCapabilities {
    #if os(iOS)
    /// photos, files, share, print and notifications on UIKit, PhotoKit and
    /// UserNotifications, presenting from `config.presenter`.
    public static func ios(_ config: CapabilityConfig = CapabilityConfig()) -> [Capability] {
        [
            PhotosCapability(library: PhotoKitLibrary(), picker: SystemPhotoPicker(config)),
            FilesCapability(picker: SystemDocumentPicker(config)),
            ShareCapability(sheet: SystemShareSheet(config)),
            PrintCapability(dialog: SystemPrintDialog(config)),
            NotificationsCapability(scheduler: UserNotificationScheduler()),
        ]
    }
    #endif

    #if os(macOS)
    /// macOS: notifications on UserNotifications; photos, files, share and
    /// print are mounted but answer `unavailable` (no AppKit seams yet; pass
    /// your own to the capabilities' initializers).
    public static func macOS() -> [Capability] {
        [
            PhotosCapability(library: nil, picker: nil),
            FilesCapability(picker: nil),
            ShareCapability(sheet: nil),
            PrintCapability(dialog: nil),
            NotificationsCapability(scheduler: UserNotificationScheduler()),
        ]
    }
    #endif
}

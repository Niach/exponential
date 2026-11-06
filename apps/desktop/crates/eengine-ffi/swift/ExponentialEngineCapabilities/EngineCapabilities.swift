// #VAPP-265: the platform's set, ready for `EngineProviders(capabilities:)`.
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

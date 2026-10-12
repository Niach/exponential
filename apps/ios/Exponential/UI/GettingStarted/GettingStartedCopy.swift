import Foundation

/// EXP-1270 (EXP-1259 D6): Getting started's own words are the fixture's
/// (`getting-started.json`, ported by ExpCore `GettingStarted`): the block's
/// title, `{done} of {total}`, the Docs button, `Next`, and per row the title,
/// the `why` sentence and the `cta` button label (`GettingStarted.title`,
/// `.why`, `.cta`). No locked hints.
///
/// What is left here is the device-setup block's two cards (`DeviceSetup`,
/// EXP-1169: the wizard's devices step and Add device). They read their
/// words from here since the old checklist owned them, byte-identical to web
/// `DEVICE_CARD_COPY` (`getting-started-copy.ts`).
enum GettingStartedCopy {
    static let desktopTitle = "Get the desktop app"
    static let desktopDescription = "Runs agents on your machine and registers it as one of your devices."
    static let desktopAction = "Download the desktop app"

    static let serverTitle = "Set up a server"
    static let serverDescription = "Run the headless daemon on an always-on machine to take remote Start coding requests."
    static let serverAction = "Copy install command"
}

# eengine-desktop

The Engine's native capabilities on the desktop (#VAPP-265, E5): `photos`, `files`, `share`, `print` and `notifications` as `eengine::Capability` providers for macOS, Windows and Linux. It is host code for E6's runners and eDE, so it never depends on gpui or an app crate. The contract is `packages/eengine/fixtures/capabilities.json` (`tools[].nativeOutputSchema`). The Engine's bridge validates, maps `fileId`s to base64 `data` and stores answered bytes; this crate only does the native half.

```rust
let capabilities = eengine_desktop::desktop_capabilities(DesktopConfig::new("Exponential", on_tap));
```

`desktop_capabilities` returns only what this OS provides. The broker answers `unavailable` for the rest.

## Threading rule

Every `call` blocks its caller until the person answers the platform's dialog. Call it from a worker thread, never from the UI/main thread.

- **macOS:** the AppKit work is dispatched to the main queue and the caller waits. Panels and sheets use their non-modal completion-handler forms, so the main queue keeps draining while they are open. A call made on the main thread answers `failed`, and so does a host whose main thread runs no event loop (after 10 s).
- **Linux:** every zbus call (portal, notifications) runs on a fresh thread inside this crate's own Tokio runtime. A zbus built with the `tokio` feature, which cargo may unify in, cannot panic there, and neither can a caller that is itself a Tokio worker (see `crates/ui/src/file_picker.rs`).
- **Windows:** each dialog runs on a fresh COM STA thread.

## Tools per OS

| Tool | macOS | Linux | Windows |
|---|---|---|---|
| photos.list / photos.get | the Pictures folder (`~/Pictures`) | `XDG_PICTURES_DIR` | Known Folder `FOLDERID_Pictures` |
| photos.pick | NSOpenPanel, image types | portal FileChooser, image MIME types + globs | IFileOpenDialog, image extensions |
| files.open / files.save | NSOpenPanel / NSSavePanel | portal FileChooser | IFileOpenDialog / IFileSaveDialog |
| share.send | NSSharingServicePicker on the key window's content view | not provided (no share portal) | not provided (DataTransferManager needs a WinRT window interop handle) |
| print.print | NSPrintOperation sheet: PDFKit for PDFs, an NSImageView for PNG/JPEG | portal Print (PreparePrint, then Print of a temp file) | not provided (the shell `print` verb prints silently, with no dialog and therefore no consent) |
| notifications.post / cancel | NSUserNotificationCenter (mac-notification-sys) | org.freedesktop.Notifications (notify-rust) | WinRT toasts (notify-rust) |

**photos.** The library is the folder, scanned recursively for jpg, jpeg, png, heic, gif and webp files, newest first by mtime. Hidden files and folders, `*.photoslibrary` packages and symlinks are skipped. `id` is the path relative to the folder with `/` separators, and a picked file outside the folder gets `picked:<n>` (valid for this process only). Absolute ids, `..` and anything that resolves outside the folder answer `notFound`. `takenAt` is the mtime in RFC 3339 UTC. `cursor` is an opaque offset token, and each page rescans the folder. `DesktopConfig::pictures_dir` overrides the folder. Files over 64 MB answer `failed`.

**files.open.** `accept` becomes the picker filter. Linux filters by the MIME types plus extension globs. macOS and Windows filter by the extensions in the bridge's `mimeByExtension` table. A type the table cannot express makes the picker offer every file. The answered `mimeType` comes from the extension, then from sniffing the bytes, then falls back to `application/octet-stream`.

**share.send.** A chosen service answers `shared: true` (AppKit does not report whether it finished) and a dismissed picker answers `false`. A shared file is written to `$TMPDIR/eengine-share/` and left for the OS to clean up, because the service reads it after the call answers. The anchor is the window's content view. AppKit documents `showRelativeToRect:` for mouse-down, so this needs a check on real hardware.

**print.print.** A cancelled dialog answers `printed: false`. On Linux, PNG/JPEG go through the portal backend's (CUPS') image filters, which needs a check on a real printer.

**notifications.** `at` null or in the past posts now. A future `at` is scheduled in-process: one scheduler thread fires it, but only while the host runs, and a host that quits drops its pending ones. Posting an existing id replaces it, and a late click on the replaced one is dropped. `cancel` removes a pending one and withdraws a shown one: Linux uses `CloseNotification`, macOS removes the delivered notification with the same title and body, and Windows cannot withdraw a shown toast, so only the pending one and its tap are dropped. A click calls `DesktopConfig::on_tap` with the stored `tap` JSON verbatim, and only when the notification carried one. A dismissal never calls it.

## Platform permissions (E7 declares them; nothing here edits an app)

- **macOS:** the App Sandbox needs `com.apple.security.files.user-selected.read-write` (open/save panels, picks), `com.apple.security.assets.pictures.read-only` (photos.list/get on `~/Pictures`) and `com.apple.security.print` (print). Sharing needs nothing extra. Notifications need a bundle identifier (`DesktopConfig::app_id`, default the main bundle's): unbundled dev binaries post under mac-notification-sys's fallback identity, and the person allows notifications in System Settings. Reading `~/Pictures` from an unsandboxed app may raise the TCC "access Pictures" prompt on newer macOS.
- **Linux (Flatpak/Snap):** the portals need `xdg-desktop-portal` with a FileChooser and Print backend, plus `--talk-name=org.freedesktop.Notifications` and `--filesystem=xdg-pictures:ro` for the library. Outside a sandbox a running portal and notification daemon are enough.
- **Windows:** toasts need an AppUserModelID (`DesktopConfig::app_id`) registered by the installer's Start-menu shortcut, or set by the host process (`SetCurrentProcessExplicitAppUserModelID`). Packaged (MSIX) apps need the `picturesLibrary` capability for photos.list/get.

## Tests

```bash
cargo test -p eengine-desktop                    # fakes behind every seam; answers checked against nativeOutputSchema
cargo check -p eengine-desktop --all-targets
```

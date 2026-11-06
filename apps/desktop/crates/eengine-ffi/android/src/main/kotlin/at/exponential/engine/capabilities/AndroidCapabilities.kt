// #VAPP-265: the five native capabilities for `EngineProviders.capabilities`.
//
//   val host = ComponentActivityHost(applicationContext) { currentActivity }
//   val capabilities = AndroidCapabilities.all(host)
//
// Calls block on the Engine's worker thread; on the main thread every tool
// answers `failed: main thread`.
package at.exponential.engine.capabilities

import at.exponential.engine.ffi.Capability

object AndroidCapabilities {
    fun photos(host: ActivityHost) = PhotosCapability(AndroidPhotoLibrary(host))
    fun files(host: ActivityHost) = FilesCapability(AndroidDocumentPicker(host))
    fun share(host: ActivityHost) = ShareCapability(AndroidShareSheet(host))
    fun print(host: ActivityHost) = PrintCapability(AndroidPrinter(host))
    fun notifications(host: ActivityHost) = AndroidNotifications.capability(host.context, host)

    fun all(host: ActivityHost): List<Capability> = listOf(photos(host), files(host), share(host), print(host), notifications(host))
}

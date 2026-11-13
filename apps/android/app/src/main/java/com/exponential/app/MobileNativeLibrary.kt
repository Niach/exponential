package com.exponential.app

/**
 * #PERF-11: the app ships ONE Rust library, `libexponential_mobile_ffi.so`
 * (apps/desktop/crates/exponential-mobile-ffi): genUI's, the Engine's and
 * eNet's UniFFI facades linked together. Their generated bindings each load
 * their own library name unless `uniffi.component.<namespace>.libraryOverride`
 * names another, so this points all three at it. It runs before anything in
 * the process touches a binding (ExponentialApp's construction).
 *
 * Only on Android's runtime: the JVM unit tests (and Robolectric) load the
 * facades' standalone host builds by their own names (`jna.library.path`).
 */
object MobileNativeLibrary {
    const val NAME = "exponential_mobile_ffi"

    /** The UniFFI namespaces linked into [NAME]. */
    val NAMESPACES = listOf("exponential_ui_ffi", "eengine_ffi", "exponential_net_ffi")

    fun install(vmName: String? = System.getProperty("java.vm.name")) {
        if (vmName != "Dalvik") return
        for (namespace in NAMESPACES) System.setProperty("uniffi.component.$namespace.libraryOverride", NAME)
    }
}

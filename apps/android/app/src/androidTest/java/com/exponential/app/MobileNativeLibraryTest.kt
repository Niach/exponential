package com.exponential.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.util.zip.ZipFile

/**
 * #PERF-11: the APK carries ONE Rust library, and genUI's, the Engine's and
 * eNet's bindings all load and call into it (MobileNativeLibrary).
 */
@RunWith(AndroidJUnit4::class)
class MobileNativeLibraryTest {
    @Test
    fun theApkShipsOneRustLibrary() {
        val apk = InstrumentationRegistry.getInstrumentation().targetContext.applicationInfo.sourceDir
        val libs = ZipFile(apk).use { zip -> zip.entries().toList().map { it.name }.filter { it.startsWith("lib/") } }
        val rust = libs.map { it.substringAfterLast('/') }.filter { it.contains("ffi") }.toSortedSet()
        assertEquals(sortedSetOf("lib${MobileNativeLibrary.NAME}.so"), rust)
    }

    @Test
    fun everyFacadeCallsIntoIt() {
        for (namespace in MobileNativeLibrary.NAMESPACES) {
            assertEquals(MobileNativeLibrary.NAME, System.getProperty("uniffi.component.$namespace.libraryOverride"))
        }
        // genUI: a call into the core.
        assertTrue(at.exponential.ui.ffi.builtinThemeIds().isNotEmpty())
        // the Engine: its library loads and its scaffolding matches the binding.
        at.exponential.engine.ffi.uniffiEnsureInitialized()
        // eNet: a call into the node's crypto.
        assertTrue(at.exponential.net.ffi.pushKeypairGenerate().`public`.size == 32)
    }
}

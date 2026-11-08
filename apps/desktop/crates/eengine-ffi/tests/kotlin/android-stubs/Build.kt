// The host (JVM) suite's stand-in for android.os.Build: the binding is generated
// in Android mode (uniffi.toml), SDK_INT 0 = JNA's cleaner, as below API 34.
package android.os

object Build {
    object VERSION {
        const val SDK_INT: Int = 0
    }
    object VERSION_CODES {
        const val UPSIDE_DOWN_CAKE: Int = 34
    }
}

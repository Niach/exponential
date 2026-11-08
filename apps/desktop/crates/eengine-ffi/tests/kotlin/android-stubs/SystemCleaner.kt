// The host suite's stand-in for android.system.SystemCleaner (never picked: SDK_INT is 0).
package android.system

object SystemCleaner {
    fun cleaner(): java.lang.ref.Cleaner = java.lang.ref.Cleaner.create()
}

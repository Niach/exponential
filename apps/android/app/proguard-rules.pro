# Keep kotlinx.serialization metadata
-keepattributes *Annotation*, InnerClasses
-dontnote kotlinx.serialization.AnnotationsKt
-keepclassmembers class kotlinx.serialization.json.** {
    *** Companion;
}
-keepclasseswithmembers class kotlinx.serialization.json.** {
    kotlinx.serialization.KSerializer serializer(...);
}

# VAPP-4 spike: uniffi bindings reach Rust through JNA reflection.
-keep class com.sun.jna.** { *; }
-keep class uniffi.** { *; }
# JNA's desktop-only AWT helpers reference java.awt, absent on Android (R8
# otherwise fails the release/benchmark build on missing classes).
-dontwarn java.awt.**
-keep class com.exponential.app.ui.spike.** { *; }

# Keep kotlinx.serialization metadata
-keepattributes *Annotation*, InnerClasses
-dontnote kotlinx.serialization.AnnotationsKt
-keepclassmembers class kotlinx.serialization.json.** {
    *** Companion;
}
-keepclasseswithmembers class kotlinx.serialization.json.** {
    kotlinx.serialization.KSerializer serializer(...);
}

# EXP-1435 / VAPP-40: the eNet UniFFI facade is reached over JNA; its
# structures, callback interfaces (EnetListener, EnetAccessPolicy) and
# error classes are looked up by name. JNA's AWT helper references classes
# Android does not ship (VAPP-4).
-keep class at.exponential.net.ffi.** { *; }
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-dontwarn java.awt.**

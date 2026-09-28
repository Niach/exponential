# VAPP-3 spike (-PpeerSpike=true only): JNA + UniFFI bindings are reached by reflection/JNI.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class uniffi.peer_ffi.** { *; }
-dontwarn java.awt.**
-dontwarn com.sun.jna.**

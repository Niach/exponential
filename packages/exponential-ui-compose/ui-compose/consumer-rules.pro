# VAPP-89: the UniFFI facade is reached over JNA; R8 must keep the binding's
# JNA structures and callbacks (they are looked up by name), and JNA's AWT
# helper references classes Android does not ship.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class at.exponential.ui.ffi.** { *; }
-dontwarn java.awt.**

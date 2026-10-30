# #EXP-1441: the facade is reached over JNA; R8 must keep the binding's JNA
# structures and callbacks (looked up by name), and JNA's AWT helper
# references classes Android does not ship (as eUI Compose).
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class at.exponential.talk.ffi.** { *; }
-dontwarn java.awt.**

# The consumer rules of eui-compose and engine keep JNA and both UniFFI bindings.
# Strip every android.util.Log call a dependency might make (no diagnostics in a runner).
-assumenosideeffects class android.util.Log {
    public static int v(...);
    public static int d(...);
    public static int i(...);
    public static int w(...);
    public static int e(...);
    public static int wtf(...);
    public static int println(...);
}

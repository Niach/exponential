package com.exponential.app.domain

import android.os.Trace

/**
 * EXP-1521/1522: a named section in a Perfetto/systrace capture, so the
 * Macrobenchmarks (`TraceSectionMetric`) can measure the hot paths by name.
 * Free when no trace is recording; a no-op in JVM unit tests.
 */
inline fun <T> perfTrace(name: String, block: () -> T): T {
    Trace.beginSection(name)
    try {
        return block()
    } finally {
        Trace.endSection()
    }
}

/**
 * #EXP-1538: an async section (begin and end on different frames), e.g. a
 * view opening until its Dock shows a card. API 29+; a no-op below.
 */
fun perfTraceBegin(name: String, cookie: Int) {
    if (android.os.Build.VERSION.SDK_INT >= 29) Trace.beginAsyncSection(name, cookie)
}

fun perfTraceEnd(name: String, cookie: Int) {
    if (android.os.Build.VERSION.SDK_INT >= 29) Trace.endAsyncSection(name, cookie)
}

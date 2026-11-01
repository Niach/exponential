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

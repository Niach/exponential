package com.exponential.app.domain

import java.lang.management.ManagementFactory

/**
 * #ERR-28: the bytes this thread allocates while [block] runs (garbage
 * included), for the memory-budget tests: a phone OOMs on churn, not only on
 * what stays live, so a regression in the hot paths fails CI here first.
 */
internal fun <T> allocatedBytes(block: () -> T): Pair<Long, T> {
    val bean = ManagementFactory.getThreadMXBean() as com.sun.management.ThreadMXBean
    val before = bean.currentThreadAllocatedBytes
    val result = block()
    return (bean.currentThreadAllocatedBytes - before) to result
}

package com.exponential.macrobenchmark

import androidx.benchmark.macro.ExperimentalMetricApi
import androidx.benchmark.macro.TraceMetric
import androidx.benchmark.perfetto.ExperimentalPerfettoTraceProcessorApi
import androidx.benchmark.perfetto.PerfettoTraceProcessor

/**
 * The frames of opening a session, apart from the cold start that precedes
 * them. `openSession` runs on a cold process, so a plain
 * `Choreographer#doFrame%` Max is the process's FIRST frame: the window's EGL
 * context and surface, plus every Skia shader compiled afresh (Macrobenchmark
 * drops the shader cache on a cold start). On the emulator that one frame is
 * 0.2–0.8 s depending on its GPU mode and runs no session code at all.
 *
 * This metric reads only the main thread's frames that START after the last
 * input event, the session card's tap (the chat's scrolls come first):
 *
 * - `sessionOpenFrameMaxMs`: the longest of them, wall time (includes the
 *   main thread waiting on the RenderThread, i.e. the emulator's GL).
 * - `sessionOpenMainThreadMaxMs`: the most main-thread CPU any one of them
 *   took (composition, layout, the feed fold): the app's own share.
 * - `coldFirstFrameMs`: the process's first frame, for comparison.
 */
@OptIn(ExperimentalMetricApi::class, ExperimentalPerfettoTraceProcessorApi::class)
class SessionOpenFrameMetric : TraceMetric() {
    override fun getMeasurements(
        captureInfo: CaptureInfo,
        traceSession: PerfettoTraceProcessor.Session,
    ): List<Measurement> {
        val pkg = captureInfo.targetPackageName
        val main = """
            (select t.utid from thread t join process p using(upid)
             where p.name = '$pkg' and t.tid = p.pid order by t.utid desc limit 1)
        """.trimIndent()
        val frames = """
            select s.ts, s.dur from slice s join thread_track tt on s.track_id = tt.id
            where tt.utid = $main and s.depth = 0 and s.name like 'Choreographer#doFrame%'
        """.trimIndent()
        val tap = """
            (select max(s.ts) from slice s join thread_track tt on s.track_id = tt.id
             where tt.utid = $main and s.name like 'deliverInputEvent%')
        """.trimIndent()
        val rows = traceSession.query(
            """
            select f.dur as dur,
              (select coalesce(sum(min(st.ts + st.dur, f.ts + f.dur) - max(st.ts, f.ts)), 0)
               from thread_state st
               where st.utid = $main and st.state = 'Running'
                 and st.ts < f.ts + f.dur and st.ts + st.dur > f.ts) as running
            from ($frames) f where f.ts > $tap
            """.trimIndent(),
        ).toList()
        val first = traceSession.query("select dur from ($frames) order by ts limit 1").firstOrNull()
        return buildList {
            if (rows.isNotEmpty()) {
                add(Measurement("sessionOpenFrameMaxMs", rows.maxOf { ms(it["dur"]) }))
                add(Measurement("sessionOpenMainThreadMaxMs", rows.maxOf { ms(it["running"]) }))
            }
            if (first != null) add(Measurement("coldFirstFrameMs", ms(first["dur"])))
        }
    }

    private fun ms(nanos: Any?): Double = ((nanos as? Number)?.toDouble() ?: 0.0) / 1_000_000.0
}

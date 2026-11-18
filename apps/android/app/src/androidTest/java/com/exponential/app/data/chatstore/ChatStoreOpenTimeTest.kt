package com.exponential.app.data.chatstore

import android.os.Bundle
import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import at.exponential.chatstore.ffi.ChatStore
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.TailEvent
import com.exponential.app.domain.foldStoredTail
import java.io.File
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * #EDE-114 (chat-store.json OPEN) measured on a device: a big chat (17,000
 * events: tool steps with ~900-byte output, narration) seeded into a store
 * in its own cache dir, then the time a session view takes to open from it:
 * `ChatStore.open` + `openTail` (the REPLAY TAIL) + decoding the answer +
 * folding it to rows. Median of several runs, logged under
 * `ChatStoreOpenTime` and reported in the instrumentation status; the dir is
 * deleted afterwards.
 */
@RunWith(AndroidJUnit4::class)
class ChatStoreOpenTimeTest {
    private val output = "x".repeat(880)

    private fun event(i: Int): JsonObject = when (i % 3) {
        0 -> buildJsonObject {
            put("kind", "tool")
            put("name", "Bash")
            put("id", "t$i")
            put("detail", "cargo test -p chat-store --release case $i")
            put("at", 1_800_000_000_000L + i)
        }
        1 -> buildJsonObject {
            put("kind", "tool_update")
            put("id", "t${i - 1}")
            put("output", "$i $output")
            put("at", 1_800_000_000_000L + i)
        }
        else -> buildJsonObject {
            put("kind", "narration")
            put("text", "Step $i: the tests pass, the next case reads the journal and checks the watermarks once more.")
            put("messageId", "m$i")
            put("at", 1_800_000_000_000L + i)
        }
    }

    @Test
    fun openingABigChatFromTheStore() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val dir = File(context.cacheDir, "chat-store-open-time")
        dir.deleteRecursively()
        try {
            ChatStore.open(dir.path, "bench").use { store ->
                (0 until 17_000).chunked(1_000).forEach { chunk ->
                    store.putFeed("root", "s1", "d1", ChatStoreCodec.feedJson(chunk.map { TailEvent(event(it), it.toLong()) }), 1_800_000_000_000L)
                }
                assertEquals(17_000uL, store.count("s1", "d1"))
            }
            val runs = (0 until RUNS).map { run ->
                val t0 = System.nanoTime()
                val store = ChatStore.open(dir.path, "bench")
                val t1 = System.nanoTime()
                val text = store.openTail("s1", "d1", DomainContract.steerFeedReplayTail.toUInt())
                val t2 = System.nanoTime()
                val tail = ChatStoreCodec.parseTail(text)!!
                val t3 = System.nanoTime()
                val state = foldStoredTail(tail.events)
                val t4 = System.nanoTime()
                store.close()
                assertTrue(tail.truncated)
                assertTrue(state.feed.isNotEmpty())
                if (run == 0) Log.i(TAG, "events=${tail.events.size} rows=${state.feed.size} bytes=${text.length}")
                longArrayOf(t1 - t0, t2 - t1, t3 - t2, t4 - t3, t4 - t0)
            }
            val names = listOf("open", "openTail", "decode", "fold", "total")
            val bundle = Bundle()
            names.forEachIndexed { index, name ->
                val ms = runs.map { it[index] / 1_000_000.0 }.sorted()
                val median = ms[ms.size / 2]
                Log.i(TAG, "$name median=%.1f ms runs=%s".format(median, ms.joinToString { "%.1f".format(it) }))
                bundle.putString(name, "%.1f".format(median))
            }
            InstrumentationRegistry.getInstrumentation().sendStatus(0, bundle)
        } finally {
            dir.deleteRecursively()
        }
    }

    private companion object {
        const val TAG = "ChatStoreOpenTime"
        const val RUNS = 9
    }
}

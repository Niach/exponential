package com.exponential.app.data.composer

import com.exponential.app.domain.ComposerDraft
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * #FEED-219: the writer's debounce/flush and the binding's follow-until-edited
 * rule (composer-draft.json `$comment`), over a fake transport and an
 * in-memory "synced" table.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ComposerDraftBindingTest {

    private class Fake : ComposerDraftWriter.Transport {
        val synced = MutableStateFlow<Map<String, String>>(emptyMap())
        val calls = mutableListOf<String>()
        var fail = false

        override suspend fun upsert(accountId: String, teamId: String, key: String, text: String) {
            calls += "upsert $key $text"
            if (fail) error("offline")
            synced.value = synced.value + (key to text)
        }

        override suspend fun delete(accountId: String, key: String) {
            calls += "delete $key"
            if (fail) error("offline")
            synced.value = synced.value - key
        }
    }

    private val comment = ComposerDraft.Target(ComposerDraft.Kind.Comment, "issue-1")
    private val reply = ComposerDraft.Target(ComposerDraft.Kind.Reply, "comment-1")

    private fun rig(block: suspend Rig.() -> Unit) = runBlocking(Dispatchers.Unconfined) {
        val scope = CoroutineScope(coroutineContext + kotlinx.coroutines.SupervisorJob())
        val rig = Rig(scope)
        try {
            rig.block()
        } finally {
            scope.cancel()
        }
    }

    private class Rig(scope: CoroutineScope) {
        val fake = Fake()
        val writer = ComposerDraftWriter(scope, fake, debounceMs = 40)
        val field = MutableStateFlow("")
        val shown: (ComposerDraftWriter.Id) -> Flow<String> = { id ->
            combine(writer.localFlow(id), fake.synced) { local, rows ->
                ComposerDraft.field("", edited = false, local = local, synced = rows[id.key])
            }
        }
        val binding = ComposerDraftBinding(scope, writer, shown, field) { field.value = it }
    }

    private suspend fun settle() {
        repeat(5) { yield() }
    }

    @Test
    fun `a bound field shows the synced draft and follows it until edited`() = rig {
        fake.synced.value = mapOf(comment.key to "half a thought")
        binding.bind("acc", comment, "team")
        settle()
        assertEquals("half a thought", field.value)
        fake.synced.value = mapOf(comment.key to "new from the phone")
        settle()
        assertEquals("new from the phone", field.value)
        field.value = "mine"
        settle()
        fake.synced.value = mapOf(comment.key to "from the phone again")
        settle()
        assertEquals("mine", field.value)
    }

    @Test
    fun `edits write debounced, blank deletes`() = rig {
        binding.bind("acc", comment, "team")
        settle()
        field.value = "a"
        settle()
        field.value = "ab"
        settle()
        assertEquals(emptyList<String>(), fake.calls)
        delay(120)
        assertEquals(listOf("upsert comment:issue-1 ab"), fake.calls)
        field.value = "  "
        settle()
        binding.flush()
        settle()
        assertEquals("delete comment:issue-1", fake.calls.last())
    }

    @Test
    fun `a new target flushes the old one and starts from its own draft`() = rig {
        fake.synced.value = mapOf(reply.key to "the reply")
        binding.bind("acc", comment, "team")
        settle()
        field.value = "the comment"
        settle()
        binding.bind("acc", reply, "team")
        settle()
        assertEquals("upsert comment:issue-1 the comment", fake.calls.single())
        assertEquals("the reply", field.value)
        binding.bind("acc", comment, "team")
        settle()
        assertEquals("the comment", field.value)
    }

    @Test
    fun `a failed write keeps the local copy and the next write retries`() = rig {
        binding.bind("acc", comment, "team")
        settle()
        fake.fail = true
        field.value = "kept"
        settle()
        binding.flush()
        settle()
        assertEquals("kept", writer.local(ComposerDraftWriter.Id("acc", comment.key)))
        fake.fail = false
        binding.writeNow()
        settle()
        assertEquals("kept", fake.synced.value[comment.key])
    }

    @Test
    fun `a settled send writes the field it leaves`() = rig {
        binding.bind("acc", comment, "team")
        settle()
        field.value = "ship it"
        settle()
        binding.flush()
        settle()
        val at = binding.bound
        field.value = ""
        binding.settleSent(at, "ship it")
        settle()
        assertEquals(null, fake.synced.value[comment.key])
    }

    @Test
    fun `text typed before the first binding is kept and written`() = rig {
        field.value = "typed early"
        settle()
        binding.bind("acc", comment, "team")
        settle()
        assertEquals("typed early", field.value)
        delay(120)
        assertEquals("typed early", fake.synced.value[comment.key])
    }
}

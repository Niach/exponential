package com.exponential.app.data.electric

import com.exponential.app.data.db.ElectricOffsetDao
import com.exponential.app.data.db.ElectricOffsetEntity
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The one-time shape repair machinery (first used by EXP-985 for the blanked
 * `issue_drafts` rows, retired with the version floor): a listed shape is
 * refetched ONCE per account database. The list itself ships empty; the cases
 * drive the mechanism with an inline fixture.
 */
class ShapeRepairsTest {

    private val fixture = listOf(ShapeRepair(id = "test", shapes = listOf("issue_drafts")))

    private class FakeOffsetDao : ElectricOffsetDao {
        val map = mutableMapOf<String, ElectricOffsetEntity>()
        override suspend fun get(shape: String): ElectricOffsetEntity? = map[shape]
        override fun observeIsLive(shape: String): Flow<Boolean?> = flowOf(map[shape]?.isLive)
        override suspend fun upsert(item: ElectricOffsetEntity) { map[item.shape] = item }
        override suspend fun clear() { map.clear() }
    }

    private fun live(shape: String) =
        ElectricOffsetEntity(shape = shape, handle = "h1", offset = "12_3", isLive = true)

    @Test
    fun `no repair ships right now`() {
        assertTrue(SHAPE_REPAIRS.isEmpty())
    }

    @Test
    fun `a synced drafts shape is marked for an atomic refetch exactly once`() = runBlocking {
        val dao = FakeOffsetDao()
        dao.upsert(live("issue_drafts"))

        applyShapeRepairs(dao, "issue_drafts", fixture)
        val marked = dao.get("issue_drafts")!!
        assertTrue(marked.needsRefetch)
        assertFalse(marked.isLive)
        assertEquals(INITIAL_OFFSET, marked.offset)
        assertEquals("h1", marked.handle)

        // The refetch lands; the next launch must leave the cursor alone.
        dao.upsert(live("issue_drafts"))
        applyShapeRepairs(dao, "issue_drafts", fixture)
        assertEquals(live("issue_drafts"), dao.get("issue_drafts"))
    }

    @Test
    fun `other shapes are never touched`() = runBlocking {
        val dao = FakeOffsetDao()
        dao.upsert(live("issues"))
        applyShapeRepairs(dao, "issues", fixture)
        assertEquals(mapOf("issues" to live("issues")), dao.map)
    }

    @Test
    fun `a fresh database records the repair without inventing a cursor`() = runBlocking {
        val dao = FakeOffsetDao()
        applyShapeRepairs(dao, "issue_drafts", fixture)
        assertNull(dao.get("issue_drafts"))
        assertNotNull(dao.get(repairMarker("test", "issue_drafts")))
    }
}

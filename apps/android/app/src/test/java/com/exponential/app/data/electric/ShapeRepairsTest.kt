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
 * EXP-985: upgraded devices hold `issue_drafts` rows the old full-decode
 * REPLACE blanked; the shape is refetched ONCE per account database.
 */
class ShapeRepairsTest {

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
    fun `the shipped repair covers exactly the drafts shape`() {
        assertEquals(listOf("issue_drafts"), SHAPE_REPAIRS.flatMap { it.shapes })
    }

    @Test
    fun `a synced drafts shape is marked for an atomic refetch exactly once`() = runBlocking {
        val dao = FakeOffsetDao()
        dao.upsert(live("issue_drafts"))

        applyShapeRepairs(dao, "issue_drafts")
        val marked = dao.get("issue_drafts")!!
        assertTrue(marked.needsRefetch)
        assertFalse(marked.isLive)
        assertEquals(INITIAL_OFFSET, marked.offset)
        assertEquals("h1", marked.handle)

        // The refetch lands; the next launch must leave the cursor alone.
        dao.upsert(live("issue_drafts"))
        applyShapeRepairs(dao, "issue_drafts")
        assertEquals(live("issue_drafts"), dao.get("issue_drafts"))
    }

    @Test
    fun `other shapes are never touched`() = runBlocking {
        val dao = FakeOffsetDao()
        dao.upsert(live("issues"))
        applyShapeRepairs(dao, "issues")
        assertEquals(mapOf("issues" to live("issues")), dao.map)
    }

    @Test
    fun `a fresh database records the repair without inventing a cursor`() = runBlocking {
        val dao = FakeOffsetDao()
        applyShapeRepairs(dao, "issue_drafts")
        assertNull(dao.get("issue_drafts"))
        assertNotNull(dao.get(repairMarker("exp985_issue_drafts", "issue_drafts")))
    }
}

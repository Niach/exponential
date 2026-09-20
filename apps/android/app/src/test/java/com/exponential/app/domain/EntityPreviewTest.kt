package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-920: the entity-preview chip rule, locked ×4 (web `entity-preview.test.ts`,
 * iOS `EntityPreviewTests`, desktop `domain::entity_preview`) against the ONE
 * contract fixture `entity-chip.json` — same cases, same test names.
 */
class EntityPreviewTest {

    private val fixture = Json.parseToJsonElement(contractFixtureJson("entity-chip.json")).jsonObject

    @Test
    fun everyChipCaseRendersByteExact() {
        val cases = fixture.getValue("chips").jsonArray
        assertTrue(cases.isNotEmpty())
        cases.forEach { element ->
            val case = element.jsonObject
            val name = case.getValue("name").jsonPrimitive.content
            // The fixture's refs go in UNPARSED, the way web feeds them to the
            // rule: the padding/unknown-kind cases exercise the rule's own
            // trimming and fallbacks, not the parser's.
            val ref = rawRef(case.getValue("ref").jsonObject)
            assertEquals(name, case.getValue("label").jsonPrimitive.content, EntityPreview.chipLabel(ref))
            val detail = case.getValue("detail").let { if (it is JsonNull) null else it.jsonPrimitive.content }
            assertEquals(name, detail, EntityPreview.chipDetail(ref))
            assertEquals(name, case.getValue("icon").jsonPrimitive.content, EntityPreview.refIcon(ref))
        }
    }

    @Test
    fun everyGroupCaseGroupsTheSame() {
        val cases = fixture.getValue("groups").jsonArray
        assertTrue(cases.isNotEmpty())
        cases.forEach { element ->
            val case = element.jsonObject
            val name = case.getValue("name").jsonPrimitive.content
            val refs = case.getValue("refs").jsonArray.map { rawRef(it.jsonObject) }
            val expected = case.getValue("groups").jsonArray.map { group ->
                val obj = group.jsonObject
                EntityPreview.Group(
                    ref = refs[obj.getValue("ref").jsonPrimitive.int],
                    members = obj.getValue("members").jsonArray.map { refs[it.jsonPrimitive.int] },
                )
            }
            assertEquals(name, expected, EntityPreview.groupRefs(refs))
        }
    }

    @Test
    fun `every contract kind has an icon concept`() {
        DomainContract.entityRefKindValues.forEach { kind ->
            assertNotNull("icon for $kind", EntityPreview.ICON[kind])
        }
        assertEquals(DomainContract.entityRefKindValues.toSet(), EntityPreview.ICON.keys)
    }

    @Test
    fun `the parser mirrors the web one`() {
        // An unknown kind drops; a blank id drops; strings trim + clamp; count rounds.
        assertNull(EntityRef.parse(Json.parseToJsonElement("""{"kind":"thing","id":"x"}""")))
        assertNull(EntityRef.parse(Json.parseToJsonElement("""{"kind":"issue","id":"  "}""")))
        assertNull(EntityRef.parse(Json.parseToJsonElement("""{"kind":"issue"}""")))
        assertNull(EntityRef.parse(Json.parseToJsonElement("""["issue"]""")))
        val long = "x".repeat(DomainContract.expToolPreviewTextMax + 20)
        val parsed = EntityRef.parse(
            Json.parseToJsonElement(
                """{"kind":"issue","id":" i-1 ","identifier":" EXP-1 ","title":"$long","count":2.6}""",
            ),
        )!!
        assertEquals("i-1", parsed.id)
        assertEquals("EXP-1", parsed.identifier)
        assertEquals(DomainContract.expToolPreviewTextMax, parsed.title!!.length)
        assertEquals(3, parsed.count)
        // A blank title is absent, a negative count is absent, a string count is absent.
        val sparse = EntityRef.parse(
            Json.parseToJsonElement("""{"kind":"board","id":"b","title":"  ","count":-1}"""),
        )!!
        assertNull(sparse.title)
        assertNull(sparse.count)
        assertNull(EntityRef.parse(Json.parseToJsonElement("""{"kind":"list","id":"issue","count":"3"}"""))!!.count)
    }

    /** A fixture ref as written, parser untouched. */
    private fun rawRef(obj: kotlinx.serialization.json.JsonObject): EntityRef = EntityRef(
        kind = obj.getValue("kind").jsonPrimitive.content,
        id = obj.getValue("id").jsonPrimitive.content,
        identifier = obj["identifier"]?.jsonPrimitive?.content,
        title = obj["title"]?.jsonPrimitive?.content,
        count = obj["count"]?.jsonPrimitive?.int,
    )
}

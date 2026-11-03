package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// #EXP-1661 (+ #VAPP-213 built-ins): team skills, replayed off `skills.json` (web `skills.test.ts`,
// same case names).
class SkillsTest {
    private val fixture: JsonObject = Json.parseToJsonElement(contractFixtureJson("skills.json")).jsonObject

    private fun JsonObject.string(key: String): String = getValue(key).jsonPrimitive.content
    private fun JsonObject.strings(key: String): List<String> = getValue(key).jsonArray.map { it.jsonPrimitive.content }
    private fun cases(key: String) = fixture.getValue(key).jsonArray.map { it.jsonObject }.also { assertTrue(it.isNotEmpty()) }

    @Test
    fun `sortCases replay`() {
        for (case in cases("sortCases")) {
            val sorted = sortSkills(case.strings("names").map { SkillOption(id = it, name = it) })
            assertEquals(case.string("name"), case.strings("expected"), sorted.map { it.name })
        }
    }

    @Test
    fun `preselectCases replay`() {
        for (case in cases("preselectCases")) {
            val skills = case.getValue("skills").jsonArray.map {
                val s = it.jsonObject
                SkillOption(id = s.string("id"), name = s.string("name"), enabledByDefault = s.getValue("enabledByDefault").jsonPrimitive.boolean)
            }
            val saved = case.getValue("saved").let { if (it is JsonNull) null else it.jsonArray.map { e -> e.jsonPrimitive.content } }
            assertEquals(case.string("name"), case.strings("expected"), preselectSkillIds(skills, saved))
        }
    }

    @Test
    fun `menuValueCases replay`() {
        for (case in cases("menuValueCases")) {
            val expected = case.getValue("expected").let { if (it is JsonNull) null else it.jsonPrimitive.content }
            assertEquals(case.string("name"), expected, skillsMenuValue(case.getValue("count").jsonPrimitive.int))
        }
    }

    @Test
    fun `agentValueCases replay`() {
        for (case in cases("agentValueCases")) {
            assertEquals(case.string("name"), case.string("expected"), agentSkillsValue(case.strings("names")))
        }
    }

    @Test
    fun `pillCases replay`() {
        for (case in cases("pillCases")) {
            val skill = case.getValue("skill").jsonObject
            val expected = case.getValue("expected").let { if (it is JsonNull) null else it.jsonPrimitive.content }
            assertEquals(
                case.string("name"),
                expected,
                skillPill(skill.getValue("builtin").jsonPrimitive.boolean, skill.getValue("enabledByDefault").jsonPrimitive.boolean),
            )
        }
    }

    @Test
    fun `editCases replay`() {
        for (case in cases("editCases")) {
            val builtin = case.getValue("skill").jsonObject.getValue("builtin").jsonPrimitive.boolean
            assertEquals(
                case.string("name"),
                case.getValue("expected").jsonPrimitive.boolean,
                canEditSkill(builtin, case.getValue("owner").jsonPrimitive.boolean),
            )
        }
    }

    @Test
    fun `filePathCases replay`() {
        for (case in cases("filePathCases")) {
            assertEquals(case.string("name"), case.getValue("expected").jsonPrimitive.boolean, isSkillFilePath(case.string("path")))
        }
    }

    @Test
    fun `the built-ins match the fixture`() {
        val builtin = fixture.getValue("builtin").jsonObject
        assertEquals(builtin.strings("names"), Skills.BUILTIN_NAMES)
        assertEquals(builtin.strings("editorDelivery"), Skills.BUILTIN_EDITOR_DELIVERY)
        assertEquals(builtin.string("pill"), Skills.BUILTIN_PILL)
    }

    @Test
    fun `the copy matches the fixture`() {
        val list = fixture.getValue("list").jsonObject
        assertEquals(list.string("defaultPill"), Skills.DEFAULT_PILL)
        assertEquals(list.getValue("empty").jsonObject.string("title"), Skills.EMPTY_TITLE)
        val ids = list.getValue("testIds").jsonObject
        assertEquals(ids.string("page"), Skills.PAGE_TEST_ID)
        assertEquals(ids.string("list"), Skills.LIST_TEST_ID)
        assertEquals(ids.string("row"), Skills.ROW_TEST_ID)
        val agentRow = fixture.getValue("agentRow").jsonObject
        assertEquals(agentRow.string("label"), Skills.AGENT_ROW_LABEL)
        assertEquals(agentRow.string("none"), Skills.AGENT_ROW_NONE)
        val fields = fixture.getValue("page").jsonObject.getValue("fields").jsonArray.associate {
            it.jsonObject.string("id") to it.jsonObject.string("label")
        }
        assertEquals(fields["name"], Skills.FIELD_NAME)
        assertEquals(fields["description"], Skills.FIELD_DESCRIPTION)
        assertEquals(fields["body"], Skills.FIELD_BODY)
        assertEquals(fields["enabledByDefault"], Skills.FIELD_DEFAULT)
        assertEquals(fixture.string("title"), Skills.TITLE)
        val menuRow = fixture.getValue("menuRow").jsonObject
        val row = ComposerMenu.ROWS.filterIsInstance<ComposerMenuEntry.Row>().single { it.id == ComposerMenuRowId.Skills }
        assertEquals(menuRow.string("label"), row.label)
        assertEquals(menuRow.string("icon"), row.icon)
        assertEquals(menuRow.strings("when"), row.conditions.map { it.wire })
    }
}

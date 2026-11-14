package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.boolean
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-1841: [ComposerAttachments] replays ALL four lists of
 * `fixtures/composer-attachments.json` and compares the WHOLE expected value,
 * like the TS reference, desktop `domain::composer_attachments` and iOS
 * `ComposerAttachments.swift`.
 */
class ComposerAttachmentsTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("composer-attachments.json")).jsonObject
    }

    private fun list(key: String): List<JsonObject> = fixture.getValue(key).jsonArray.map { it.jsonObject }

    private fun JsonObject.str(key: String) = getValue(key).jsonPrimitive.content
    private fun JsonObject.num(key: String) = getValue(key).jsonPrimitive.int

    private fun kind(wire: String) = when (wire) {
        "image" -> ComposerAttachments.Kind.Image
        "file" -> ComposerAttachments.Kind.File
        else -> error("unknown kind $wire")
    }

    private fun pick(wire: String) = when (wire) {
        "image" -> ComposerAttachments.PickKind.Image
        "file" -> ComposerAttachments.PickKind.File
        "rejected" -> ComposerAttachments.PickKind.Rejected
        else -> error("unknown pick $wire")
    }

    private fun JsonObject.kinds(key: String) = getValue(key).jsonArray.map { kind(it.jsonPrimitive.content) }

    @Test
    fun everyListHasCases() {
        for (key in listOf("insert", "renumber", "stage", "drop")) assertTrue(key, list(key).isNotEmpty())
    }

    @Test
    fun insert() {
        for (case in list("insert")) {
            val input = case.getValue("input").jsonObject
            val expected = case.getValue("expected").jsonObject
            assertEquals(
                case.str("name"),
                ComposerAttachments.Placed(expected.str("text"), expected.num("caret")),
                ComposerAttachments.insert(input.str("text"), input.num("caret"), input.num("index")),
            )
        }
    }

    @Test
    fun renumber() {
        for (case in list("renumber")) {
            val input = case.getValue("input").jsonObject
            assertEquals(
                case.str("name"),
                case.getValue("expected").jsonPrimitive.content,
                ComposerAttachments.renumber(input.str("text"), input.num("removed")),
            )
        }
    }

    @Test
    fun stage() {
        for (case in list("stage")) {
            val input = case.getValue("input").jsonObject
            val expected = case.getValue("expected").jsonObject
            val staged = ComposerAttachments.stage(
                input.kinds("strip"),
                input.getValue("add").jsonArray.map { pick(it.jsonPrimitive.content) },
                input.str("text"),
                input.num("caret"),
            )
            val name = case.str("name")
            assertEquals(name, expected.kinds("strip"), staged.strip)
            assertEquals(name, expected.getValue("taken").jsonArray.map { it.jsonPrimitive.boolean }, staged.taken)
            assertEquals(name, expected.str("text"), staged.text)
            assertEquals(name, expected.num("caret"), staged.caret)
            assertEquals(name, expected.num("rejected"), staged.rejected)
            assertEquals(name, expected.num("overflow"), staged.overflow)
            assertEquals(name, expected.num("fileOverflow"), staged.fileOverflow)
            assertEquals(name, expected.getValue("toasts").jsonArray.map { it.jsonPrimitive.content }, staged.toasts)
        }
    }

    @Test
    fun drop() {
        for (case in list("drop")) {
            val input = case.getValue("input").jsonObject
            val expected = case.getValue("expected").jsonObject
            assertEquals(
                case.str("name"),
                ComposerAttachments.Dropped(expected.kinds("strip"), expected.str("text")),
                ComposerAttachments.drop(input.kinds("strip"), input.num("at"), input.str("text")),
            )
        }
    }
}

package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1215: every confirm/choice prompt's wording, locked against
 * `prompts.json` (web `prompts.test.ts`, iOS `PromptsTests`): the entry set,
 * every title/body template, the params, the actions (id, label, role, display
 * order) and the focus.
 */
class PromptsTest {

    private fun fixture(): JsonObject =
        Json.parseToJsonElement(contractFixtureJson("prompts.json")).jsonObject

    private fun entries(): JsonObject = fixture().getValue("prompts").jsonObject

    /** Keys of an entry that are not copy. */
    private val structural = setOf("params", "actions", "focus", "variants", "slot")

    @Test
    fun `the roles are the contract's`() {
        assertEquals(
            fixture().getValue("roles").jsonArray.map { it.jsonPrimitive.content },
            Prompts.Role.entries.map { it.key },
        )
    }

    @Test
    fun `the entry set is the contract's`() {
        assertEquals(entries().keys, Prompts.all.keys)
    }

    @Test
    fun `every entry's copy, params, actions and focus are the contract's`() {
        for ((id, element) in entries()) {
            val entry = element.jsonObject
            val spec = Prompts.all.getValue(id)
            val copy = entry.filterKeys { it !in structural }.mapValues { it.value.jsonPrimitive.content }
            assertEquals("$id copy", copy, spec.copy)
            assertEquals(
                "$id params",
                entry.getValue("params").jsonArray.map { it.jsonObject.getValue("name").jsonPrimitive.content },
                spec.params,
            )
            assertEquals(
                "$id actions",
                entry.getValue("actions").jsonArray.map {
                    val action = it.jsonObject
                    Triple(
                        action.getValue("id").jsonPrimitive.content,
                        action.getValue("label").jsonPrimitive.content,
                        action.getValue("role").jsonPrimitive.content,
                    )
                },
                spec.actions.map { Triple(it.id, it.label, it.role.key) },
            )
            assertEquals("$id focus", entry.getValue("focus").jsonPrimitive.content, spec.focus)
        }
    }

    @Test
    fun `every placeholder is a declared param`() {
        val placeholder = Regex("\\{(\\w+)\\}")
        for (spec in Prompts.all.values) {
            for (template in spec.copy.values) {
                for (match in placeholder.findAll(template)) {
                    assertTrue("${spec.id}: ${match.value}", match.groupValues[1] in spec.params)
                }
            }
        }
    }

    @Test
    fun `focus never lands on a destructive answer`() {
        for (spec in Prompts.all.values) {
            val focused = spec.actions.single { it.id == spec.focus }
            assertTrue(spec.id, focused.role != Prompts.Role.Destructive && focused.role != Prompts.Role.QuietDestructive)
        }
    }

    @Test
    fun `the builders fill every param`() {
        val rendered = listOf(
            Prompts.DeleteIssue.prompt("EXP-12"),
            Prompts.DeleteIssues.prompt(1),
            Prompts.DeleteIssues.prompt(3),
            Prompts.DeleteFile.prompt("a.png"),
            Prompts.MoveIssue.prompt("EXP-12", "Web"),
            Prompts.MergeIssuePr.prompt(7),
            Prompts.MergeIssuePr.prompt(null, 3),
            Prompts.MergeRunPr.prompt(7),
            Prompts.MergeRunPr.prompt(null),
            Prompts.StopRun.prompt(),
            Prompts.ResumeRun.prompt("Mac"),
            Prompts.ResumeRun.prompt(null),
            Prompts.DeleteTrigger.prompt(),
            Prompts.RemoveDevice.prompt("Mac"),
            Prompts.DeleteTeam.prompt("Acme"),
            Prompts.TrashBoard.prompt("Web"),
            Prompts.DeleteLabel.prompt("bug"),
            Prompts.RemoveMember.prompt("Ada"),
            Prompts.LeaveTeam.prompt("Acme"),
            Prompts.MakeOwner.prompt("Ada"),
            Prompts.MakeMember.prompt("Ada"),
            Prompts.RemoveRepository.prompt("niach/exponential"),
            Prompts.UnlinkSignInMethod.prompt("Google"),
            Prompts.RemovePassword.prompt(),
            Prompts.RemovePasskey.prompt(null),
            Prompts.DeleteAccount.prompt("Cloud"),
            Prompts.RemoveServer.prompt("Cloud"),
        )
        for (prompt in rendered) {
            assertTrue(prompt.title, '{' !in prompt.title && '{' !in (prompt.body ?: ""))
        }
    }

    @Test
    fun `the variants pick the right keys`() {
        assertEquals("Delete EXP-12?", Prompts.DeleteIssue.prompt("EXP-12").title)
        assertEquals("Delete 1 issue?", Prompts.DeleteIssues.prompt(1).title)
        assertEquals("Their comments and files are deleted with them.", Prompts.DeleteIssues.prompt(2).body)
        assertEquals("Delete 2 issues?", Prompts.DeleteIssues.prompt(2).title)
        assertEquals("Merge PR #7?", Prompts.MergeIssuePr.prompt(7).title)
        assertEquals("It is squash-merged.", Prompts.MergeIssuePr.prompt(7).body)
        assertEquals("It is squash-merged. It covers 3 issues.", Prompts.MergeIssuePr.prompt(7, 3).body)
        assertEquals("Merge this pull request?", Prompts.MergeRunPr.prompt(null).title)
        assertEquals("Resume this run?", Prompts.ResumeRun.prompt(" ").title)
        assertEquals("Resume this run on Mac?", Prompts.ResumeRun.prompt("Mac").title)
        assertNull(Prompts.StopRun.prompt().body)
        assertEquals("Remove the passkey \"Passkey\"?", Prompts.RemovePasskey.prompt("").title)
        assertEquals("Delete your account on Cloud?", Prompts.DeleteAccount.prompt("Cloud").title)
        assertEquals("Move \"Web\" to trash?", Prompts.TrashBoard.prompt("Web").title)
    }
}

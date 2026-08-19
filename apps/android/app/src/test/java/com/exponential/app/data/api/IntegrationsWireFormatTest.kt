package com.exponential.app.data.api

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-557 (per-user repo sharing) wire-format locks: the
 * `integrations.github.unlink` mutation payload, the additive `stale` flag on
 * `installations[]` entries (optional-with-default — old servers omit it, and
 * an unknown-to-old-clients field must never break decoding), and the additive
 * `sharedBy` object on `repositories.list` rows.
 */
class IntegrationsWireFormatTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun `unlink input is the flat teamId + installationId payload`() {
        assertEquals(
            """{"teamId":"team-1","installationId":81577533}""",
            json.encodeToString(
                UnlinkInput.serializer(),
                UnlinkInput(teamId = "team-1", installationId = 81577533L),
            ),
        )
    }

    @Test
    fun `installations entry without stale defaults to false`() {
        val inst = json.decodeFromString(
            GithubInstallation.serializer(),
            """{"installationId":1,"accountLogin":"acme","manageUrl":"https://github.com/settings/installations/1"}""",
        )
        assertFalse(inst.stale)
        assertFalse(inst.needsReauth)
    }

    @Test
    fun `installations entry with stale true parses`() {
        val inst = json.decodeFromString(
            GithubInstallation.serializer(),
            """{"installationId":1,"accountLogin":"acme","manageUrl":"https://github.com/settings/installations/1","needsReauth":true,"stale":true}""",
        )
        assertTrue(inst.stale)
        assertTrue(inst.needsReauth)
    }

    @Test
    fun `repo row without sharedBy parses to null`() {
        val repo = json.decodeFromString(
            TeamRepo.serializer(),
            """{"id":"repo-1","fullName":"acme/api","defaultBranch":"main","private":true,"boards":[]}""",
        )
        assertNull(repo.sharedBy)
    }

    @Test
    fun `repo row with sharedBy parses, name nullable`() {
        val repo = json.decodeFromString(
            TeamRepo.serializer(),
            """{"id":"repo-1","fullName":"acme/api","defaultBranch":"main","private":false,"boards":[],""" +
                """"sharedBy":{"id":"user-1","name":null,"email":"dev@acme.test"}}""",
        )
        assertEquals("user-1", repo.sharedBy?.id)
        assertNull(repo.sharedBy?.name)
        assertEquals("dev@acme.test", repo.sharedBy?.email)
    }
}

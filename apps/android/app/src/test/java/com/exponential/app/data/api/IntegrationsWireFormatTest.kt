package com.exponential.app.data.api

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * SLOP-26 wire-format locks: the `integrations.github.disconnect` payload
 * (`{}`), the connection fields `linked`/`needsReconnect`/`login` (with the
 * pre-SLOP-7 fallbacks), the prerequisite ladder every surface names, the
 * by-name lookup (FEED-30) and the additive `sharedBy` on `repositories.list`.
 */
class IntegrationsWireFormatTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun `disconnect input is the empty object`() {
        assertEquals("{}", json.encodeToString(DisconnectInput.serializer(), DisconnectInput))
    }

    // FEED-30: the by-name lookup sends the flat teamId + fullName payload the
    // server's z.object expects, and its result decodes as a picker row.
    @Test
    fun `lookupRepo input is the flat teamId + fullName payload`() {
        assertEquals(
            """{"teamId":"team-1","fullName":"acme/web"}""",
            json.encodeToString(
                LookupRepoInput.serializer(),
                LookupRepoInput(teamId = "team-1", fullName = "acme/web"),
            ),
        )
    }

    @Test
    fun `lookupRepo result decodes as a picker row`() {
        val repo = json.decodeFromString(
            GithubPickerRepo.serializer(),
            """{"fullName":"acme/web","private":true,"defaultBranch":"trunk","installationId":7}""",
        )
        assertEquals("acme/web", repo.fullName)
        assertTrue(repo.isPrivate)
        assertEquals("trunk", repo.defaultBranch)
        assertEquals(7, repo.installationId)
    }

    @Test
    fun `status decodes the connection fields`() {
        val status = json.decodeFromString(
            GithubStatusResult.serializer(),
            """{"configured":true,"connectConfigured":true,"linked":true,"needsReconnect":false,"login":"octocat",""" +
                """"installed":true,"installUrl":"https://github.com/apps/exp/installations/new",""" +
                """"connectUrl":"https://app.test/integrations/github?team=t&return=app",""" +
                """"installations":[{"installationId":7,"accountLogin":"acme","accountType":"Organization",""" +
                """"manageUrl":"https://github.com/organizations/acme/settings/installations/7",""" +
                """"suspended":false,"needsReauth":false,"stale":false}]}""",
        )
        assertTrue(status.isLinked)
        assertFalse(status.needsReconnect)
        assertEquals("octocat", status.login)
        assertNull(status.prerequisite)
        assertEquals("acme", status.installations.single().accountLogin)
        assertFalse(status.installations.single().suspended)
    }

    @Test
    fun `repos without the new fields falls back to installed`() {
        val repos = json.decodeFromString(
            GithubReposResult.serializer(),
            """{"configured":true,"installed":false,"installUrl":null,"repos":[],"hasMore":false}""",
        )
        assertFalse(repos.isLinked)
        assertFalse(repos.needsReconnect)
        assertNull(repos.login)
        assertEquals(GithubPrerequisite.NOT_LINKED, repos.prerequisite)
    }

    @Test
    fun `prerequisite ladder`() {
        assertEquals(GithubPrerequisite.NOT_CONFIGURED, githubPrerequisite(configured = false, linked = true, needsReconnect = false, installed = true))
        assertEquals(GithubPrerequisite.NOT_LINKED, githubPrerequisite(configured = true, linked = false, needsReconnect = false, installed = false))
        assertEquals(GithubPrerequisite.EXPIRED, githubPrerequisite(configured = true, linked = true, needsReconnect = true, installed = false))
        assertEquals(GithubPrerequisite.NOT_INSTALLED, githubPrerequisite(configured = true, linked = true, needsReconnect = false, installed = false))
        assertNull(githubPrerequisite(configured = true, linked = true, needsReconnect = false, installed = true))
    }

    @Test
    fun `installations entry ignores the legacy marks and defaults suspended`() {
        val inst = json.decodeFromString(
            GithubInstallation.serializer(),
            """{"installationId":1,"accountLogin":"acme","manageUrl":"https://github.com/settings/installations/1","needsReauth":true,"stale":true}""",
        )
        assertEquals("acme", inst.accountLogin)
        assertFalse(inst.suspended)
        assertFalse(inst.hasMore)
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

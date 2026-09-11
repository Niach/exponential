package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// FEED-30/32: the "Add by name" shape check mirrors the web REPO_FULL_NAME_RE,
// and the board form's repository label is never blank for a linked board
// whose repo the local list can't resolve (web board-repo-field.tsx parity).
class RepoFullNameTest {
    @Test
    fun acceptsExactlyOwnerSlashName() {
        for (ok in listOf("acme/web", "a/b", "org-name/repo.name", "Niach/exponential")) {
            assertTrue(ok, isRepoFullName(ok))
        }
    }

    @Test
    fun rejectsEverythingElse() {
        for (bad in listOf("", "acme", "acme/", "/web", "acme/web/extra", "acme /web", "acme/we b", " acme/web", "acme/web\n")) {
            assertFalse(bad, isRepoFullName(bad))
        }
    }

    @Test
    fun unlinkedBoardReadsNoRepository() {
        assertEquals("No repository", BoardRepoLabel.trigger(null, null, loading = false, resolving = false))
        assertEquals("Loading…", BoardRepoLabel.trigger(null, null, loading = true, resolving = false))
    }

    @Test
    fun resolvedRepoWinsOverEveryFallback() {
        assertEquals("acme/web", BoardRepoLabel.trigger("acme/web", "repo-1", loading = true, resolving = true))
    }

    @Test
    fun unknownLinkedRepoNeverRendersBlank() {
        assertEquals("Loading repository…", BoardRepoLabel.trigger(null, "repo-1", loading = true, resolving = false))
        assertEquals("Loading repository…", BoardRepoLabel.trigger(null, "repo-1", loading = false, resolving = true))
        assertEquals("Repository unavailable", BoardRepoLabel.trigger(null, "repo-1", loading = false, resolving = false))
    }
}

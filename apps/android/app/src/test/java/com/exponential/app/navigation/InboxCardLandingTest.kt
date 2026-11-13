package com.exponential.app.navigation

import com.exponential.app.data.push.PushDeepLinks
import com.exponential.app.domain.ChatRowAction
import com.exponential.app.domain.DeepLinkDriver
import com.exponential.app.domain.InboxRoute
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.WaitingCards.DeepLinkStep
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * waiting-cards.json DEEP LINKS: an inbox row or a push tap lands on the card
 * it is about (the push's `messageId`, else the session's waiting card), the
 * chat's `?card=` pending deep link, not on the session's newest rows.
 */
class InboxCardLandingTest {

    private val chat = InboxRoute.SessionTarget.Chat("root", null)

    private fun message(i: Int, card: JsonObject? = null) = SessionTree.Message(
        id = "m$i",
        sessionId = "root",
        authorKind = "agent",
        authorId = null,
        body = "row $i",
        card = card,
        answeredAt = null,
        consumedAt = null,
        createdAt = "2026-11-13T00:%02d:%02d.000Z".format(i / 60, i % 60),
    )

    @Test
    fun `an inbox row lands on a waiting card outside the newest 200 rows`() {
        // 250 messages, the waiting question is the OLDEST: outside the
        // chat's newest-200 window, still the session's waiting card.
        val question = JsonObject(mapOf("kind" to JsonPrimitive("question")))
        val messages = (0 until 250).map { i -> message(i, if (i == 10) question else null) }
        val card = ChatRowAction.sessionWaitingCard("root", messages)
        assertEquals("m10", card)

        val landing = InboxRoute.landing(chat, "root", card, plan = false)
        assertEquals("chat/root?card=m10", landingRoute(landing))

        // The chat takes it as THE pending deep link: one older page at a
        // time until the card is loaded, then centred and rung.
        val driver = DeepLinkDriver()
        driver.take(card!!, 0)
        var loadedFrom = 250 - 200
        var landingIn = 0
        val steps = mutableListOf<DeepLinkStep>()
        var centred = false
        var now = 0L
        while (driver.link != null && now < 20_000) {
            now += 16
            if (landingIn > 0 && --landingIn == 0) loadedFrom = maxOf(0, loadedFrom - 200)
            val found = 10 >= loadedFrom
            val step = driver.step(found, found, found && centred, loadedFrom > 0, 250 - loadedFrom, now) ?: break
            if (steps.lastOrNull() != step) steps += step
            when (step) {
                DeepLinkStep.Page -> landingIn = 3
                DeepLinkStep.Scroll -> centred = true
                else -> Unit
            }
        }
        assertEquals(listOf(DeepLinkStep.Page, DeepLinkStep.Wait, DeepLinkStep.Scroll, DeepLinkStep.Ring), steps)
    }

    @Test
    fun `a question push carries its card through the link and lands the chat on it`() {
        val card = PushDeepLinks.card("m1", "question", "root")
        val target = PushDeepLinks.target(type = "agent_message", issueId = null, sessionId = "root", card = card)
        assertEquals(PushDeepLinks.Target.Session("root", card), target)
        assertEquals(
            "exponential://session/root?card=m1&rootId=root&userId=u1",
            PushDeepLinks.uri(target!!, "u1"),
        )
        // Not synced yet (a cold tap): the push's own rootId still lands it.
        assertEquals(
            "chat/root?card=m1",
            landingRoute(InboxRoute.landing(null, card!!.rootId, card.messageId, card.plan)),
        )
    }

    @Test
    fun `a plan push opens THE plan page`() {
        val card = PushDeepLinks.card("p1", "plan", "root")!!
        assertEquals(
            "exponential://session/child?card=p1&cardKind=plan&rootId=root",
            PushDeepLinks.uri(PushDeepLinks.Target.Session("child", card), null),
        )
        assertEquals(
            "chat/root?tab=plan&plan=p1",
            landingRoute(InboxRoute.landing(InboxRoute.SessionTarget.Chat("root", "child"), "root", "p1", card.plan)),
        )
    }

    @Test
    fun `no card, no root, or a session on its own screen opens the session as before`() {
        assertEquals(InboxRoute.Landing.Session, InboxRoute.landing(chat, "root", null, plan = false))
        assertEquals(InboxRoute.Landing.Session, InboxRoute.landing(null, null, "m1", plan = false))
        assertEquals(
            InboxRoute.Landing.Session,
            InboxRoute.landing(InboxRoute.SessionTarget.Session("s1"), "root", "m1", plan = false),
        )
        assertEquals(null, PushDeepLinks.card(null, "question", "root"))
        assertEquals(
            "exponential://session/s1",
            PushDeepLinks.uri(PushDeepLinks.Target.Session("s1"), null),
        )
    }
}

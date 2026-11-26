package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.ElicitationCard
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.chat.ChatAskCard
import com.exponential.app.ui.session.LocalCardDismiss
import com.exponential.app.ui.theme.AppBackground
import com.exponential.app.ui.theme.TextEmphasis
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject

/*
 * #NEW-146 styleguide `elicitation-card` (web `ElicitationCardSpecimen`):
 * the fixture's device-built cards (elicitation-card.json `cardCases`, copied
 * here; ElicitationCardSpecimenTest compares them to the fixture) through the
 * REAL row model ([SessionTree.askRow]) and [ChatAskCard]: the waiting form,
 * the clean url, the warned url, an answered form, captioned `copy.specimen`.
 * Capture only (StyleguideScreenshotsTest `captureOnlyScreens`).
 */

const val ElicitationCardSpecimenTestId = "elicitation-card-specimen"

/** `cardCases` "a form with every field kind…". */
internal const val ELICITATION_SPECIMEN_FORM = """{"kind":"question","question":"Where should the issue go?","options":["Submit","Decline","Cancel"],"custom":false,"multi":false,"surface":{"root":{"id":"root","component":"Stack","props":{"direction":"vertical","gap":"md"},"children":[{"id":"f_title","component":"Input","props":{"label":"Title","name":"title","type":"text","value":{"path":"/form/title"}}},{"id":"f_team","component":"Select","props":{"label":"Team","name":"team","options":[{"label":"Web","value":"web"},{"label":"iOS","value":"ios"}],"value":{"path":"/form/team"}}},{"id":"f_labels","component":"Select","props":{"label":"Labels","name":"labels","options":[{"label":"bug","value":"bug"},{"label":"crash","value":"crash"},{"label":"p1","value":"p1"}],"value":{"path":"/form/labels"},"multiple":true}},{"id":"f_estimate","component":"NumberField","props":{"label":"Estimate","name":"estimate","value":{"path":"/form/estimate"},"min":1,"max":8,"step":1,"precision":0}},{"id":"f_points","component":"NumberField","props":{"label":"points","name":"points","value":{"path":"/form/points"},"placeholder":"Confidence"}},{"id":"f_due","component":"DatePicker","props":{"label":"Due","name":"due","value":{"path":"/form/due"}}},{"id":"f_email","component":"Input","props":{"label":"Reporter","name":"email","type":"email","value":{"path":"/form/email"},"placeholder":"Who reported it"}},{"id":"f_link","component":"Input","props":{"label":"Link","name":"link","type":"url","value":{"path":"/form/link"}}},{"id":"f_notify","component":"Switch","props":{"label":"Notify the team","name":"notify","checked":{"path":"/form/notify"},"description":"Posts in the team channel."}}]},"data":{"form":{"title":"Crash on launch","team":"ios","labels":["bug"],"estimate":null,"points":null,"due":"","email":"","link":"","notify":true}}},"elicitation":{"server":"linear","mode":"form","required":["title","team"],"url":null}}"""

/** `cardCases` "a url: the domain…". */
internal const val ELICITATION_SPECIMEN_URL = """{"kind":"question","question":"Sign in to GitHub","options":["I'm done","Decline"],"custom":false,"multi":false,"surface":{"root":{"id":"root","component":"Stack","props":{"direction":"vertical","gap":"sm"},"children":[{"id":"domain","component":"Heading","props":{"text":"github.com","level":"h4"}},{"id":"url","component":"Text","props":{"text":"https://github.com/login/oauth/authorize?client_id=abc&state=xyz","variant":"code"}}]},"data":{}},"elicitation":{"server":"github","mode":"url","required":[],"url":"https://github.com/login/oauth/authorize?client_id=abc&state=xyz"}}"""

/** `cardCases` "a url on http…". */
internal const val ELICITATION_SPECIMEN_WARNED = """{"kind":"question","question":"Pay the invoice","options":["I'm done","Decline"],"custom":false,"multi":false,"surface":{"root":{"id":"root","component":"Stack","props":{"direction":"vertical","gap":"sm"},"children":[{"id":"domain","component":"Heading","props":{"text":"xn--pple-43d.co.uk","level":"h4"}},{"id":"url","component":"Text","props":{"text":"http://pay.xn--pple-43d.co.uk/invoice/42","variant":"code"}},{"id":"warn_punycode","component":"Alert","props":{"title":"This address uses look-alike characters.","type":"warning"}},{"id":"warn_insecure","component":"Alert","props":{"title":"This link is not encrypted (no https).","type":"warning"}}]},"data":{}},"elicitation":{"server":"billing","mode":"url","required":[],"url":"http://pay.xn--pple-43d.co.uk/invoice/42"}}"""

/** The answered form: Submit with the defaults + an estimate and a due date. */
internal fun elicitationSpecimenAnswered(): JsonObject {
    val form = Json.parseToJsonElement(ELICITATION_SPECIMEN_FORM).jsonObject
    val data = form.getValue("surface").jsonObject.getValue("data").jsonObject.getValue("form").jsonObject
    return JsonObject(
        form + mapOf(
            "answer" to JsonPrimitive(ElicitationCard.Copy.SUBMIT),
            "answers" to buildJsonArray { add(JsonPrimitive(ElicitationCard.Copy.SUBMIT)) },
            "surfaceData" to buildJsonObject {
                put("form", JsonObject(data + mapOf("estimate" to JsonPrimitive(3), "due" to JsonPrimitive("2026-12-01"))))
            },
        ),
    )
}

private fun specimenRow(id: String, card: JsonObject, answeredAt: String? = null): SessionTree.SessionChatRow? =
    SessionTree.askRow(
        SessionTree.Message(
            id = id,
            sessionId = "s-1",
            authorKind = "agent",
            authorId = null,
            body = "",
            card = card,
            answeredAt = answeredAt,
            consumedAt = null,
            createdAt = "2026-11-26T12:00:00.000Z",
        ),
    )

@Composable
fun ElicitationCardSpecimenScreen() {
    val cases = remember {
        listOf(
            ElicitationCard.Copy.Specimen.FORM to specimenRow("e1", Json.parseToJsonElement(ELICITATION_SPECIMEN_FORM).jsonObject),
            ElicitationCard.Copy.Specimen.URL to specimenRow("e2", Json.parseToJsonElement(ELICITATION_SPECIMEN_URL).jsonObject),
            ElicitationCard.Copy.Specimen.WARNED to specimenRow("e3", Json.parseToJsonElement(ELICITATION_SPECIMEN_WARNED).jsonObject),
            ElicitationCard.Copy.Specimen.ANSWERED to specimenRow("e4", elicitationSpecimenAnswered(), "2026-11-26T12:00:00.000Z"),
        )
    }
    // The waiting cards carry THE × (web passes a no-op dismiss too).
    AppBackground(Modifier.testTag(ElicitationCardSpecimenTestId)) {
        CompositionLocalProvider(LocalCardDismiss provides { _, _ -> }) {
            Column(
                Modifier.fillMaxSize().statusBarsPadding().verticalScroll(rememberScrollState()).padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(24.dp),
            ) {
                cases.forEach { (caption, row) ->
                    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        Text(
                            caption,
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                        )
                        if (row != null) ChatAskCard(row, onAnswer = { _, _, _ -> }, onDecidePlan = { _, _, _ -> })
                    }
                }
            }
        }
    }
}

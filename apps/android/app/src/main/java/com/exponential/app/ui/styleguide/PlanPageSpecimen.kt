package com.exponential.app.ui.styleguide

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import com.exponential.app.domain.PlanPage
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.plan.PlanPageContent
import com.exponential.app.ui.plan.PlanPageState
import com.exponential.app.ui.theme.AppBackground
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject

// #EXP-1493 THE styleguide specimen ×4 (`plan-page`, plan-page.json
// `specimen`): this card, waiting, drawn phone (the document from its top,
// the DockCard at the foot; pictures = placeholders, the muted fill). Web
// `PlanPageSpecimen`, iOS ChatSpecimens `plan-page`, desktop plan_page.rs.

const val PlanPageSpecimenTestId = "plan-page-specimen"

/** plan-page.json `specimen.card`, verbatim. */
private const val SPECIMEN_CARD = """{"kind": "plan", "plan": "# Plan: editor-first app creation\n\nA new app opens straight in the editor; every app gets an MCP endpoint.\n\n## Current state\n- Creating an app starts from a form.\n- The editor opens only after the first save.\n\n## Part A: editor-first creation\nThe editor opens on an empty canvas with the chat docked.\n\n![web editor new](/api/attachments/sg-1)\n![web editor preview](/api/attachments/sg-2)\n![web editor data](/api/attachments/sg-3)\n![web editor versions](/api/attachments/sg-4)\n\n## Part B: MCP for eApps\nEach team's apps answer at one MCP endpoint, scoped by a team token.\n\n## Phased issues\n1. The contract\n2. The editor\n3. The MCP endpoint", "decision": null, "pictures": [{"label": "web-editor-1", "attachmentId": "sg-1", "width": 1440, "height": 900}, {"label": "web-editor-2", "attachmentId": "sg-2", "width": 1440, "height": 900}, {"label": "web-editor-3", "attachmentId": "sg-3", "width": 1440, "height": 900}, {"label": "web-editor-4", "attachmentId": "sg-4", "width": 1440, "height": 900}], "questions": [{"id": "endpoint", "prompt": "How should clients address the MCP endpoint for a team's apps?", "multi": false, "custom": true, "options": [{"label": "One endpoint per team", "description": "/mcp/t/acme, tools prefixed by app"}, {"label": "One endpoint per app", "description": "/mcp/apps/:id"}]}, {"id": "chat", "prompt": "Keep the floating chat layout in the editor?", "multi": false, "custom": false, "options": [{"label": "Keep it"}, {"label": "Dock it right"}]}]}"""

internal val PlanPageSpecimenCard: JsonObject = Json.parseToJsonElement(SPECIMEN_CARD).jsonObject

/** The page's state over the specimen card (`sessionId` apps, message plan). */
internal fun planPageSpecimenState(card: JsonObject = PlanPageSpecimenCard): PlanPageState {
    val message = SessionTree.Message(
        id = "plan",
        sessionId = "apps",
        authorKind = "agent",
        authorId = null,
        body = "",
        card = card,
        answeredAt = null,
        consumedAt = null,
        createdAt = "2027-01-15T08:00:00.000Z",
        authorSessionId = "apps",
    )
    return PlanPageState(
        sessionId = "apps",
        messageId = "plan",
        card = card,
        row = SessionTree.askRow(message) as? SessionTree.ThreadRow.Plan,
        sessionName = PlanPage.Specimen.CHAT_NAME,
        loading = false,
    )
}

/** `plan-page`: THE plan page, phone, over the specimen card. */
@Composable
fun PlanPageSpecimenScreen() {
    AppBackground(Modifier.testTag(PlanPageSpecimenTestId)) {
        Box(Modifier.fillMaxSize()) {
            PlanPageContent(
                state = planPageSpecimenState(),
                onBack = {},
                onDecidePlan = { _, _, settled -> settled(false) },
                chatName = PlanPage.Specimen.CHAT_NAME,
                activeSection = PlanPage.Specimen.ACTIVE_SECTION,
            )
        }
    }
}

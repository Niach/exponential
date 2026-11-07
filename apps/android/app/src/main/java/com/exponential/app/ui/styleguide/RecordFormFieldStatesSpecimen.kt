package com.exponential.app.ui.styleguide

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.host.NoHost
import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.model.fieldFocused
import kotlinx.coroutines.flow.first

const val RecordFormFieldStatesSpecimenTestId = "record-form-field-states-specimen"

private val FIELDS = listOf(
    Triple("email", "Email", "ada@") to "/error",
    Triple("password", "Password", "hunter2") to "/warning",
    Triple("nickname", "Nickname", "ada") to "/info",
)

private fun fieldStates(): String = FIELDS.joinToString(
    prefix = """{"id":"field-states","component":"Stack","props":{"gap":"md"},"children":[""",
    postfix = "]}",
) { (field, path) ->
    val (id, label, value) = field
    """{"id":"$id","component":"Input","props":{"label":"$label","name":"$id","value":"$value","validateOn":"blur","checks":[{"condition":{"@path":"$path"}}]}}"""
}

private fun failing(severity: String, message: String) = JsonValue.Obj(
    mapOf("valid" to JsonValue.Bool(false), "severity" to JsonValue.Str(severity), "message" to JsonValue.Str(message)),
)

/**
 * Round 9 §6.6: the `record-form` entry's field states (web
 * `ExponentialUiFieldStates`, under the RecordForm specimen): one Input per
 * check severity with its message showing: error = the invalid state, warning
 * and info advisory. Each field blurs once, so the core reveals its check.
 */
@Composable
fun RecordFormFieldStatesSpecimenScreen() {
    val model = remember {
        SurfaceModel("sg-field-states", SurfaceOptions(), NoHost) { error("set by ExponentialSurface") }.also { m ->
            m.setNested(fieldStates())
            m.setData("/error", failing("error", "Not an email"))
            m.setData("/warning", failing("warning", "Weak: add a few more characters"))
            m.setData("/info", failing("info", "Shown on your profile"))
        }
    }
    LaunchedEffect(model) {
        snapshotFlow { model.indexOf("nickname.field") }.first { it != null }
        for ((field) in FIELDS) model.indexOf("${field.first}.field")?.let { model.fieldFocused(it, false) }
    }
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background)
            .padding(16.dp)
            .testTag(RecordFormFieldStatesSpecimenTestId),
    ) {
        ExponentialSurface(model)
    }
}

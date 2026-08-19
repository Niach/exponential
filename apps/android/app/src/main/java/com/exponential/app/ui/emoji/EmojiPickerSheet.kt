package com.exponential.app.ui.emoji

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.exponential.app.ExponentialApp
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetSearchField
import com.exponential.app.ui.theme.AccentIndigo
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import dagger.hilt.android.EntryPointAccessors

// EXP-551 — the emoji picker sheet. Every emoji affordance on Android (the
// floating markdown toolbar, the comment composer, the comment editor) opens
// THIS sheet, and every pick inserts the UNICODE, never `:shortcode:` text.

private const val TONE_SAMPLE_SHORTCODE = "hand"
private const val PICKER_LIMIT = 64

/**
 * Search + skin tone + a grouped grid, in the shared glass sheet chrome. The
 * search results REPLACE the grouped grid while a query is typed; picking
 * records the base unicode in recents, hands the toned unicode to [onPick] and
 * dismisses.
 */
@Composable
fun EmojiPickerSheet(
    onPick: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val data = rememberEmojiData(enabled = true)
    val prefs = rememberEmojiPrefs()
    var query by remember { mutableStateOf("") }
    var tone by remember { mutableIntStateOf(prefs.skinTone()) }
    var recents by remember { mutableStateOf(prefs.recents()) }

    val results = remember(data, query) {
        if (data == null || query.isBlank()) emptyList() else data.search(query, PICKER_LIMIT)
    }

    fun pick(unicode: String, base: String) {
        recents = prefs.pushRecent(base)
        onPick(unicode)
        onDismiss()
    }

    GlassSheet(title = "Emoji", onDismiss = onDismiss) {
        GlassSheetSearchField(
            value = query,
            onValueChange = { query = it },
            placeholder = "Search emoji",
        )
        Spacer(Modifier.height(8.dp))
        SkinToneRow(
            data = data,
            tone = tone,
            onSelect = {
                tone = it
                prefs.setSkinTone(it)
            },
        )
        Spacer(Modifier.height(4.dp))
        if (data == null) {
            Text(
                "Loading emoji…",
                style = MaterialTheme.typography.bodyMedium,
                color = Color.White.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier.padding(horizontal = 20.dp, vertical = 24.dp),
            )
            return@GlassSheet
        }
        LazyVerticalGrid(
            columns = GridCells.Adaptive(minSize = 44.dp),
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(max = 380.dp)
                .padding(horizontal = 12.dp),
            horizontalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            if (query.isNotBlank()) {
                if (results.isEmpty()) {
                    item(span = { GridItemSpan(maxLineSpan) }) {
                        SectionHeader("No emoji match \"${query.trim()}\"")
                    }
                }
                items(results) { emoji ->
                    EmojiCell(applySkinTone(emoji, tone), emoji.label) { unicode ->
                        pick(unicode, emoji.unicode)
                    }
                }
                return@LazyVerticalGrid
            }
            // Recents first (base unicodes, so a tone change re-tones them),
            // then the dataset's nine groups in order.
            val recentRecords = recents.mapNotNull { data.findUnicode(it) }
            if (recentRecords.isNotEmpty()) {
                item(span = { GridItemSpan(maxLineSpan) }) { SectionHeader("Recent") }
                items(recentRecords) { emoji ->
                    EmojiCell(applySkinTone(emoji, tone), emoji.label) { unicode ->
                        pick(unicode, emoji.unicode)
                    }
                }
            }
            data.groups.forEach { group ->
                if (group.emojis.isEmpty()) return@forEach
                item(span = { GridItemSpan(maxLineSpan) }) { SectionHeader(group.label) }
                items(group.emojis) { emoji ->
                    EmojiCell(applySkinTone(emoji, tone), emoji.label) { unicode ->
                        pick(unicode, emoji.unicode)
                    }
                }
            }
        }
    }
}

@Composable
private fun SectionHeader(label: String) {
    Text(
        label,
        style = MaterialTheme.typography.labelMedium,
        color = Color.White.copy(alpha = TextEmphasis.Tertiary),
        modifier = Modifier.padding(start = 8.dp, top = 12.dp, bottom = 4.dp),
    )
}

@Composable
private fun EmojiCell(unicode: String, label: String, onClick: (String) -> Unit) {
    Box(
        modifier = Modifier
            .size(44.dp)
            .clip(RoundedCornerShape(10.dp))
            .clickable(onClickLabel = label) { onClick(unicode) },
        contentAlignment = Alignment.Center,
    ) {
        Text(unicode, fontSize = 24.sp, textAlign = TextAlign.Center)
    }
}

/**
 * "None" plus the five uniform tones, sampled on ✋ so the swatches show the
 * actual skin tone rather than a color chip. The sample comes from the dataset
 * (never a hardcoded sequence) so it can't drift from what picks insert.
 */
@Composable
private fun SkinToneRow(data: EmojiData?, tone: Int, onSelect: (Int) -> Unit) {
    val sample = data?.findShortcode(TONE_SAMPLE_SHORTCODE) ?: return
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        for (option in 0..EMOJI_TONES) {
            val selected = option == tone
            Box(
                modifier = Modifier
                    .size(34.dp)
                    .clip(RoundedCornerShape(10.dp))
                    .background(if (selected) GlassTokens.RowFill else Color.Transparent)
                    .border(
                        if (selected) 1.5.dp else 1.dp,
                        if (selected) AccentIndigo else Color.White.copy(alpha = TextEmphasis.Quaternary),
                        RoundedCornerShape(10.dp),
                    )
                    .clickable(
                        onClickLabel = if (option == 0) "Default skin tone" else "Skin tone $option",
                    ) { onSelect(option) },
                contentAlignment = Alignment.Center,
            ) {
                Text(applySkinTone(sample, option), fontSize = 18.sp)
            }
        }
    }
}

// --- Hosting helpers --------------------------------------------------------

/**
 * The shared dataset, parsed once per process off the main thread. Nothing
 * loads until [enabled] first goes true, so a screenful of editors costs
 * nothing until a picker opens or a `:xx` token is typed.
 */
@Composable
fun rememberEmojiData(enabled: Boolean): EmojiData? {
    val context = LocalContext.current
    var data by remember { mutableStateOf<EmojiData?>(null) }
    LaunchedEffect(enabled) {
        if (enabled && data == null) data = EmojiCatalog.get(context)
    }
    return data
}

/** Per-device emoji prefs, backed by the app's existing key/value store. */
@Composable
fun rememberEmojiPrefs(): EmojiPrefs {
    val app = LocalContext.current.applicationContext as? ExponentialApp
    return remember(app) {
        val store = if (app == null) {
            // No Hilt graph (previews/tests): prefs degrade to this session.
            object : EmojiPrefsStore {
                private val values = mutableMapOf<String, String>()
                override fun get(key: String): String? = values[key]
                override fun set(key: String, value: String?) {
                    if (value == null) values.remove(key) else values[key] = value
                }
            }
        } else {
            val secureStore = EntryPointAccessors
                .fromApplication(app, EmojiPrefsEntryPoint::class.java)
                .secureStore()
            object : EmojiPrefsStore {
                override fun get(key: String): String? = secureStore.get(key)
                override fun set(key: String, value: String?) = secureStore.set(key, value)
            }
        }
        EmojiPrefs(store)
    }
}

@dagger.hilt.EntryPoint
@dagger.hilt.InstallIn(dagger.hilt.components.SingletonComponent::class)
private interface EmojiPrefsEntryPoint {
    fun secureStore(): com.exponential.app.data.auth.SecureStore
}

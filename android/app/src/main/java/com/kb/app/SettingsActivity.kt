package com.kb.app

import android.content.Context
import android.os.Bundle
import androidx.activity.compose.setContent
import androidx.appcompat.app.AppCompatActivity
import androidx.appcompat.app.AppCompatDelegate
import androidx.compose.foundation.clickable
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import android.widget.Toast
import com.kb.ime.CustomPackStore
import com.kb.ime.FriendDefaults
import com.kb.ime.GestureLog
import com.kb.ime.GestureThresholds
import com.kb.ime.GestureTuningStore
import com.kb.ime.KbImeTheme
import com.kb.ime.LayoutStore
import com.kb.ime.SearchRow
import com.kb.ime.filterSearchRows
import com.kb.ime.SUPPORTED_LAYOUT_IDS
import com.kb.ime.SuggestionStore
import com.kb.ime.ThemeStore
import com.kb.sync.SyncPreferences
import com.kb.sync.SyncWorker
import kotlinx.coroutines.launch

/** Applies the stored ThemeStore mode to AppCompat DayNight. Call before `super.onCreate`. */
fun applyStoredNightMode(context: Context) {
    AppCompatDelegate.setDefaultNightMode(
        ThemeStore.toNightMode(ThemeStore.mode(context))
    )
}

/** Settings host: DayNight activity + 4-section nav (plan/19) + search. */
class SettingsActivity : AppCompatActivity() {
    companion object {
        const val EXTRA_SECTION = "section"
        fun intent(context: Context, section: String = Sections.HOME): android.content.Intent =
            android.content.Intent(context, SettingsActivity::class.java)
                .putExtra(EXTRA_SECTION, section)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        applyStoredNightMode(this)
        super.onCreate(savedInstanceState)
        val start = intent?.getStringExtra(EXTRA_SECTION) ?: Sections.HOME
        setContent {
            val context = LocalContext.current
            val storedMode = ThemeStore.mode(context)
            KbImeTheme(
                darkTheme = when (storedMode) {
                    ThemeStore.LIGHT -> false
                    ThemeStore.DARK -> true
                    else -> isSystemInDarkTheme()
                }
            ) {
                SettingsNav(initialRoute = start)
            }
        }
    }
}

/**
 * Four-section IA (plan/19 §New IA): Typing, Dictionaries, Appearance,
 * Advanced. One place per concern — "how typing feels", the single
 * dictionary editor (plan/20), look + mirrors, buried power tools.
 */
object Sections {
    const val HOME = "home"
    const val TYPING = "typing"
    const val DICTIONARIES = "dictionaries"
    const val APPEARANCE = "appearance"
    const val ADVANCED = "advanced"
}

/**
 * Legacy 8-page routes. Kept as constants (not destinations) so old
 * deep-links, docs, and the 8→4 acceptance test resolve instead of
 * 404ing — every one maps into its new section via [resolveSection].
 */
private object Routes {
    const val HOME = Sections.HOME
    const val APPEARANCE = Sections.APPEARANCE
    const val LAYOUT = "layout"
    const val SUGGESTIONS = "suggestions"
    const val CLIPBOARD = "clipboard"
    const val DICTIONARY = "dictionary"
    const val GESTURES = "gestures"
    const val TUNING = "tuning"
    const val PRIVACY = "privacy"
}

/**
 * 8→4 route resolution (plan/19 Tests): every legacy page lands on its
 * new section; unknown routes land on home. Pure and total — no dead
 * routes.
 */
internal fun resolveSection(route: String): String = when (route) {
    Sections.TYPING, Routes.LAYOUT, Routes.SUGGESTIONS, Routes.GESTURES -> Sections.TYPING
    Sections.DICTIONARIES, Routes.DICTIONARY -> Sections.DICTIONARIES
    Sections.APPEARANCE -> Sections.APPEARANCE
    Sections.ADVANCED, Routes.CLIPBOARD, Routes.TUNING, Routes.PRIVACY -> Sections.ADVANCED
    else -> Sections.HOME
}

private data class HomeEntry(val route: String, val title: String, val subtitle: String)

private val HOME_ENTRIES = listOf(
    HomeEntry(Sections.TYPING, "Typing", "Strip + layout + QWERTY + gestures"),
    HomeEntry(Sections.DICTIONARIES, "Dictionaries", "Tabs + packs (single editor)"),
    HomeEntry(Sections.APPEARANCE, "Appearance", "Theme + hints + mirrors"),
    HomeEntry(Sections.ADVANCED, "Advanced", "Tuning + clipboard + sync + reset"),
)

/**
 * Top-level search index (plan/19 step 2): every row of every section by
 * title. Tapping a result opens its section — even this dumb filter beats
 * the old maze.
 */
private val SEARCH_INDEX: List<Pair<SearchRow, String>> = listOf(
    SearchRow("typing", "Typing", "how typing feels", "strip autospace") to Sections.TYPING,
    SearchRow("strip", "Suggestion strip", "show strip, auto-space", "candidates bar") to Sections.TYPING,
    SearchRow("qwerty-default", "QWERTY default", "familiar board for friends", "layout choice 9-key opt-in") to Sections.TYPING,
    SearchRow("layout-global", "Layout global default", "9 / 12 / 16-key", "t9 pad per-tab medical") to Sections.TYPING,
    SearchRow("layout-pertab", "Per-tab layout overrides", "incl. medical", "ne words emoji") to Sections.TYPING,
    SearchRow("gestures", "Pad gestures", "flings, terminator, coach, log", "delete space accept hide 12-key") to Sections.TYPING,
    SearchRow("dictionaries", "Dictionaries", "single tab manager", "words nepali stack packs") to Sections.DICTIONARIES,
    SearchRow("show-code-tabs", "Show code tabs", "js rust html math medical", "friend default hide dev") to Sections.DICTIONARIES,
    SearchRow("custom-category", "Custom categories", "own wordlist tabs", "names import export priority") to Sections.DICTIONARIES,
    SearchRow("appearance", "Appearance", "theme + hints + mirrors", "dark light system") to Sections.APPEARANCE,
    SearchRow("theme", "Theme", "system light dark", "night mode daynight") to Sections.APPEARANCE,
    SearchRow("coach", "Coach + hints", "replay tutorial, footer hint", "onboarding discoverability") to Sections.APPEARANCE,
    SearchRow("advanced", "Advanced", "buried power tools", "tuning sliders log") to Sections.ADVANCED,
    SearchRow("tuning", "Detection tuning", "fling thresholds", "sliders dp velocity") to Sections.ADVANCED,
    SearchRow("clipboard", "Clipboard", "history retention + clear", "TTL pins 24h 7d privacy") to Sections.ADVANCED,
    SearchRow("sync", "Privacy / Sync", "opt-in sync + wizard", "export backup account") to Sections.ADVANCED,
    SearchRow("reset", "Reset to defaults", "layout + friend prefs", "restore undo") to Sections.ADVANCED,
)

@Composable
fun SettingsNav(initialRoute: String = Sections.HOME) {
    var route by rememberSaveable { mutableStateOf(resolveSection(initialRoute)) }
    when (route) {
        Sections.HOME -> SettingsHome(onOpen = { route = it })
        Sections.TYPING -> SettingsPage(title = "Typing", onBack = { route = Sections.HOME }) {
            TypingTopSection()
            SuggestionsPage()
            LayoutSettingsSection()
            GestureSwitchesSection()
        }
        Sections.DICTIONARIES -> SettingsPage(title = "Dictionaries", onBack = { route = Sections.HOME }) {
            FriendTabsHeader()
            DictStackSection()
            CustomCategorySection()
        }
        Sections.APPEARANCE -> SettingsPage(title = "Appearance", onBack = { route = Sections.HOME }) {
            AppearancePage()
            AppearanceMirrors()
        }
        Sections.ADVANCED -> SettingsPage(title = "Advanced", onBack = { route = Sections.HOME }) {
            GestureTuningSlidersSection()
            ClipboardSettingsSection()
            PrivacyPage()
            ResetSection()
        }
        else -> SettingsHome(onOpen = { route = it })
    }
}

/** Home list: search over every row + one compact card per section. */
@Composable
private fun SettingsHome(onOpen: (String) -> Unit) {
    var query by rememberSaveable { mutableStateOf("") }
    Column(modifier = Modifier.fillMaxSize()) {
        Text(
            "Keyboard Settings",
            style = MaterialTheme.typography.headlineSmall,
            modifier = Modifier.padding(16.dp)
        )
        androidx.compose.material3.OutlinedTextField(
            value = query,
            onValueChange = { query = it },
            label = { Text("Search settings") },
            singleLine = true,
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp)
        )
        val results = remember(query) {
            filterSearchRows(SEARCH_INDEX.map { it.first }, query)
        }
        LazyColumn(modifier = Modifier.fillMaxSize()) {
            if (query.isBlank()) {
                items(HOME_ENTRIES) { entry ->
                    HomeRow(title = entry.title, subtitle = entry.subtitle) {
                        onOpen(entry.route)
                    }
                }
            } else {
                items(results) { row ->
                    val section = SEARCH_INDEX.firstOrNull { it.first.id == row.id }
                        ?.second ?: Sections.HOME
                    HomeRow(title = row.title, subtitle = row.subtitle) {
                        onOpen(section)
                    }
                }
                if (results.isEmpty()) {
                    item {
                        Text(
                            "No settings match — try \"layout\" or \"clipboard\".",
                            style = MaterialTheme.typography.labelSmall,
                            modifier = Modifier.padding(16.dp)
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun HomeRow(title: String, subtitle: String, onOpen: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable { onOpen() }
            .padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge)
            Text(
                subtitle,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }
        Text("›", style = MaterialTheme.typography.headlineSmall)
    }
}

/**
 * Typing top: friend-facing board choice (plan/21 step 2). QWERTY default
 * ON for friends; 9-key stays one toggle away (research opt-in). The IME
 * reads it on its next start (session state — in-session toggles are
 * never clobbered).
 */
@Composable
private fun TypingTopSection() {
    val context = LocalContext.current
    var error by remember { mutableStateOf<String?>(null) }
    var qwerty by remember { mutableStateOf(FriendDefaults.qwertyDefault(context)) }

    SettingsSectionTitle("Board")
    error?.let {
        Text(it, color = MaterialTheme.colorScheme.error)
    }
    Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Column(modifier = Modifier.weight(1f)) {
            Text("QWERTY default (friends)")
            Text(
                "OFF = 9-key pad first. 9-key is the research vehicle, opt-in.",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }
        Switch(
            checked = qwerty,
            onCheckedChange = {
                try {
                    FriendDefaults.setQwertyDefault(context, it)
                    qwerty = it
                    error = null
                } catch (e: Exception) {
                    error = "Save failed: ${e.message}"
                }
            }
        )
    }
    Text(
        "Applies on next keyboard start. The ⌨ key switches boards anytime.",
        style = MaterialTheme.typography.labelSmall
    )
}

/**
 * Dictionaries header (plan/20 step 1): friends see two always-on base
 * toggles first (English, नेपाली — fixed pins, info-only) + "Show code
 * tabs". The power-user drag UI ([DictStackSection]) stays below, not
 * above. The single editor for everything on this page.
 */
@Composable
private fun FriendTabsHeader() {
    val context = LocalContext.current
    var error by remember { mutableStateOf<String?>(null) }
    var showCode by remember { mutableStateOf(FriendDefaults.showCodeTabs(context)) }

    SettingsSectionTitle("Tabs")
    error?.let {
        Text(it, color = MaterialTheme.colorScheme.error)
    }
    Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text("English (base — always on)", modifier = Modifier.weight(1f))
        Switch(checked = true, onCheckedChange = {}, enabled = false)
    }
    Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text("नेपाली (base — always on)", modifier = Modifier.weight(1f))
        Switch(checked = true, onCheckedChange = {}, enabled = false)
    }
    Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Column(modifier = Modifier.weight(1f)) {
            Text("Show code tabs")
            Text(
                "js · rust · html · math · medical. OFF = EN/ने/😀/123 only.",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }
        Switch(
            checked = showCode,
            onCheckedChange = {
                try {
                    FriendDefaults.setShowCodeTabs(context, it)
                    showCode = it
                    error = null
                } catch (e: Exception) {
                    error = "Save failed: ${e.message}"
                }
            }
        )
    }
    Text(
        "Tab order, enable flags, and custom tabs below apply on next input view (order needs a restart).",
        style = MaterialTheme.typography.labelSmall
    )
}

/**
 * Appearance mirrors (plan/19): theme lives here ([AppearancePage]);
 * clipboard/incognito mirror as status + entry points (full editors live
 * in Advanced). Incognito is session-only by design — the mirror says
 * what to look for (dark strip + mask badge), not a toggle.
 */
@Composable
private fun AppearanceMirrors() {
    val context = LocalContext.current
    var error by remember { mutableStateOf<String?>(null) }
    var clipOn by remember { mutableStateOf(FriendDefaults.clipboardEnabled(context)) }

    SettingsSectionTitle("Clipboard + incognito")
    error?.let {
        Text(it, color = MaterialTheme.colorScheme.error)
    }
    Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Column(modifier = Modifier.weight(1f)) {
            Text("Clipboard history")
            Text(
                "Friend-default OFF. Retention + clear live in Advanced.",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }
        Switch(
            checked = clipOn,
            onCheckedChange = {
                try {
                    FriendDefaults.setClipboardEnabled(context, it)
                    clipOn = it
                    error = null
                } catch (e: Exception) {
                    error = "Save failed: ${e.message}"
                }
            }
        )
    }
    Text(
        "Incognito = dark strip + 🎭 badge (auto in password fields). Nothing is saved while it shows.",
        style = MaterialTheme.typography.labelSmall
    )
}

/**
 * Advanced: reset-to-defaults (plan/19). Clears per-tab layout overrides
 * + global layout + friend prefs (board, code tabs) back to ship
 * defaults. Dictionary order, customs, and clipboard contents are
 * untouched (those have their own deletes).
 */
@Composable
private fun ResetSection() {
    val context = LocalContext.current
    var error by remember { mutableStateOf<String?>(null) }
    var notice by remember { mutableStateOf<String?>(null) }

    SettingsSectionTitle("Reset")
    error?.let {
        Text(it, color = MaterialTheme.colorScheme.error)
    }
    notice?.let {
        Text(it, style = MaterialTheme.typography.labelSmall)
    }
    Button(onClick = {
        try {
            LayoutStore.setGlobal(context, com.kb.ime.DEFAULT_LAYOUT_ID)
            for ((cat, _) in LayoutStore.KNOWN_CATS) {
                LayoutStore.clearCatLayout(context, cat)
            }
            FriendDefaults.setShowCodeTabs(context, false)
            FriendDefaults.setQwertyDefault(context, true)
            notice = "Defaults restored (layout ${com.kb.ime.DEFAULT_LAYOUT_ID}, QWERTY first, code tabs hidden). Applies on next input view."
            error = null
        } catch (e: Exception) {
            error = "Reset failed: ${e.message}"
        }
    }) { Text("Reset to defaults") }
}

/**
 * One settings page: back bar + scrollable compact content. Every page
 * scrolls on its own (the old monolith + nested LazyColumn measured
 * unbounded and rendered broken).
 */
@Composable
private fun SettingsPage(
    title: String,
    onBack: () -> Unit,
    content: @Composable () -> Unit
) {
    Column(modifier = Modifier.fillMaxSize()) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically
        ) {
            TextButton(onClick = onBack) { Text("‹ Back") }
            Text(title, style = MaterialTheme.typography.titleMedium)
        }
        Column(
            modifier = Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp, vertical = 8.dp)
        ) {
            content()
        }
    }
}

/** Appearance page: AppCompat DayNight theme selector (system/light/dark). */
@Composable
private fun AppearancePage() {
    val context = LocalContext.current
    var mode by remember { mutableStateOf(ThemeStore.mode(context)) }
    var error by remember { mutableStateOf<String?>(null) }

    SettingsSectionTitle("Theme")
    Text(
        "Applies to settings and the keyboard (DayNight).",
        style = MaterialTheme.typography.labelSmall
    )
    error?.let {
        Text(it, color = MaterialTheme.colorScheme.error)
    }
    ThemeStore.MODES.forEach { id ->
        val label = when (id) {
            ThemeStore.LIGHT -> "Light"
            ThemeStore.DARK -> "Dark"
            else -> "System"
        }
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clickable {
                    try {
                        ThemeStore.setMode(context, id)
                        mode = id
                        error = null
                        AppCompatDelegate.setDefaultNightMode(ThemeStore.toNightMode(id))
                    } catch (e: Exception) {
                        error = "Save failed: ${e.message}"
                    }
                }
                .padding(vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Text(
                if (id == mode) "● $label" else "○ $label",
                style = MaterialTheme.typography.bodyLarge,
                modifier = Modifier.weight(1f)
            )
        }
    }
}

/** Suggestions page: strip behavior flags ([SuggestionStore], immediate save). */
@Composable
private fun SuggestionsPage() {
    val context = LocalContext.current
    var error by remember { mutableStateOf<String?>(null) }
    var show by remember { mutableStateOf(SuggestionStore.showSuggestions(context)) }
    var autoSpace by remember { mutableStateOf(SuggestionStore.autoSpace(context)) }

    SettingsSectionTitle("Suggestions")
    error?.let {
        Text(it, color = MaterialTheme.colorScheme.error)
    }
    Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text("Show suggestion strip", modifier = Modifier.weight(1f))
        Switch(
            checked = show,
            onCheckedChange = {
                try {
                    SuggestionStore.setShowSuggestions(context, it)
                    show = it
                    error = null
                } catch (e: Exception) {
                    error = "Save failed: ${e.message}"
                }
            }
        )
    }
    Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text("Auto-space after accept", modifier = Modifier.weight(1f))
        Switch(
            checked = autoSpace,
            onCheckedChange = {
                try {
                    SuggestionStore.setAutoSpace(context, it)
                    autoSpace = it
                    error = null
                } catch (e: Exception) {
                    error = "Save failed: ${e.message}"
                }
            }
        )
    }
    Text(
        "The IME reads these on its next refresh — no restart needed.",
        style = MaterialTheme.typography.labelSmall
    )
}

/** Privacy page: sync opt-in + enable-IME wizard entry. */
@Composable
private fun PrivacyPage() {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val optedIn by SyncPreferences.optedIn(context).collectAsState(initial = false)

    SettingsSectionTitle("Privacy / Sync")
    Text(
        "Sync is opt-in and OFF by default. Clipboard history never syncs.",
        style = MaterialTheme.typography.labelSmall
    )
    Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(if (optedIn) "Sync: ON (opt-in)" else "Sync: OFF", modifier = Modifier.weight(1f))
        Switch(
            checked = optedIn,
            onCheckedChange = { value ->
                scope.launch {
                    SyncPreferences.setOptedIn(context, value)
                    if (value) SyncWorker.schedule(context) else SyncWorker.cancel(context)
                }
            }
        )
    }
    Button(onClick = { context.startActivity(OnboardingActivity.intent(context)) }) {
        Text("Open enable-IME wizard")
    }
}

/**
 * Consistent section header (M3 Expressive pass): titleMedium in primary
 * with uniform top spacing. Every Settings section uses this — the screen
 * title keeps headlineSmall, so the hierarchy is exactly two levels in
 * light and dark themes (no hardcoded colors).
 */
@Composable
fun SettingsSectionTitle(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.titleMedium,
        color = MaterialTheme.colorScheme.primary,
        modifier = Modifier.padding(top = 24.dp)
    )
}

/**
 * Settings → Layout (plan 02): global default (t9-9 / t9-12 / t9-16) plus
 * the per-tab override list (e.g. General→t9-9, NE→t9-9). Persisted by
 * [LayoutStore] (same prefs file the IME service resolves per
 * `active_tab`); changing a layout takes effect on the next tab switch
 * or field start — labels re-encode from the newly loaded spec.
 * Validation failures surface as an explicit Toast, never a silent keep.
 */
@Composable
fun LayoutSettingsSection() {
    val context = LocalContext.current
    var error by remember { mutableStateOf<String?>(null) }
    var savedTick by remember { mutableStateOf(0) }

    fun readGlobal(): String = try {
        LayoutStore.global(context)
    } catch (e: Exception) {
        error = "Layout prefs unreadable, showing default: ${e.message}"
        com.kb.ime.DEFAULT_LAYOUT_ID
    }

    fun readOverrides(): Map<String, String> = try {
        LayoutStore.overrides(context)
    } catch (e: Exception) {
        error = "Layout prefs unreadable: ${e.message}"
        emptyMap()
    }

    var global by remember(savedTick) { mutableStateOf(readGlobal()) }
    var overrides by remember(savedTick) { mutableStateOf(readOverrides()) }

    SettingsSectionTitle("Layout")
    error?.let {
        Text(it, color = MaterialTheme.colorScheme.error)
    }

    Text("Global default", style = MaterialTheme.typography.titleMedium)
    Row(modifier = Modifier.fillMaxWidth()) {
        SUPPORTED_LAYOUT_IDS.forEach { id ->
            val short = id.removePrefix("t9-")
            TextButton(onClick = {
                try {
                    LayoutStore.setGlobal(context, id)
                    global = id
                    error = null
                } catch (e: Exception) {
                    error = "Save failed: ${e.message}"
                    Toast.makeText(context, "Layout not saved: ${e.message}", Toast.LENGTH_LONG).show()
                }
            }) { Text(if (id == global) "[$short]" else short) }
        }
    }
    Text(
        "Applies to every tab without an override below. " +
            "NE defaults to t9-9 (shared Latin code set; t9-16 stays opt-in).",
        style = MaterialTheme.typography.labelSmall
    )

    Text("Per-tab overrides", style = MaterialTheme.typography.titleMedium,
        modifier = Modifier.padding(top = 8.dp))
    LayoutStore.KNOWN_CATS.forEach { (cat, label) ->
        val current = overrides[cat]
        Row(modifier = Modifier.fillMaxWidth()) {
            Text(
                "$label${if (cat != label) " ($cat)" else ""}",
                modifier = Modifier
                    .weight(1f)
                    .padding(vertical = 12.dp)
            )
            TextButton(onClick = {
                try {
                    LayoutStore.clearCatLayout(context, cat)
                    overrides = overrides - cat
                    error = null
                } catch (e: Exception) {
                    error = "Clear failed: ${e.message}"
                }
            }) { Text(if (current == null) "[auto]" else "auto") }
            SUPPORTED_LAYOUT_IDS.forEach { id ->
                val short = id.removePrefix("t9-")
                TextButton(onClick = {
                    try {
                        LayoutStore.setCatLayout(context, cat, id)
                        overrides = overrides + (cat to id)
                        error = null
                    } catch (e: Exception) {
                        error = "Save failed: ${e.message}"
                        Toast.makeText(context, "Layout not saved: ${e.message}", Toast.LENGTH_LONG).show()
                    }
                }) { Text(if (id == current) "[$short]" else short) }
            }
        }
    }
    Button(onClick = {
        // Re-read from disk so external (IME-process) writes become visible.
        savedTick++
        global = readGlobal()
        overrides = readOverrides()
    }) { Text("Refresh layout settings") }
}

/**
 * Settings → Custom categories (user packs, plan/08): create a new tab
 * from a pasted/imported wordlist, toggle it, export/import the set.
 * There is no built-in names starter list (import-only by design — see
 * `docs/CUSTOM_CATEGORIES.md`). Every save/import validates loudly:
 * the exact per-row errors render inline and nothing is stored unless
 * the whole pack validates.
 */
@Composable
fun CustomCategorySection() {
    val context = LocalContext.current
    var error by remember { mutableStateOf<String?>(null) }
    var savedTick by remember { mutableStateOf(0) }
    var id by remember { mutableStateOf("") }
    var title by remember { mutableStateOf("") }
    var priorityText by remember { mutableStateOf("50") }
    var wordlist by remember { mutableStateOf("") }
    var importText by remember { mutableStateOf("") }
    var exportText by remember { mutableStateOf<String?>(null) }

    fun readPacks() = try {
        CustomPackStore.list(context)
    } catch (e: Exception) {
        error = "Custom packs unreadable: ${e.message}"
        emptyList()
    }

    var packs by remember(savedTick) { mutableStateOf(readPacks()) }

    SettingsSectionTitle("Custom categories")
    Text(
        "New tab from your own wordlist (e.g. a names list). " +
            "One `word [freq]` per line, `#` comments allowed. " +
            "New tabs isolate to their own words; learned words overlay " +
            "under the tab. Takes effect in the engine after the keyboard restarts.",
        style = MaterialTheme.typography.labelSmall
    )
    error?.let {
        Text(it, color = MaterialTheme.colorScheme.error)
    }

    packs.forEach { pack ->
        Row(modifier = Modifier.fillMaxWidth()) {
            Text(
                "${pack.title} (${pack.id}, prio ${pack.priority})",
                modifier = Modifier
                    .weight(1f)
                    .padding(vertical = 12.dp)
            )
            Switch(
                checked = pack.enabled,
                onCheckedChange = { enabled ->
                    try {
                        CustomPackStore.setEnabled(context, pack.id, enabled)
                        error = null
                        savedTick++
                        packs = readPacks()
                    } catch (e: Exception) {
                        error = "Toggle failed: ${e.message}"
                    }
                }
            )
            TextButton(onClick = {
                try {
                    CustomPackStore.remove(context, pack.id)
                    error = null
                    savedTick++
                    packs = readPacks()
                } catch (e: Exception) {
                    error = "Delete failed: ${e.message}"
                }
            }) { Text("Delete") }
        }
    }

    Text("New category", style = MaterialTheme.typography.titleMedium)
    androidx.compose.material3.OutlinedTextField(
        value = id, onValueChange = { id = it },
        label = { Text("id (e.g. names)") },
        modifier = Modifier.fillMaxWidth()
    )
    androidx.compose.material3.OutlinedTextField(
        value = title, onValueChange = { title = it },
        label = { Text("title (e.g. Names)") },
        modifier = Modifier.fillMaxWidth()
    )
    androidx.compose.material3.OutlinedTextField(
        value = priorityText, onValueChange = { priorityText = it },
        label = { Text("priority 10-90") },
        modifier = Modifier.fillMaxWidth()
    )
    androidx.compose.material3.OutlinedTextField(
        value = wordlist, onValueChange = { wordlist = it },
        label = { Text("wordlist (word [freq] per line)") },
        minLines = 4,
        modifier = Modifier.fillMaxWidth()
    )
    Button(onClick = {
        try {
            val priority = priorityText.trim().toIntOrNull()
                ?: throw IllegalArgumentException("priority \"$priorityText\" is not a number (10-90)")
            CustomPackStore.save(context, id.trim(), title.trim(), wordlist, priority)
            error = null
            id = ""
            title = ""
            wordlist = ""
            savedTick++
            packs = readPacks()
        } catch (e: Exception) {
            error = "Save failed: ${e.message}"
        }
    }) { Text("Save category") }

    Text("Export / import", style = MaterialTheme.typography.titleMedium,
        modifier = Modifier.padding(top = 8.dp))
    Row(modifier = Modifier.fillMaxWidth()) {
        Button(onClick = {
            try {
                exportText = CustomPackStore.exportJson(context)
                error = null
            } catch (e: Exception) {
                error = "Export failed: ${e.message}"
            }
        }) { Text("Export") }
        TextButton(onClick = {
            try {
                val imported = CustomPackStore.importJson(context, importText)
                error = null
                importText = ""
                savedTick++
                packs = readPacks()
                Toast.makeText(context, "Imported ${imported.size} pack(s)", Toast.LENGTH_LONG).show()
            } catch (e: Exception) {
                error = "Import failed: ${e.message}"
            }
        }) { Text("Import pasted JSON") }
    }
    exportText?.let { Text(it, style = MaterialTheme.typography.labelSmall) }
    androidx.compose.material3.OutlinedTextField(
        value = importText, onValueChange = { importText = it },
        label = { Text("paste export JSON to import") },
        minLines = 2,
        modifier = Modifier.fillMaxWidth()
    )
    Button(onClick = {
        savedTick++
        packs = readPacks()
        error = null
    }) { Text("Refresh custom categories") }
}

/**
 * Settings → Gestures (behavior switches): 12-key fling opt-in, code
 * terminator, coach replay, false-trigger log. Every switch writes
 * immediately ([GestureTuningStore] setters throw loudly on failure);
 * numeric thresholds live in [GestureTuningSlidersSection] (Tuning page).
 */
@Composable
fun GestureSwitchesSection() {
    val context = LocalContext.current
    var error by remember { mutableStateOf<String?>(null) }
    var savedTick by remember { mutableStateOf(0) }
    var allow12 by remember(savedTick) {
        mutableStateOf(GestureTuningStore.allowTwelveKeyFlings(context))
    }
    var terminator by remember(savedTick) {
        mutableStateOf(GestureTuningStore.codeTerminator(context))
    }
    var logStats by remember(savedTick) {
        mutableStateOf(
            GestureLog(context, onError = { error = it }).stats()
        )
    }

    SettingsSectionTitle("Gestures")
    error?.let {
        Text(it, color = MaterialTheme.colorScheme.error)
    }

    Row(modifier = Modifier.fillMaxWidth()) {
        Text("Allow pad flings in 12-key (thresholds +30%)",
            modifier = Modifier.weight(1f))
        Switch(
            checked = allow12,
            onCheckedChange = {
                try {
                    GestureTuningStore.setAllowTwelveKeyFlings(context, it)
                    allow12 = it
                    error = null
                } catch (e: Exception) {
                    error = "Save failed: ${e.message}"
                }
            }
        )
    }
    Text(
        text = "Default OFF: the 12-key bottom row [Sym|Space|⌫] is authoritative. " +
            "←/↑ flings stay disabled; → accept keeps working.",
        style = MaterialTheme.typography.labelSmall
    )
    Row(modifier = Modifier.fillMaxWidth()) {
        Text("Code/math ↑ terminator", modifier = Modifier.weight(1f))
        TextButton(onClick = {
            try {
                GestureTuningStore.setCodeTerminator(context, " ")
                terminator = " "
                error = null
            } catch (e: Exception) {
                error = "Save failed: ${e.message}"
            }
        }) {
            Text(if (terminator == " ") "[space]" else "space")
        }
        TextButton(onClick = {
            try {
                GestureTuningStore.setCodeTerminator(context, ";")
                terminator = ";"
                error = null
            } catch (e: Exception) {
                error = "Save failed: ${e.message}"
            }
        }) {
            Text(if (terminator == ";" ) "[;]" else ";")
        }
    }

    Text("Discoverability", style = MaterialTheme.typography.titleMedium,
        modifier = Modifier.padding(top = 16.dp))
    Button(onClick = {
        try {
            GestureTuningStore.requestCoachReplay(context)
            error = "T9 coach will replay on next input view"
        } catch (e: Exception) {
            error = "Coach replay failed: ${e.message}"
        }
    }) { Text("Replay T9 coach") }

    Text("False-trigger log (log.jsonl)", style = MaterialTheme.typography.titleMedium,
        modifier = Modifier.padding(top = 16.dp))
    if (logStats.unreadable) {
        Text("Log unreadable: ${logStats.error}",
            color = MaterialTheme.colorScheme.error)
    } else {
        Text("Events: ${logStats.events} · fling-← commits: ${logStats.flingLeftCommits} " +
            "· undone: ${logStats.undoneDeletes}")
        if (logStats.flingLeftCommits > 0) {
            val rate = 100 * logStats.undoneDeletes / logStats.flingLeftCommits
            Text("Undo rate ≈ $rate% (false-trigger estimate)",
                style = MaterialTheme.typography.labelSmall)
        }
    }
    Button(onClick = {
        val ok = GestureLog(context, onError = { error = it }).clear()
        if (ok) logStats = GestureLog(context).stats()
    }) { Text("Clear gesture log") }
    TextButton(onClick = { savedTick++ }) { Text("Refresh gesture settings") }
}

/**
 * Settings → Gesture Tuning (plan 01 §Detection + §12-key override).
 * Ship defaults: fling 24dp / 200dp/s / 1.4:1 axis, word-step 20dp,
 * long-press 400ms (300–600), slop 8dp, edge 8dp. Out-of-range saves throw
 * ([GestureThresholds.init]) and surface inline instead of clamping.
 */
/**
 * Settings → Tuning: numeric detection thresholds only (behavior switches
 * live in [GestureSwitchesSection], Gestures page). Saves thresholds alone
 * so a Tuning save never clobbers switches edited on the Gestures page.
 */
@Composable
fun GestureTuningSlidersSection() {
    val context = LocalContext.current
    var error by remember { mutableStateOf<String?>(null) }
    var savedTick by remember { mutableStateOf(0) }

    fun current(): GestureThresholds = try {
        GestureTuningStore.loadThresholds(context)
    } catch (e: Exception) {
        error = "Tuning prefs unreadable, showing defaults: ${e.message}"
        GestureThresholds()
    }

    var flingDp by remember(savedTick) { mutableFloatStateOf(current().flingThresholdDp) }
    var velocity by remember(savedTick) { mutableFloatStateOf(current().flingVelocityDpS) }
    var axis by remember(savedTick) { mutableFloatStateOf(current().axisRatio) }
    var wordStep by remember(savedTick) { mutableFloatStateOf(current().wordStepDp) }
    var longPress by remember(savedTick) { mutableFloatStateOf(current().longPressMs.toFloat()) }

    SettingsSectionTitle("Tuning")
    error?.let {
        Text(it, color = MaterialTheme.colorScheme.error)
    }

    TuningSlider("Fling distance (dp)", flingDp, 12f, 48f) { flingDp = it }
    TuningSlider("Fling velocity (dp/s)", velocity, 100f, 500f) { velocity = it }
    TuningSlider("Dominant-axis ratio", axis, 1.1f, 3f) { axis = it }
    TuningSlider("Word step (dp/word)", wordStep, 10f, 40f) { wordStep = it }
    TuningSlider("Long-press (ms)", longPress, 300f, 600f) { longPress = it }

    Button(onClick = {
        try {
            val base = current()
            GestureTuningStore.saveThresholds(
                context,
                base.copy(
                    flingThresholdDp = flingDp,
                    flingVelocityDpS = velocity,
                    axisRatio = axis,
                    wordStepDp = wordStep,
                    longPressMs = longPress.toLong()
                )
            )
            error = null
            savedTick++
        } catch (e: Exception) {
            error = "Save failed: ${e.message}"
        }
    }) { Text("Save gesture tuning") }
    TextButton(onClick = { savedTick++ }) { Text("Refresh tuning") }
}

@Composable
private fun TuningSlider(
    label: String,
    value: Float,
    min: Float,
    max: Float,
    onChange: (Float) -> Unit
) {
    Text("$label: ${"%.1f".format(value)}")
    Slider(
        value = value,
        onValueChange = onChange,
        valueRange = min..max,
        modifier = Modifier.fillMaxWidth()
    )
}

// VAPP-3 spike: PeerHello. Throwaway. Viewer on the steer relay + the Rust peer core via UniFFI.
//   adb shell am start -n at.exponential.peerhello/.MainActivity --es auto bench --es policy all --es scenario emulator-nat
package at.exponential.peerhello

import android.app.Activity
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.graphics.Typeface
import android.net.ConnectivityManager
import android.net.Network
import android.os.Build
import android.os.Bundle
import android.os.SystemClock
import android.text.InputType
import android.util.Log
import android.view.View
import android.view.ViewGroup
import android.widget.ArrayAdapter
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.Spinner
import android.widget.TextView
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withTimeoutOrNull
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import org.json.JSONArray
import org.json.JSONObject
import java.time.Instant
import java.util.UUID
import java.util.concurrent.TimeUnit
import uniffi.peer_ffi.IcePolicy
import uniffi.peer_ffi.LinkState
import uniffi.peer_ffi.PeerConfig
import uniffi.peer_ffi.PeerLink
import uniffi.peer_ffi.peerVersion
import uniffi.peer_ffi.resultEnvelope

private const val TAG = "VAPP3"

class MainActivity : Activity() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private val http = OkHttpClient.Builder().pingInterval(20, TimeUnit.SECONDS).build()

    private lateinit var relayField: EditText
    private lateinit var ticketField: EditText
    private lateinit var sessionField: EditText
    private lateinit var turnField: EditText
    private lateinit var policySpinner: Spinner
    private lateinit var scenarioSpinner: Spinner
    private lateinit var statusView: TextView
    private lateinit var logView: TextView

    private val isEmulator = Build.FINGERPRINT.contains("generic") || Build.FINGERPRINT.contains("emulator") ||
        Build.MODEL.contains("sdk_gphone") || Build.HARDWARE.contains("ranchu")
    private val source get() = if (isEmulator) "android-emu" else "android-device"

    // Connection state (main thread only).
    private var ws: WebSocket? = null
    private var link: PeerLink? = null
    private var pump: Job? = null
    private var peerId = newPeerId()
    private var daemonId: String? = null
    private var daemonPub: String? = null
    private var synced = CompletableDeferred<Unit>()
    private var helloWaiter: CompletableDeferred<Pair<String, String>>? = null
    private val seen = HashSet<String>()
    private var connectStartedAt = 0L
    private var connectMs: Double? = null
    private var lastReconnect: JSONObject? = null
    private var lastLine: String? = null
    private var stoppedAt = 0L
    private var reconnecting = false
    @Volatile private var answersAccepted = 0
    private var initialNetwork: Network? = null
    private var netCallback: ConnectivityManager.NetworkCallback? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        buildUi()
        log("peer-ffi ${runCatching { peerVersion() }.getOrElse { "load failed: $it" }}, source=$source, peerId=$peerId")
        watchNetwork()
        handleAuto(intent)
    }

    override fun onNewIntent(intent: android.content.Intent) {
        super.onNewIntent(intent)
        handleAuto(intent)
    }

    override fun onStop() {
        super.onStop()
        stoppedAt = SystemClock.elapsedRealtime()
    }

    override fun onResume() {
        super.onResume()
        if (stoppedAt > 0 && link != null) {
            val awayMs = SystemClock.elapsedRealtime() - stoppedAt
            log("resumed after ${awayMs} ms in background")
            reconnect("background")
        }
        stoppedAt = 0
    }

    override fun onDestroy() {
        netCallback?.let { runCatching { getSystemService(ConnectivityManager::class.java).unregisterNetworkCallback(it) } }
        link?.closeLink()
        ws?.close(1000, "bye")
        scope.cancel()
        super.onDestroy()
    }

    // ── UI ────────────────────────────────────────────────────────────────────
    private fun buildUi() {
        val pad = (12 * resources.displayMetrics.density).toInt()
        val col = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; setPadding(pad, pad * 3, pad, pad) }
        fun field(label: String, value: String, multi: Boolean = false): EditText {
            col.addView(TextView(this).apply { text = label; textSize = 12f })
            return EditText(this).apply {
                setText(value); textSize = 13f
                inputType = if (multi) InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_MULTI_LINE else InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI
                if (multi) maxLines = 3 else isSingleLine = true
                col.addView(this)
            }
        }
        fun spinner(label: String, items: List<String>, initial: String): Spinner {
            col.addView(TextView(this).apply { text = label; textSize = 12f })
            return Spinner(this).apply {
                adapter = ArrayAdapter(this@MainActivity, android.R.layout.simple_spinner_dropdown_item, items)
                setSelection(items.indexOf(initial).coerceAtLeast(0))
                col.addView(this)
            }
        }
        val relayDefault = if (isEmulator) "ws://10.0.2.2:4002" else "ws://${DevTickets.MAC_HOST}:4002"
        relayField = field("Relay URL", relayDefault)
        ticketField = field("Viewer ticket", DevTickets.VIEWER, multi = true)
        sessionField = field("Session id", DevTickets.SESSION_ID)
        turnField = field("TURN host:port (user exp / pass spike / realm exponential.local)", "${DevTickets.MAC_HOST}:3478")
        policySpinner = spinner("Policy", listOf("all", "relay", "host"), "all")
        scenarioSpinner = spinner("Scenario", SCENARIOS, if (isEmulator) "emulator-nat" else "same-lan")
        col.addView(TextView(this).apply { text = "Source: $source"; textSize = 12f })

        val row1 = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        val row2 = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        fun button(row: LinearLayout, label: String, onClick: () -> Unit) = row.addView(Button(this).apply {
            text = label; isAllCaps = false; setOnClickListener { onClick() }
        }, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        button(row1, "Connect") { scope.launch { runCatching { connect() }.onFailure { log("connect failed: $it") } } }
        button(row1, "Bench") { scope.launch { runCatching { bench() }.onFailure { log("bench failed: $it") } } }
        button(row1, "Reconnect") { reconnect("manual") }
        button(row2, "Send results") { scope.launch { sendResults() } }
        button(row2, "Copy JSON") { copyJson() }
        col.addView(row1); col.addView(row2)
        statusView = TextView(this).apply { text = "idle"; typeface = Typeface.DEFAULT_BOLD; textSize = 13f }
        col.addView(statusView)
        logView = TextView(this).apply { typeface = Typeface.MONOSPACE; textSize = 10f; setTextIsSelectable(true) }
        col.addView(logView)
        setContentView(ScrollView(this).apply { addView(col) })
    }

    private fun log(msg: String) {
        Log.i(TAG, msg)
        val apply = { logView.append("${Instant.now().toString().substring(11, 23)} $msg\n") }
        if (android.os.Looper.myLooper() == android.os.Looper.getMainLooper()) apply() else runOnUiThread(apply)
    }

    private fun status(s: String) = runOnUiThread { statusView.text = s }

    private fun policy(): IcePolicy = when (policySpinner.selectedItem as String) {
        "relay" -> IcePolicy.RELAY
        "host" -> IcePolicy.HOST
        else -> IcePolicy.ALL
    }

    // ── Relay (viewer) ──────────────────────────────────────────────────────────
    private fun openRelay(): CompletableDeferred<Unit> {
        ws?.close(1000, "reopen") // close (not cancel): queued frames, e.g. a result envelope, still go out
        synced = CompletableDeferred()
        val url = relayField.text.toString().trimEnd('/') + "/ws?ticket=" + ticketField.text.toString().trim()
        val opened = CompletableDeferred<Unit>()
        val req = Request.Builder().url(url).build()
        ws = http.newWebSocket(req, object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                webSocket.send("""{"t":"join","channel":"activity"}""")
                opened.complete(Unit)
                log("relay open, joined")
            }
            override fun onMessage(webSocket: WebSocket, text: String) {
                runOnUiThread { onFrame(text) }
            }
            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                log("relay failure: $t ${response?.code ?: ""}")
                opened.completeExceptionally(t)
            }
            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                log("relay closed $code $reason")
            }
        })
        return opened
    }

    private fun onFrame(text: String) {
        val frame = runCatching { JSONObject(text) }.getOrNull() ?: return
        when (frame.optString("t")) {
            "activity_synced" -> synced.complete(Unit)
            "error", "bye" -> log("relay: $text".take(300))
            "activity" -> {
                val ev = frame.optJSONObject("event") ?: return
                if (ev.optString("kind") != "narration") return
                val envText = ev.optString("text")
                val env = runCatching { JSONObject(envText) }.getOrNull() ?: return
                if (env.optInt("v") != 1) return
                val to = env.optString("to", "")
                if (to.isNotEmpty() && to != peerId) return
                if (!seen.add(envText)) return
                onEnvelope(env, envText)
            }
        }
    }

    private fun onEnvelope(env: JSONObject, raw: String) {
        val type = env.optString("type")
        val from = env.optString("from")
        when (type) {
            "hello" -> {
                daemonId = from; daemonPub = env.optString("pub")
                log("daemon hello from=$from version=${env.optString("version")}")
                helloWaiter?.complete(from to env.optString("pub"))
            }
            "answer" -> {
                if (from != daemonId) return
                val l = link ?: return
                scope.launch(Dispatchers.IO) {
                    runCatching { l.acceptAnswer(raw) }
                        .onSuccess { answersAccepted++; log("answer accepted") }
                        .onFailure { log("acceptAnswer failed: $it") }
                }
            }
            "candidate", "end_of_candidates" -> {
                if (from != daemonId) return
                val l = link ?: return
                if (type == "end_of_candidates") return
                scope.launch(Dispatchers.IO) { runCatching { l.addRemoteCandidate(raw) }.onFailure { log("addRemoteCandidate: $it") } }
            }
            "reject" -> log("REJECTED by daemon: ${env.optString("reason")}")
            "bye" -> log("daemon bye")
        }
    }

    private fun sendInput(envelope: String): Boolean {
        val frame = JSONObject().put("t", "input").put("data", envelope).toString()
        val ok = ws?.send(frame) == true
        if (!ok) log("relay send failed (socket down)")
        return ok
    }

    // ── Connect / bench / reconnect ────────────────────────────────────────────
    private suspend fun connect() {
        link?.closeLink(); link = null; pump?.cancel()
        peerId = newPeerId(); seen.clear(); daemonId = null; daemonPub = null; connectMs = null; lastReconnect = null
        status("connecting to relay…")
        withTimeout(10_000) { openRelay().await() }
        // Wait for the replay (it ends with activity_synced), then use the LAST hello seen;
        // if the room had none, wait for the next one.
        withTimeoutOrNull(5_000) { synced.await() }
        if (daemonId == null) {
            status("waiting for the daemon hello…")
            helloWaiter = CompletableDeferred()
            withTimeout(20_000) { helloWaiter!!.await() }
        }
        val hub = daemonId!!
        log("using daemon $hub pub=${daemonPub?.take(12)}…")
        val (turnHost, turnPort) = turnField.text.toString().trim().let { it.substringBefore(':') to (it.substringAfter(':', "3478")) }
        val cfg = PeerConfig(
            peerId = peerId,
            sessionId = sessionField.text.toString().trim(),
            stun = "$turnHost:$turnPort",
            turn = "$turnHost:$turnPort",
            turnUsername = "exp",
            turnPassword = "spike",
            turnRealm = "exponential.local",
            policy = policy(),
            identitySeed = null,
            expectedRemotePubkey = daemonPub,
            tamperSdp = false,
            tamperSig = false,
        )
        status("creating offer…")
        connectStartedAt = SystemClock.elapsedRealtime()
        val l = withContext(Dispatchers.IO) { PeerLink(cfg) }
        link = l
        val offer = withContext(Dispatchers.IO) { l.createOffer() }
        sendInput(offer)
        log("offer sent (${offer.length} B)")
        startPump(l)
        val state = awaitConnected(l, 30_000)
        connectMs = (SystemClock.elapsedRealtime() - connectStartedAt).toDouble()
        status("state=$state in ${connectMs!!.toLong()} ms")
        log("state=$state after ${connectMs!!.toLong()} ms; diag=${l.diagnostics().take(600)}")
        if (state != LinkState.CONNECTED) error("not connected: $state")
    }

    private fun startPump(l: PeerLink) {
        pump?.cancel()
        pump = scope.launch(Dispatchers.IO) {
            while (isActive) {
                val out = l.pollSignals(200u)
                // runOnUiThread, not withContext: a cancelled pump must never drop an envelope it
                // already drained (that lost ICE-restart offers).
                for (e in out) runOnUiThread {
                    val type = runCatching { JSONObject(e).optString("type") }.getOrDefault("?")
                    if (type != "candidate") log("-> $type")
                    sendInput(e)
                }
                if (out.isEmpty() && l.state() == LinkState.CLOSED) break
            }
        }
    }

    private suspend fun awaitConnected(l: PeerLink, timeoutMs: Long): LinkState = withContext(Dispatchers.IO) {
        val deadline = SystemClock.elapsedRealtime() + timeoutMs
        var s = l.state()
        while (s != LinkState.CONNECTED && s != LinkState.FAILED && s != LinkState.CLOSED && SystemClock.elapsedRealtime() < deadline) {
            delay(10); s = l.state()
        }
        s
    }

    private suspend fun bench(): String {
        val l = link ?: error("connect first")
        status("bench running…")
        val r = withContext(Dispatchers.IO) { l.runBench(2_000_000uL, 100u) }
        val line = JSONObject().apply {
            put("kind", "bench"); put("at", Instant.now().toString()); put("source", source)
            put("scenario", scenarioSpinner.selectedItem as String); put("policy", policySpinner.selectedItem as String)
            put("connectMs", connectMs ?: r.connectMs); put("rttP50Ms", r.rttP50Ms); put("rttP95Ms", r.rttP95Ms)
            put("upMbps", r.upMbps); put("downMbps", r.downMbps); put("bodyBytes", r.bodyBytes.toLong())
            put("localPath", r.localPath); put("remotePath", r.remotePath)
            put("ok", r.errors.isEmpty()); put("error", if (r.errors.isEmpty()) JSONObject.NULL else r.errors.joinToString("; "))
            put("reconnect", lastReconnect ?: JSONObject.NULL)
            put("notes", "peer-ffi ${peerVersion()}; ${Build.MODEL} API ${Build.VERSION.SDK_INT}; local ${r.localCandidate}; remote ${r.remoteCandidate}; coreConnectMs ${r.connectMs}")
        }.toString()
        lastLine = line
        status("bench: rtt p50 ${"%.1f".format(r.rttP50Ms)} ms, up ${"%.1f".format(r.upMbps)} / down ${"%.1f".format(r.downMbps)} Mbps, ${r.localPath}/${r.remotePath}")
        log("RESULT $line")
        return line
    }

    private fun failureLine(err: Throwable): String = JSONObject().apply {
        put("kind", "bench"); put("at", Instant.now().toString()); put("source", source)
        put("scenario", scenarioSpinner.selectedItem as String); put("policy", policySpinner.selectedItem as String)
        put("connectMs", connectMs ?: JSONObject.NULL); put("ok", false); put("error", err.message ?: err.toString())
        put("reconnect", lastReconnect ?: JSONObject.NULL); put("bodyBytes", 2_000_000)
        put("notes", "peer-ffi ${runCatching { peerVersion() }.getOrDefault("?")}; ${Build.MODEL} API ${Build.VERSION.SDK_INT}")
    }.toString().also { lastLine = it }

    private fun reconnect(trigger: String) {
        scope.launch { doReconnect(trigger) }
    }

    private suspend fun doReconnect(trigger: String): Boolean {
        val l = link ?: run { log("reconnect($trigger): no link"); return false }
        if (reconnecting) return false
        reconnecting = true
        try {
            // A dead relay socket (background, network switch) is reopened first; the replay is deduped.
            val t0 = SystemClock.elapsedRealtime()
            if (ws?.send("""{"t":"ping"}""") != true || trigger == "network-switch") {
                // The old socket is bound to the dead network; retry while the new default settles.
                var opened = false
                for (attempt in 1..5) {
                    opened = runCatching { withTimeout(5_000) { openRelay().await() } }.isSuccess
                    if (opened) break
                    delay(500L * attempt)
                }
                if (!opened) error("relay unreachable after network change")
            }
            val answersBefore = answersAccepted
            withContext(Dispatchers.IO) { l.restartIce() }
            if (pump?.isActive != true) startPump(l) // the restart offer arrives through pollSignals
            // Done = the restart's answer is applied AND the link is Connected (str0m may never
            // leave Connected during a restart, so the state alone would measure nothing).
            withTimeoutOrNull(30_000) { while (answersAccepted == answersBefore) delay(5) }
            val state = awaitConnected(l, 30_000)
            val ms = SystemClock.elapsedRealtime() - t0
            lastReconnect = JSONObject().put("trigger", trigger).put("ms", ms)
            status("reconnect($trigger): $state in $ms ms")
            log("reconnect($trigger) -> $state in $ms ms")
            return state == LinkState.CONNECTED
        } catch (e: Throwable) {
            log("reconnect($trigger) failed: $e")
            return false
        } finally {
            reconnecting = false
        }
    }

    private fun watchNetwork() {
        val cm = getSystemService(ConnectivityManager::class.java)
        initialNetwork = cm.activeNetwork
        val cb = object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: Network) {
                runOnUiThread {
                    val prev = initialNetwork
                    initialNetwork = network
                    if (prev != null && prev != network && link != null) {
                        log("network switch $prev -> $network")
                        reconnect("network-switch")
                    }
                }
            }
        }
        cm.registerDefaultNetworkCallback(cb)
        netCallback = cb
    }

    // ── Results ────────────────────────────────────────────────────────────────
    private suspend fun sendResults(line: String? = lastLine) {
        val payload = line ?: run { log("nothing to send"); return }
        val env = resultEnvelope(peerId, daemonId, sessionField.text.toString().trim(), payload)
        // The daemon appends relayed results to its results file; the HTTP collector is the
        // FALLBACK only (both would double-count the line).
        if (daemonId != null && sendInput(env)) { log("result envelope sent via relay"); return }
        withContext(Dispatchers.IO) {
            val hosts = if (isEmulator) listOf("10.0.2.2", DevTickets.MAC_HOST) else listOf(DevTickets.MAC_HOST)
            for (h in hosts) {
                val ok = runCatching {
                    http.newCall(Request.Builder().url("http://$h:8787/results")
                        .post(payload.toRequestBody("application/json".toMediaType())).build()).execute().use { it.isSuccessful }
                }.getOrDefault(false)
                log("POST http://$h:8787/results -> ${if (ok) "ok" else "failed"}")
                if (ok) break
            }
        }
    }

    private fun copyJson() {
        val line = lastLine ?: return
        getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("vapp3", line))
        log("copied")
    }

    // ── Headless auto mode ─────────────────────────────────────────────────────
    private fun handleAuto(intent: android.content.Intent?) {
        val auto = intent?.getStringExtra("auto") ?: return
        intent.getStringExtra("policy")?.let { p -> policySpinner.setSelection(listOf("all", "relay", "host").indexOf(p).coerceAtLeast(0)) }
        intent.getStringExtra("scenario")?.let { s -> scenarioSpinner.setSelection(SCENARIOS.indexOf(s).coerceAtLeast(0)) }
        intent.getStringExtra("relay")?.let { relayField.setText(it) }
        intent.getStringExtra("ticket")?.let { ticketField.setText(it) }
        intent.getStringExtra("session")?.let { sessionField.setText(it) }
        intent.getStringExtra("turn")?.let { turnField.setText(it) }
        val runs = intent.getStringExtra("runs")?.toIntOrNull() ?: 1
        log("AUTO $auto runs=$runs policy=${policySpinner.selectedItem} scenario=${scenarioSpinner.selectedItem}")
        val keep = intent.getStringExtra("keep") == "true"
        scope.launch {
            repeat(runs) { i ->
                val line = try {
                    when (auto) {
                        // Bench the EXISTING link (e.g. after HOME + relaunch = the background reconnect).
                        "benchonly" -> {
                            delay(1_000) // onResume (after onNewIntent) starts the background reconnect
                            val deadline = SystemClock.elapsedRealtime() + 30_000
                            while (reconnecting && SystemClock.elapsedRealtime() < deadline) delay(50)
                            bench()
                        }
                        "reconnect" -> {
                            connect()
                            if (!doReconnect("manual")) error("reconnect failed: ${link?.state()}")
                            bench()
                        }
                        else -> { connect(); bench() }
                    }
                } catch (e: Throwable) {
                    log("AUTO run ${i + 1} failed: $e")
                    failureLine(e)
                }
                sendResults(line)
                Log.i(TAG, "AUTO_LINE $line")
                lastReconnect = null
                if (!keep) { link?.closeLink(); link = null; pump?.cancel() }
            }
            Log.i(TAG, "AUTO_DONE")
        }
    }

    companion object {
        val SCENARIOS = listOf("same-lan", "emulator-nat", "home-wifi-cellular", "home-office-wifi", "cellular-cellular")
        fun newPeerId() = "android-" + UUID.randomUUID().toString().take(8)
    }
}

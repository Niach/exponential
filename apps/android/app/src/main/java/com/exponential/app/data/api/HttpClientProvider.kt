package com.exponential.app.data.api

import com.exponential.app.AppConstants
import com.exponential.app.BuildConfig
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.ktor.client.HttpClient
import io.ktor.client.engine.okhttp.OkHttp
import io.ktor.client.plugins.DefaultRequest
import io.ktor.client.plugins.HttpResponseValidator
import io.ktor.client.plugins.HttpTimeout
import io.ktor.client.plugins.contentnegotiation.ContentNegotiation
import io.ktor.client.plugins.logging.LogLevel
import io.ktor.client.plugins.logging.Logger
import io.ktor.client.plugins.logging.Logging
import io.ktor.client.plugins.websocket.WebSockets
import io.ktor.client.request.header
import io.ktor.client.statement.bodyAsText
import io.ktor.client.statement.request
import io.ktor.serialization.kotlinx.json.json
import java.util.concurrent.TimeUnit
import javax.inject.Named
import javax.inject.Singleton
import okhttp3.ConnectionPool
import okhttp3.Dispatcher
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

@Module
@InstallIn(SingletonComponent::class)
object HttpClientModule {

    @Provides
    @Singleton
    fun provideJson(): Json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    /**
     * The ONE OkHttp connection pool every client shares (EXP-656).
     *
     * Exposed as its own binding because [com.exponential.app.data.electric.SyncManager]
     * has to `evictAll()` it when the app comes back from a background trip
     * long enough for the radio to have killed the pooled sockets silently:
     * OkHttp does not health-check a pooled connection before reusing it for a
     * GET, so the 19 resumed shape polls would otherwise ride a dead socket
     * until the HTTP/2 ping noticed. ktor's OkHttp engine derives its
     * per-request clients with `newBuilder()`, which SHARES this instance, so
     * one eviction covers shapes, tRPC and Coil alike.
     */
    @Provides
    @Singleton
    fun provideConnectionPool(): ConnectionPool = ConnectionPool(32, 5, TimeUnit.MINUTES)

    @Provides
    @Singleton
    fun provideHttpClient(json: Json, updateGate: UpdateGate, pool: ConnectionPool): HttpClient =
        buildClient(json, updateGate, pool, keepAlivePings = true)

    /**
     * The steer viewer sockets' own client (EXP-656) — identical to the shared
     * one except that it sets NO `pingInterval`.
     *
     * OkHttp applies that interval to a WebSocket as a PONG DEADLINE: a relay
     * that misses one pong inside the window has its socket FAILED, which is
     * exactly the "the chat reconnects every ~30s and yanks me to the bottom"
     * report. The shape client keeps its ping (it is what earns the right to
     * multiplex 19 long-polls onto one connection); steer liveness is already
     * covered by the relay's own 15s `keepalive` frames plus
     * [com.exponential.app.data.steer.SteerTimings.liveStaleMs].
     */
    @Provides
    @Singleton
    @Named(STEER_CLIENT)
    fun provideSteerHttpClient(
        json: Json,
        updateGate: UpdateGate,
        pool: ConnectionPool,
    ): HttpClient = buildClient(json, updateGate, pool, keepAlivePings = false)

    private fun buildClient(
        json: Json,
        updateGate: UpdateGate,
        pool: ConnectionPool,
        keepAlivePings: Boolean,
    ): HttpClient =
        HttpClient(OkHttp) {
            expectSuccess = false
            // ENGINE CHOICE — OkHttp, not CIO (EXP-304). CIO is HTTP/1.1-only
            // with a pure-Kotlin TLS stack, so the 16 Electric shape loops (per
            // signed-in account!) each opened their own connection: app start
            // fired 16 simultaneous cold DNS lookups + TLS handshakes at the
            // same host, and that storm is what the "~10s before fresh data
            // shows up" reports were. Sync diagnostics caught it twice: 2x
            // "Connect timeout has expired" on all 16 shapes (5s budget, twice
            // = the reported 10s), then 10x UnresolvedAddressException on 15 of
            // 16 while the sixteenth resolved fine — resolver contention, not a
            // down network. OkHttp negotiates HTTP/2 via ALPN and MULTIPLEXES
            // every shape long-poll (and every tRPC call) onto ONE connection:
            // one lookup, one handshake, one thing to keep alive. It also
            // brings native TLS, a real connection pool, and transparent gzip
            // (it adds Accept-Encoding itself and decompresses, as long as we
            // never set that header — so don't).
            engine {
                config {
                    // OkHttp's Dispatcher defaults to maxRequestsPerHost = 5.
                    // The ktor OkHttp engine dispatches through `enqueue`, so
                    // leaving that default would park 11 of the 16 shape loops
                    // behind the other 5 FOREVER — every one of them is a
                    // minutes-long live long-poll that never frees its slot.
                    // Sized for several signed-in accounts (16 shapes each,
                    // EXP-314 added issue_statuses) plus tRPC and image loads
                    // on top — the per-host cap MUST stay comfortably above
                    // accounts × shapes or the extra loops starve.
                    dispatcher(
                        Dispatcher().apply {
                            maxRequests = 160
                            maxRequestsPerHost = 80
                        }
                    )
                    connectionPool(pool)
                    retryOnConnectionFailure(true)
                    // Putting every stream on one connection means one dead
                    // connection stalls everything, and a phone radio kills
                    // idle sockets silently. HTTP/2 keepalive pings surface
                    // that in ~30s instead of when each stream's 90s socket
                    // budget expires; this is what earns the right to
                    // multiplex. NOT set on the steer client: on a WebSocket
                    // the same interval is a pong deadline (EXP-656).
                    if (keepAlivePings) pingInterval(30, TimeUnit.SECONDS)
                }
            }
            // Still required after the engine swap: an engine left to its own
            // defaults enforces a request timeout BELOW the Electric live
            // long-poll hold window (~20s on prod, up to ~60s per
            // long-poll-canary.md), so every idle shape poll dies with
            // "Request timeout has expired" (EXP-61: errors across every synced
            // shape, sync frozen — CIO's 15s default at the time, and OkHttp
            // would apply its own 10s read timeout here). Worse, CIO enforced
            // it by CANCELLING the request job, which could kill the shape
            // run-loop outright. HttpTimeout replaces that with a plugin-level
            // typed exception and maps onto whatever the engine uses;
            // ShapeClient raises the per-request budget above the hold window
            // (iOS/desktop parity: both use 90s for shape reads, 30s for
            // everything else).
            install(HttpTimeout) {
                requestTimeoutMillis = 30_000
                connectTimeoutMillis = 10_000
                socketTimeoutMillis = 30_000
            }
            install(ContentNegotiation) { json(json) }
            // Steer viewer sockets (relay PTY mirror, masterplan §5c) dial
            // through the @Named(STEER_CLIENT) variant; the plugin is inert
            // for plain HTTP calls, so both clients install it.
            install(WebSockets)
            if (BuildConfig.DEBUG) {
                install(Logging) {
                    level = LogLevel.INFO
                    logger = object : Logger {
                        override fun log(message: String) {
                            android.util.Log.d("ktor", message)
                        }
                    }
                }
            }
            install(DefaultRequest) {
                header("Accept", "application/json")
                // Client versioning + min-version gate contract (EXP-104). Every
                // request (tRPC AND Electric shape polls — they share this client)
                // carries the version so the server can 426 an under-min build.
                header("x-client-version", AppConstants.CLIENT_VERSION_HEADER_VALUE)
            }
            // A custom validator runs even with expectSuccess = false, so this is
            // the single choke point that catches the server's HTTP 426
            // ("client_upgrade_required") across every tRPC and shape response and
            // latches the update gate FOR THAT SERVER (REV2-18: every signed-in
            // account polls its own instance through this one shared client, so
            // the latch must be keyed by the responding origin — a foreign
            // server's minimum can't be allowed to gate the whole app).
            // Parsing is fully defensive — the min/latest fields may be absent,
            // and a body that won't decode must never mask the 426 signal.
            HttpResponseValidator {
                validateResponse { response ->
                    if (response.status.value == 426) {
                        val info = runCatching {
                            val obj = json.parseToJsonElement(response.bodyAsText()).jsonObject
                            UpdateGate.UpgradeInfo(
                                min = obj["min"]?.jsonPrimitive?.contentOrNull,
                                latest = obj["latest"]?.jsonPrimitive?.contentOrNull,
                            )
                        }.getOrDefault(UpdateGate.UpgradeInfo(min = null, latest = null))
                        updateGate.trigger(response.request.url.toString(), info)
                    }
                }
            }
        }
}

/** Hilt qualifier for the ping-free steer WebSocket client (EXP-656). */
const val STEER_CLIENT = "steer"

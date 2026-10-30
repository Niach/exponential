// #EXP-1441: the Kotlin binding suite (JNA on the JVM; Android links the same
// bindings). Replays packages/etalk/fixtures/voice-call.json through the facade
// (the events built as Kotlin classes, so every case crosses the binding both
// ways) and round-trips a call's audio through the codec.
//   bash apps/desktop/crates/etalk-ffi/run-binding-tests.sh kotlin
package tests

import at.exponential.talk.ffi.*
import java.io.File
import kotlin.math.PI
import kotlin.math.sin
import kotlin.system.exitProcess

var failures = 0
var checks = 0

fun check(ok: Boolean, message: () -> String) {
    checks += 1
    if (!ok) {
        failures += 1
        println("FAIL: ${message()}")
    }
}

// A tiny JSON reader/writer (no library on the test classpath): parses into
// Map/List/String/Double/Boolean/null and writes them back.
object Json {
    fun parse(text: String): Any? = Parser(text).value()
    fun write(v: Any?): String = StringBuilder().also { emit(v, it) }.toString()

    private fun emit(v: Any?, sb: StringBuilder) {
        when (v) {
            null -> sb.append("null")
            is String -> { sb.append('"'); for (c in v) when (c) { '"' -> sb.append("\\\""); '\\' -> sb.append("\\\\"); '\n' -> sb.append("\\n"); '\r' -> sb.append("\\r"); '\t' -> sb.append("\\t"); else -> if (c < ' ') sb.append(String.format("\\u%04x", c.code)) else sb.append(c) }; sb.append('"') }
            is Boolean -> sb.append(v)
            is Double -> if (v == Math.floor(v) && !v.isInfinite() && Math.abs(v) < 1e15) sb.append(v.toLong()) else sb.append(v)
            is Number -> sb.append(v)
            is Map<*, *> -> { sb.append('{'); var first = true; for ((k, x) in v) { if (!first) sb.append(','); first = false; emit(k as String, sb); sb.append(':'); emit(x, sb) }; sb.append('}') }
            is List<*> -> { sb.append('['); var first = true; for (x in v) { if (!first) sb.append(','); first = false; emit(x, sb) }; sb.append(']') }
            else -> emit(v.toString(), sb)
        }
    }

    private class Parser(val s: String) {
        var i = 0
        fun ws() { while (i < s.length && s[i].isWhitespace()) i++ }
        fun value(): Any? {
            ws()
            return when (s[i]) {
                '{' -> { i++; val m = LinkedHashMap<String, Any?>(); ws(); if (s[i] == '}') { i++; return m }; while (true) { ws(); val k = str(); ws(); i++; m[k] = value(); ws(); if (s[i] == ',') { i++; continue }; i++; return m } }
                '[' -> { i++; val l = ArrayList<Any?>(); ws(); if (s[i] == ']') { i++; return l }; while (true) { l.add(value()); ws(); if (s[i] == ',') { i++; continue }; i++; return l } }
                '"' -> str()
                't' -> { i += 4; true }
                'f' -> { i += 5; false }
                'n' -> { i += 4; null }
                else -> { val start = i; while (i < s.length && (s[i].isDigit() || s[i] in "+-.eE")) i++; s.substring(start, i).toDouble() }
            }
        }
        fun str(): String {
            i++
            val sb = StringBuilder()
            while (s[i] != '"') {
                if (s[i] == '\\') { i++; when (s[i]) { 'n' -> sb.append('\n'); 't' -> sb.append('\t'); 'r' -> sb.append('\r'); 'b' -> sb.append('\b'); 'f' -> sb.append('\u000c'); 'u' -> { sb.append(s.substring(i + 1, i + 5).toInt(16).toChar()); i += 4 }; else -> sb.append(s[i]) }; i++ } else { sb.append(s[i]); i++ }
            }
            i++
            return sb.toString()
        }
    }
}

fun platform(v: Any?) = when (v) {
    "desktop" -> TalkCallPlatform.DESKTOP
    "phone" -> TalkCallPlatform.PHONE
    else -> TalkCallPlatform.WEB
}

fun speech(v: String) = when (v) {
    "ok" -> TalkCallSpeech.OK
    "noGpu" -> TalkCallSpeech.NO_GPU
    "failed" -> TalkCallSpeech.FAILED
    else -> TalkCallSpeech.PREPARING
}

fun button(v: Any?) = when (v) {
    "mute" -> TalkCallButton.MUTE
    "speaker" -> TalkCallButton.SPEAKER
    else -> TalkCallButton.END
}

fun event(e: Map<String, Any?>): TalkCallEvent {
    val text = (e["text"] as String?) ?: ""
    return when (e["t"]) {
        "start" -> TalkCallEvent.Start(e["device"] as String, e["language"] as String?)
        "rang" -> TalkCallEvent.Rang
        "ready" -> TalkCallEvent.Ready
        "noSound" -> TalkCallEvent.NoSound
        "partial" -> TalkCallEvent.Partial(text)
        "said" -> TalkCallEvent.Said(text)
        "speechStarted" -> TalkCallEvent.SpeechStarted
        "queued" -> TalkCallEvent.Queued((e["count"] as Double).toUInt(), text)
        "replyStarted" -> TalkCallEvent.ReplyStarted(text)
        "replyFinished" -> TalkCallEvent.ReplyFinished
        "mute" -> TalkCallEvent.Mute
        "unmute" -> TalkCallEvent.Unmute
        "speaker" -> TalkCallEvent.Speaker(e["on"] as Boolean)
        "end" -> TalkCallEvent.End
        "hungUp" -> TalkCallEvent.HungUp(e["reason"] as String?)
        "failed" -> TalkCallEvent.Failed(e["code"] as String)
        else -> error("unknown event ${e["t"]}")
    }
}

@Suppress("UNCHECKED_CAST")
fun main(args: Array<String>) {
    val text = if (args.isNotEmpty()) File(args[0]).readText() else callSpecJson()
    check(args.isEmpty() || text == callSpecJson()) { "the facade embeds the committed fixture" }
    val fixture = Json.parse(text) as Map<String, Any?>
    fun obj(v: Any?) = v as Map<String, Any?>
    fun list(v: Any?) = v as List<Any?>
    fun num(v: Any?) = v as Double?

    // availability + speech
    for (c in list(fixture["availabilityCases"]).map(::obj)) {
        val i = obj(c["input"])
        val e = obj(c["expected"])
        val reason = callUnavailable(TalkCallAvailabilityInput(
            i["agent"] as Boolean, platform(i["platform"]), i["device"] as String?, i["online"] as Boolean, i["here"] as Boolean,
            (i["speech"] as String?)?.let(::speech), i["busy"] as Boolean, (i["meeting"] as Boolean?) ?: false))
        check(reason == e["reason"]) { "availability ${c["name"]}: $reason" }
        check((reason == null) == e["enabled"]) { "availability ${c["name"]} enabled" }
    }
    for (c in list(fixture["speechCases"]).map(::obj)) {
        check(callSpeechOf(c["state"] as String?) == (c["expected"] as String?)?.let(::speech)) { "speech ${c["name"]}" }
    }

    // the flow
    val flows = list(fixture["flowCases"]).map(::obj)
    check(flows.size >= 41) { "41+ flow cases" }
    for (c in flows) {
        val name = c["name"]
        var state = callIdle()
        for (e in list(c["events"]).map(::obj)) {
            val native = event(e)
            // The wire form of the same event decodes to the same class.
            check(callEventOfWire(Json.write(e)) == native) { "$name: wire ${Json.write(e)}" }
            state = callStep(state, native)
        }
        val view = callView(state, platform(c["platform"]))
        val expected = c["view"] as Map<String, Any?>?
        if (expected == null) {
            check(view == null) { "$name: expected hidden" }
        } else if (view == null) {
            check(false) { "$name: hidden" }
        } else {
            check(view.activity == expected["activity"]) { "$name: activity ${view.activity}" }
            check(view.status == expected["status"]) { "$name: status ${view.status}" }
            check(view.caption == expected["caption"]) { "$name: caption ${view.caption}" }
            check(view.muted == expected["muted"]) { "$name: muted" }
            check(view.speaker == expected["speaker"]) { "$name: speaker" }
            check(view.buttons == list(expected["buttons"]).map(::button)) { "$name: buttons ${view.buttons}" }
            check(view.end == expected["end"]) { "$name: end ${view.end}" }
        }
        check(state.toast == c["toast"]) { "$name: toast ${state.toast}" }
    }

    // the ring + the timer
    for (c in list(fixture["ringCases"]).map(::obj)) check(callRingMs(num(c["random"])!!) == num(c["ringMs"])!!.toLong().toULong()) { "ring $c" }
    for (c in list(fixture["answerCases"]).map(::obj)) {
        val answered = callAnswerMs(num(c["random"])!!, num(c["readyMs"])?.toLong()?.toULong())
        check(answered == num(c["answeredMs"])?.toLong()?.toULong()) { "answer ${c["name"]}" }
    }
    for (c in list(fixture["ringToneCases"]).map(::obj)) check(callRingToneOn(num(c["ms"])!!.toLong()) == c["on"]) { "ring tone $c" }
    for (c in list(fixture["timerCases"]).map(::obj)) check(callTimer(num(c["ms"])!!.toLong().toULong()) == c["text"]) { "timer $c" }
    repeat(50) { val r = callRingRandom(); check(r >= 0.0 && r < 1.0) { "random $r" } }
    val k = callConstants()
    check(k.minRingMs == num(obj(fixture["constants"])["minRingMs"])!!.toLong().toULong()) { "constants" }
    check(callGreeting("de") == "Hallo?" && callGreeting(null) == "Hello?") { "greeting" }
    check(callWireEvent(TalkCallEvent.Mute) == null) { "mute stays local" }

    // the codec: a 440 Hz tone, one packet lost and recovered by the uplink's FEC
    val encoder = TalkEncoder(k.upRate, (k.upKbps * 1000u).toInt(), 10)
    val frame = encoder.frameSamples().toInt()
    check(frame == (k.upRate * k.frameMs / 1000u).toInt()) { "frame samples" }
    val decoder = TalkDecoder(k.upRate)
    val uplink = TalkUplink(k.upRate)
    var energy = 0.0
    for (n in 0 until 25) {
        val pcm = (0 until frame).map { i -> (sin((n * frame + i).toDouble() * 440 * 2 * PI / k.upRate.toDouble()) * 8000).toInt().toShort() }
        val packet = encoder.encode(pcm)
        check(packet.isNotEmpty() && packet.size <= k.maxFrameBytes.toInt()) { "packet $n size ${packet.size}" }
        val out = decoder.decode(packet)
        check(out.size == frame) { "decoded ${out.size}" }
        energy += out.sumOf { it.toDouble() * it.toDouble() }
        if (n != 10) {
            val pushed = uplink.push(n.toUShort(), packet)
            check(pushed.size == if (n == 11) 2 * frame else frame) { "uplink $n: ${pushed.size}" }
        }
    }
    check(energy > 0) { "the tone decodes to sound" }
    check(decoder.decode(ByteArray(0)).size == frame) { "an empty packet conceals" }
    check(try { encoder.encode(listOf(0, 0, 0)); false } catch (e: TalkException.Invalid) { true }) { "a short frame throws" }
    check(try { TalkEncoder(44100u, 24000, null); false } catch (e: TalkException.Invalid) { true }) { "44.1 kHz throws" }
    check(codecBackend() == "opus-rs") { "backend" }
    encoder.close(); decoder.close(); uplink.close()

    println("$checks checks, $failures failures")
    exitProcess(if (failures == 0) 0 else 1)
}

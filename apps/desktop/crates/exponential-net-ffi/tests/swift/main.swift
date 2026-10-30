// The eNet Swift binding against the host dylib: two nodes over localhost
// (relays off), chat + a record + push sealing. run-binding-tests.sh builds it.
import Foundation

func check(_ ok: Bool, _ what: String) {
    if !ok { print("FAIL: \(what)"); exit(1) }
    print("ok: \(what)")
}

func localConfig() -> EnetConfig {
    var c = defaultConfig()
    c.relaysEnabled = false
    c.bindAddr = "127.0.0.1:0"
    return c
}

// push sealing (what a Notification Service Extension does)
let kp = pushKeypairGenerate()
let sealed = try pushSeal(recipientPublic: kp.public, plaintext: Data("wake".utf8))
check(try pushOpen(recipientSecret: kp.secret, sealed: sealed) == Data("wake".utf8), "pushOpen round trip")
var tampered = sealed
tampered[tampered.count - 1] ^= 1
check((try? pushOpen(recipientSecret: kp.secret, sealed: tampered)) == nil, "tampered push refused")

let a = try await EnetNode.start(config: localConfig(), identity: generateIdentity())
let b = try await EnetNode.start(config: localConfig(), identity: generateIdentity())
check(try parseTicket(ticket: a.ticket()).endpointId == a.endpointId(), "ticket parses")
_ = try a.addTicket(ticket: b.ticket())
_ = try b.addTicket(ticket: a.ticket())
try a.allowPeer(peer: b.endpointId())
try b.allowPeer(peer: a.endpointId())

let sent = try await a.sendChat(peer: b.endpointId(), body: Data("hi from swift".utf8))
if case let .chat(from, message)? = await b.nextEvent() {
    check(from == a.endpointId() && message.body == Data("hi from swift".utf8) && message.id == sent.id, "chat delivered")
} else { check(false, "chat event") }

let space = Data(repeating: 9, count: 32)
try await a.joinSpace(space: space, bootstrap: [])
_ = try await a.putRecord(space: space, key: "title", value: Data("v".utf8))
try await b.joinSpace(space: space, bootstrap: [a.endpointId()])
if case let .record(record)? = await b.nextEvent() {
    check(record.key == "title" && record.author == a.endpointId(), "record synced")
} else { check(false, "record event") }

try await a.shutdown()
try await b.shutdown()
print("swift suite passed")

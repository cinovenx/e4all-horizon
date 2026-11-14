# The relay's protocol

What this mod speaks to e4mc's relay, and what the relay speaks back. The
server side is [vgskye/e4mc-quiclime](https://github.com/vgskye/e4mc-quiclime)
(MIT OR Apache-2.0); the client side here is a port of e4mc's and e4all's Java
client.

## The short version

```
   host (this mod)                          relay (e4mc)
   ─────────────────                        ─────────────
   GET https://broker.e4mc.link/getBestRelay
        ← {"id":"eu","host":"eu.e4mc.link","port":25575}

   QUIC connect (TLS 1.3, ALPN "quiclime")
   open one bidirectional stream  ─────────►  the control channel
        ─────────────────────────►  {"kind":"probe_capabilities"}
        ◄─────────────────────────  {"kind":"has_capabilities","caps":[...]}
        ─────────────────────────►  {"kind":"request_domain_assignment"}
        ◄─────────────────────────  {"kind":"domain_assignment_complete",
                                        "domain":"word-word.eu.e4mc.link"}

   a guest joins "word-word.eu.e4mc.link" on the relay's TCP port
                                   relay opens a bidirectional stream ──────►
        ◄─────────────────────────────────────────────────────────  the guest's
                                                                     Minecraft
                                                                     bytes
```

The relay carries the game's TCP stream **unchanged**: the host's LAN port
speaks Minecraft to the relay, and the relay speaks it to the guest.

## The control channel

One bidirectional QUIC stream, opened by the client, carries the control
messages. Every message is a length and that many bytes of JSON:

* the client writes the length as a `VarInt` (big-endian base 128),
* the relay writes it as a single byte.

Every message either side sends is shorter than 128 bytes, so the two are the
same bytes on the wire and one reader handles both. The relay refuses a
message longer than 8192 bytes.

The JSON is a `kind`-tagged object:

| Direction | `kind` | Fields |
| --- | --- | --- |
| → relay | `probe_capabilities` | — |
| → relay | `request_domain_assignment` | — |
| → relay | `dialtone_register_ticket` | `ticket` |
| ← relay | `has_capabilities` | `caps` |
| ← relay | `domain_assignment_complete` | `domain` |
| ← relay | `ticket_registered` | — |
| ← relay | `request_message_broadcast` | `message` |
| ← relay | `unknown_message` | — |

**The control stream is never finished.** The relay reads it for as long as
the session lasts; a FIN on it ends the relay's control loop, and the relay
drops the connection and the domain with it. (This is the one detail that
makes the difference between a working tunnel and a relay that closes the
connection the moment the address is asked for.)

After the address is assigned, the relay keeps answering the keepalive
probes with `has_capabilities`: the probe is a liveness check, not a question.
Any answer counts, and the counter of unanswered probes is cleared by it.

## The players

The relay opens **one bidirectional stream per joining player**, and each
carries that player's Minecraft connection. The host bridges each of them to
`127.0.0.1:<the LAN's port>`, in both directions, and the game sees an
ordinary local connection.

## The relay's transport settings

The relay's QUIC configuration allows one client-initiated bidirectional
stream (the control channel) and as many server-initiated ones as it likes
(the players). The client's own settings allow the server to open streams.

## The peer-to-peer ticket (not used here)

The relay also serves `/.well-known/dialtone_ticket/{domain}`, which answers
the *host's* peer-to-peer ticket, so a guest can reach the host directly. That
path is e4all's Dialtone sidecar, which needs the `iroh` stack; see
[PORTING.md](PORTING.md) for why this port does not speak it.

## The broker

`GET https://broker.e4mc.link/getBestRelay` answers with the nearest relay:

```json
{"id": "eu", "host": "eu.e4mc.link", "port": 25575}
```

The port is 25575 — the relay's **QUIC** port, not Minecraft's 25565. A guest
reaches the world on the relay's ordinary Minecraft port, through the domain
the relay handed out.

**The guest dials the domain's own host**, not the host the broker named:
`word-word.eu.e4mc.link:25565`, with `word-word.eu.e4mc.link` in the
Minecraft handshake's server address. The relay routes the connection by that
address. Dialing the broker's host instead can land on a different relay in
the cluster, which does not know the domain and answers with a "unknown
domain" disconnect.

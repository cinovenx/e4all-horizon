# Porting e4all to Horizon

What e4all is, and what became of each of its parts here. e4all is a Java
mod ([Synquox/e4all](https://github.com/Synquox/e4all), MIT), a fork of
[e4mc](https://github.com/vgskye/e4mc-minecraft-architectury) (MIT) with a few
additions of its own. This port is a Horizon **native mod**: a
[horizon-mixins](https://github.com/horizon-rust/horizon-mixins) bundle that
changes the game's own Rust source before it is compiled.

## The parts that came over

| e4all (Java) | Here (Rust) |
| --- | --- |
| `link.e4mc.QuiclimeSession` — the QUIC connection to the relay, the control channel, the handshake | `e4all_core::session` — the same connection (ALPN `quiclime`), the same handshake, the same control messages |
| `link.e4mc.QuiclimeSession.ControlMessageCodec` — the length-and-JSON codec | `e4all_core::protocol` — the same framing, the same `kind`-tagged messages |
| The broker request (`broker.e4mc.link/getBestRelay`) | `e4all_core::broker` |
| The stream-to-TCP bridge (`ToMinecraftHandler` / `ToQuiclimeHandler`) | `e4all_core::session::bridge` |
| `link.e4mc.Mirror` — the chat line with the address, clickable to copy | `game::e4all::announce` — Horizon's `Text::copyable` does the click |
| `link.e4all.E4allClient.registerCommands` — `/e4all stop` | `game::e4all::commands` and `game::e4all::command` — `/e4all`, `/e4all status`, `/e4all start`, `/e4all stop`, `/e4all domain` |
| `link.e4all.Config` — the settings and their defaults | `e4all_core::config::Config` — the same names, snake_cased, the same defaults, in `config/e4all.json` |
| The config screen | Options → Mods → e4all → Settings (Horizon's own page for native mods) |
| `link.e4all.mixin.OfflineModeMixin` — the LAN's online mode | The `auth::current` override: while the world is opened in offline mode, the account is hidden, so `OnlineMode::lan()` answers `None` and the LAN runs offline |
| `link.e4all.mixin.IntegratedServerPauseMixin` — keeping a hosted world ticking | The `Integrated::set_paused` override: the same rule, at the same place in the game |
| `link.e4all.ServerStartupTracker` — following the LAN's life | The `native_server_tick` hook: the session exists exactly while the world is published |

## The parts that did not come over, and why

| e4all | Why |
| --- | --- |
| **The guest side** — joining somebody else's e4mc world | The guest speaks a different protocol on a different ALPN (`e4mc-dialtone`) and needs a dialtone resolver and a ticket of its own. This port is the **host** side: the mod shares a Horizon world over the relay. |
| **Simple Voice Chat (SVC) over peer-to-peer** | e4all's voice rides on *Dialtone*, a peer-to-peer stack built on [iroh](https://github.com/iroh-computations/iroh). A Horizon native mod may only name crates that are already in Horizon's `Cargo.lock`, and `iroh` is not in it (nor may a bundle add one). The relay side of the ticket is here — `e4all_core::ticket` knows the `v1_` format, the sanitizing and the cache — but the peer-to-peer transport itself is not, so voice chat is not part of this port. |
| **Krypton compatibility** | Krypton is a Netty transport for *Java* Minecraft. Horizon has no Netty and no Java mods: its LAN is its own stack. There is nothing to be incompatible with. |
| **Android support** | e4all shipped a separate Android jar because a Java mod cannot load on Android's launcher. A native Rust mod is one build for every system Horizon itself runs on; Android is Horizon's own question, not the mod's. |
| **`restoreDedicatedCommands`, `useWhiteList`** | These restored Minecraft's dedicated-server commands (`/ban`, `/whitelist`) on a singleplayer world. Horizon's command tree is its own, and its LAN world already has the owner-only rules it needs; there is nothing to restore. |
| **`loginReadyTimeoutMs`** | e4all waited for a starting world's first tick before letting a guest log in. Horizon's integrated server publishes the LAN only once the world is open and ticking, so there is no window to wait out. The setting is kept in the config for parity and is not read. |
| **No chat reports (NCR)** | On Horizon this is already the case for a shared world: a LAN world's network configuration carries no secure chat (`NetConfig::secure_chat` is `None` for a LAN world, and only a dedicated server has one), so chat in a shared world is unsigned and no signature is demanded of the guests. Nothing had to be done, and no switch is offered for it. |
| **`offlineWarningShown`, `welcomeShown`** | e4all showed a warning and a welcome text once, on its own screens. There are no such screens here: the offline mode is a switch on the mod's own settings page, which says what it does. The settings are kept in the config for parity. |

## The offline mode, and when it takes hold

Horizon decides the LAN's online mode **while the world opens**
(`OnlineMode::lan()` reads the signed-in account when the world's server
starts) and never reads it again. So the `offline_mode` switch is read at
`server_about_to_start`, before the world's server starts, and a switch
flipped while a world is running takes hold the next time that world opens.
e4all had the same rule in practice: its button was on the Open to LAN
screen, and the mode was fixed when the world was published.

Horizon also ties the online mode to the host's account: without a session
file from the launcher, `OnlineMode::lan()` answers `None` and the LAN hosts
offline whatever the switch says. The settings page therefore shows the
switch only when the game has an account; a player playing without one is
already hosting offline, and a switch that cannot change that would only
promise a choice they do not have. The value stays in `config/e4all.json` and
takes effect after the game is started with an account.

The switch itself is a single `AtomicBool` beside `auth`, in Horizon's render
crate, because that is where `auth::current` lives; the game crate sets it
when the world opens and clears it when the world closes.

## The pause of a hosted world

Vanilla pauses a single-player world when the pause menu is open, and a paused
world freezes every guest connected to it. e4all overrode the pause the same
way this port does: the world keeps ticking while the mod is hosting it. Here
that is `Integrated::set_paused`, the one place Horizon decides whether the
integrated server ticks, so the override is a single argument.

## What the port adds of its own

* `/e4all status` and `/e4all domain`, which e4all did not have as commands.
* The relay session **recycles**: a probe that goes unanswered three times in
  a row means the link is dead, and the session dials the relay again rather
  than keeping an address nobody can reach. e4all had a watchdog for the same
  thing.
* The address can be **hidden in chat** (`hide_domain_in_chat`), e4all's own
  setting. Here the chat line is left out entirely and `/e4all domain` still
  shows the address to the player who asks for it.

## Credits and licensing

e4all is MIT (JustPixel_0), e4mc is MIT (vgskye and contributors), the relay's
server side is MIT OR Apache-2.0 (`vgskye/e4mc-quiclime`), and Horizon itself
is MIT. This port is MIT; see `NOTICE` for the full credit.

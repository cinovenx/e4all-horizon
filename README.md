# e4all for Horizon

> **An unofficial port.** This is not e4all, and it is not made or endorsed by
> the e4all project or its authors. It is a from-scratch Rust mod for
> [Horizon](https://github.com/horizon-rust/horizon) that speaks the e4mc
> relay's protocol, written so Horizon players can share a LAN world the way
> e4all lets Minecraft players share one — including friends on **offline
> accounts** with no Microsoft login. Bug reports belong here, not with e4all.

Open your Horizon LAN world to friends anywhere, **without port forwarding**.
The mod asks the e4mc relay for a public domain, you send that domain to your
friends, and they join from a plain Minecraft client.

e4all itself is by [JustPixel_0](https://github.com/Synquox/e4all) (MIT), a
fork of [e4mc](https://github.com/vgskye/e4mc-minecraft-architectury) (MIT);
the relay protocol follows [e4mc-quiclime](https://github.com/vgskye/e4mc-quiclime)
(MIT OR Apache-2.0). See [NOTICE](NOTICE) for the full credit.

## Use

1. Open your world.
2. Type `/e4all start`. The mod opens the LAN itself and asks the relay for an
   address.
3. Chat shows `e4all: word-word.eu.e4mc.link  (click to copy)` — click the
   address to copy it.
4. Send it to your friends; they paste it into **Multiplayer → Direct
   Connection**.
5. `/e4all stop` closes the address and keeps it closed. Closing the world
   closes it too.

Opening the LAN by hand (Esc → Open to LAN) works the same: the mod notices
and publishes an address for it.

### Friends without a Minecraft account

e4all's own addition to e4mc. Turn it on and the world runs **without Mojang
authentication** while it is shared, so a friend who has no account (or no
Microsoft login) can join under any name:

- **Options → Mods → e4all → Settings → "Offline mode (no Mojang login for
  guests)"**, or
- `offline_mode = true` in `config/e4all.json`.

The setting is read **while the world opens**, because that is when Horizon
decides the LAN's online mode, and it is not read again. Close the world and
open it once more to change it.

> **It is off by default, and while it is on it applies to every LAN world you
> open** — not only the ones shared over the relay, and even if *Share this
> world over the relay* is off. That is e4all's own behaviour: its switch
> disables Microsoft authentication for all LAN connections. Turn it off when
> you no longer need guests without an account.

### In game: Options → Mods

e4all has its own row in **Options → Mods** like any other mod, with a
**Settings** button next to it. Its page has the switches, applied and saved
as you click:

| Switch | What it does |
| --- | --- |
| Share this world over the relay | Publish an address as soon as the world is shared. |
| Keep the world ticking while hosting | e4all's `preventPauseWhileHosting`: guests are not frozen while the host has the pause menu open. |
| Offline mode (no Mojang login for guests) | Offline mode, above. |
| Hide the address in chat | The address stays out of the public chat; `/e4all domain` still shows it to the player who asks. |

### Commands

| Command | What it does |
| --- | --- |
| `/e4all` / `/e4all status` | whether it is open, the address, the LAN port |
| `/e4all start` | open the LAN if needed, then get an address |
| `/e4all stop` | close the address and keep it closed (the LAN stays open) |
| `/e4all domain` | print the address |

## Config

`<game home>/config/e4all.json`, written with the defaults on the first run.
The names are e4all's own, snake_cased, and the defaults are the ones e4all's
Java mod ships:

```json
{
  "use_broker": true,
  "broker_url": "https://broker.e4mc.link/getBestRelay",
  "relay_host": "test.e4mc.link",
  "relay_port": 25575,
  "restore_dedicated_commands": true,
  "use_white_list": false,
  "host_enabled": true,
  "prevent_pause_while_hosting": true,
  "dialtone_host_enabled": true,
  "dialtone_player_enabled": true,
  "dialtone_relay_map": "https://natives.e4mc.link/relaymap.json",
  "dialtone_sanitize_ticket": true,
  "keepalive_interval_seconds": 15,
  "login_ready_timeout_ms": 15000,
  "relay_connect_timeout_ms": 15000,
  "offline_mode": false,
  "offline_warning_shown": false,
  "welcome_shown": false,
  "hide_domain_in_chat": false,
  "voice_p2p_enabled": true
}
```

The switches the settings page offers are the four that matter in the game;
the rest are kept so the file reads like e4all's own.

## How it works

The mod is a **Horizon native mod**: a `horizon-mixins` bundle that changes the
game's own Rust source before it is compiled. It has two parts:

* `e4all/mixins/e4all/core` — a crate with the relay session: the broker, the QUIC
  control channel (ALPN `quiclime`), the Dialtone ticket format and the bridge
  that carries a joining player's stream to the world's own TCP port. It knows
  nothing about the game, so it is tested with `cargo test` alone.
* `mixins/e4all/runtime.rs` and its helpers — the glue: when the world is
  shared, which port it is shared on, what the players are told, the settings
  page and the commands.

Four of Horizon's own files are touched, by ten small injections and two
inserted modules:

| Where | What |
| --- | --- |
| `horizon_client_render::auth::current` | the account is hidden while a world is opened in offline mode, so the LAN runs offline |
| `horizon_client::game::integrated::Integrated::set_paused` | the world keeps ticking while it is hosted |
| `horizon_client::game::native_hooks` (server start, server tick, world lifecycle, commands, the mod list, the settings page and its clicks) | the session's life inside the game |
| `horizon_client::game` | the mod's own module (`game::e4all`) is added here |

Two modules are inserted rather than rewritten: `game::e4all`, which carries
the mod's code, and `auth::e4all_auth_suppress`, the flag the offline mode is
read from — beside `auth` in the render crate, because a module of the game
crate is not visible from there.

See [PORTING.md](PORTING.md) for the feature-by-feature comparison with the
Java mod, and [PROTOCOL.md](PROTOCOL.md) for the relay's wire format.

## Install

In the Horizon launcher: **Mods → Add a native mod from GitHub**, and paste:

```
https://github.com/cinovenx/e4all-horizon
```

The launcher reads the release's `.hnmod`, builds it into the game, and asks
before running native code (mods run with your account's rights — read the
source first, it is all here).

Native mods are always on in Horizon: there is no switch. The mod's own row in
Options → Mods is its settings page, not a way to turn it off — use
`host_enabled = false` in the config file to stop it publishing addresses.
That switch does not affect `offline_mode`, which is a separate decision, as
described above.

## Building it yourself

The mod is a bundle of a crate and a `horizon-mixin.toml`; the launcher builds
it. To build the crate on its own (and run its tests):

```sh
cd e4all/mixins/e4all/core
cargo test
```

Two of its tests need the network and are skipped by default:

```sh
cargo test -- --ignored
```

* `the_live_relay_gives_a_domain` — the broker names a relay, the QUIC
  handshake with ALPN `quiclime` succeeds and the relay hands out a domain.
* `a_minecraft_client_reaches_the_host_through_the_relay` — the whole way a
  guest takes: the domain is dialled on the relay's Minecraft port, the
  handshake arrives at the host's game port unchanged, and the host's answer
  comes back through the relay.

> `cargo test` writes a `Cargo.lock` into the crate's folder. A bundle crate
> may not carry one; `tools/package.py` leaves it out of the archive, so it is
> enough to keep it out of git.

To pack the `.hnmod` and check its pins, module names and contents without a
Horizon checkout:

```sh
python tools/package.py
python tools/check.py
```

`tools/check.py` also compares the archive with its sources byte for byte, so a
stale `.hnmod` is caught before it is published. The remaining checks need
Horizon; they are described in [VALIDATION.md](VALIDATION.md).

To see what the bundle does to Horizon's source, `horizon-mixins` has a `plan`
command. `--mods` takes the directory that holds the mod folders, so from this
repository it is `e4all/mixins`:

```sh
horizon-mixins plan \
  --workspace /path/to/horizon \
  --mods e4all/mixins \
  --out plan.json
```

`apply` writes the transformed source to a separate directory and leaves
Horizon's own tree untouched:

```sh
horizon-mixins apply \
  --workspace /path/to/horizon \
  --mods e4all/mixins \
  --out /path/to/staged
```

## Which Horizon it builds on

`horizon-native.json` names the hook points' version the mod needs:

```json
"hooks": 411
```

The launcher reads it and builds the mod on any Horizon whose
`NATIVE_HOOKS_API` is **411 or later** — its own rule, and the one that
matters at install time. `horizon` in the same file is the revision the mod
was tested on; the launcher rewrites the bundle's
`[compatibility] horizon_revision` to the revision it builds on, so the mod
follows the player's Horizon rather than pinning one.

Two revisions are tested here, and both build:

| Horizon | Hook points | |
| --- | --- | --- |
| `fdc87aecdbd116bf1e08adcd062fcbaea8c2e2ca` | 411 | the floor: the revision the mod was tested on |
| `1a62ef7eb8cf730cb190c4d7fa5e41654e90e90d` | 415 | a later one |

Every hook point the mod uses — the eight in `game/native_hooks.rs`, plus
`Integrated::set_paused` and `auth::current` — exists at both revisions with
the same signature, and Horizon only ever *adds* hook points, so 411 is a
sound floor.

If the launcher says *"Horizon … doesn't have the native hook points these
mods use: e4all (hook points v415)"*, the bundle is an older one that
declared 415; the current one declares 411 and builds on that Horizon.

## What has been checked

On 2026-11-14, on both revisions above, the way the launcher does it:

* **The bundle is a bundle.** Horizon's own
  `horizon_mods::index::native::Bundle::read` opens `e4all-0.1.0.hnmod`, reads
  its manifest (`hooks` is 411) and all 25 files.
* **It applies.** `horizon-mixins plan` and `apply` report `status=ok` with
  one crate, two modules and ten injections, and `apply` writes every one of
  them into the staged source.
* **It coexists with e4mc.** Applied together with the published e4mc 0.1.2 and
  both repinned to 415, the two mods plan and apply (`status=ok`, 2 crates,
  4 modules, 19 injections) and the combined game compiles. Their module names
  do not collide, and both `RETURN` injections on `auth::current` stack.
* **The game still compiles.** The staged tree — the real Horizon checkout
  with the four transformed files — passes `cargo check --locked` for
  `horizon-client`, `horizon-client-render` and `e4all_core`. Nothing in e4all
  or in the files it transforms warns; the 411 checkout has two pre-existing
  warnings elsewhere (`horizon-protocol` and `horizon-friends`), and the 415
  checkout has none. The only difference between the staged tree and the
  pristine checkout is those four files and the mod's own crate.
* **The relay still works.** The core's 34 tests pass, and the two that need
  the network pass too: the broker names a relay, the QUIC handshake succeeds
  and a real Minecraft client reaches the host through the relay.
* **The archive matches its sources.** `python tools/check.py` reports 25
  files and the same SHA-256 as the published file, and every packed file is
  byte-identical to the source it came from.

The game itself was not launched, and no screenshots were taken: the session
was verified by compiling it, not by playing it. See
[VALIDATION.md](VALIDATION.md) for the exact commands and results.

## License

MIT, like e4all. See [LICENSE](LICENSE) and [NOTICE](NOTICE).

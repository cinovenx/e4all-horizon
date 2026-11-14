# Validation

What was checked before publishing e4all 0.1.1 for Horizon, on what, and what
was not. Everything below was run on 2026-11-14.

## What was tested on

| | |
| --- | --- |
| OS | Windows 11 Pro, 10.0.26300, x86-64 |
| Rust, the checks | `rustc 1.95.0 (59807616e 2026-04-14)`, `cargo 1.95.0 (f2d3ce0bd 2026-03-21)` |
| Rust, the game's build | the launcher's own toolchain, Rust 1.99.1, `x86_64-pc-windows-gnu`, `--profile fast` |
| Python | 3.11.9 |
| `horizon-mixins` | 0.6.0 |
| Horizon, the floor | `fdc87aecdbd116bf1e08adcd062fcbaea8c2e2ca`, hook points 411 |
| Horizon, a later one | `1a62ef7eb8cf730cb190c4d7fa5e41654e90e90d`, hook points 415 |
| Minecraft | 26.3 |

The pins are the ones in `compatibility.json` and both
`horizon-native.json` files.

0.1.1 touches two helper files (the settings page and its click handler) and
the version strings; the relay crate, the runtime module and the injections are
unchanged from 0.1.0. The checks below were therefore run once more on the
0.1.1 sources where the bundle is affected, and are otherwise the 0.1.0
results for code that did not change.

## The relay crate

Run in `e4all/mixins/e4all/core`:

```sh
cargo +1.95.0 fmt --check
cargo +1.95.0 clippy --all-targets -- -D warnings
cargo +1.95.0 test
cargo +1.95.0 test -- --ignored
```

Results:

* `cargo fmt --check` — clean.
* `cargo clippy --all-targets -- -D warnings` — clean, exit code 0.
* `cargo test` — **34 passed, 0 failed, 2 ignored**; doc-tests 0.
* `cargo test -- --ignored` — **2 passed, 0 failed**: the broker names a relay,
  the QUIC handshake with ALPN `quiclime` succeeds and a domain is handed out;
  a real Minecraft client reaches the host through the relay and the host's
  answer comes back.

`cargo test` writes `Cargo.lock` and `target/` into the crate folder. Both were
removed after the run; the archive does not carry them.

## The archive

```sh
python tools/package.py
python tools/check.py
```

Result:

```text
e4all-0.1.1.hnmod: 32960 bytes,
SHA-256 b625afc90f37e54a982446e60af746653a8020081b9db01b30ea0e2aa70a9cd0,
25 files
OK
```

`tools/check.py` checks the pins, the two module names, the crate and runtime
versions, the page metadata, every archive path, the fixed timestamps, the
absence of `Cargo.lock`, and that every packed file is byte-identical to its
source.

The same archive was opened with Horizon's own reader,
`horizon_mods::index::native::Bundle::read`, built from `fdc87ae`. It reported
`read ok`, the manifest with `version: "0.1.1"` and `hooks: Some(411)`, and all
25 source files. Packing twice from the same sources gives the same bytes.

## Applying it to Horizon

For each revision, a copy of the bundle was pinned to that revision the way the
launcher does (`repin`), `plan` and `apply` were run against a clean checkout,
and the staged tree was compiled:

```sh
horizon-mixins plan  --workspace <horizon> --mods <mods> --out plan.json
horizon-mixins apply --workspace <horizon> --mods <mods> --out <stage>
cargo +1.95.0 check --locked -p horizon-client -p horizon-client-render -p e4all_core
```

Both revisions gave the same `plan` and `apply` result:
`status=ok`, 1 crate, 2 modules, 10 injections.

| Horizon | `cargo check` | Warnings |
| --- | --- | --- |
| `fdc87ae` (411) | passed in 1m50s | 2, both pre-existing: `horizon-protocol` and `horizon-friends` |
| `1a62ef7` (415) | passed in 1m20s | none |

Neither warning is in e4all or in the four Horizon files the bundle transforms.
The only difference between a staged tree and the pristine checkout is those
four files and the mod's own crate.

The 411 stage was also built the way the launcher builds a player's game:

```sh
cargo build --locked --profile fast -p horizon-client --bin horizon-client
```

It finished in 5m07s, with the launcher's own Rust 1.99.1 toolchain (the check
above uses 1.95.0; the launcher ships 1.99.1), producing
`target/fast/horizon-client.exe` — 135 580 821 bytes. That binary is the one
the game run below used.

## In the game

The staged build was launched in a window with its own game and home folders,
so the player's world and settings were not touched.

* `0.1.0`: a world was opened, `/e4all start` opened the LAN itself and the
  relay answered with `decathlon-unlined.de.e4mc.link`; the address was in chat.
  A guest joined and played under the name `oxynos`. The screenshot of that
  session is `2026-11-14_20.20.32.png` in the launcher's `screenshots` folder.
* The host was started **without a Microsoft account** (a manual launch, no
  session file), so Horizon hosted the LAN offline: `OnlineMode::lan()` answered
  `None` whatever `offline_mode` said. The settings page showed the offline
  switch anyway, and switching it changed nothing — the one problem found in
  that run. 0.1.1 is the fix: the switch is shown only when the game has an
  account, and it is on the page as before when there is one.
* `0.1.1`: rebuilt with the change, launched the same way. `/e4all start`
  published an address, a guest joined, and **Options → Mods → e4all** showed
  the three switches that apply to an unsigned host — *Share this world over
  the relay*, *Keep the world ticking while hosting* and *Hide the address in
  chat* — with the offline switch left out, as intended.

## Together with the published e4mc

The registry builds every native mod together, so e4all was also applied
alongside the published `e4mc` 0.1.2 bundle, both repinned to `1a62ef7`. The
names the two mods insert are `e4all`, `e4all_auth_suppress`, `e4mc` and
`auth_suppress`; no module or injection collides, including the two `RETURN`
injections on `horizon_client_render::auth::current`.

* `horizon-mixins plan` and `apply`: `status=ok`, 2 crates, 4 modules,
  19 injections.
* `cargo check --locked -p horizon-client -p horizon-client-render
  -p e4all_core -p e4mc_bridge`: passed in 1m06s.
* The four warnings in that build are all in e4mc's generated module
  (`e4mc::start`, `e4mc::tick`, `e4mc::command`, `e4mc::mine`); e4all adds
  none.

This run was made with the 0.1.0 bundle. 0.1.1 adds no module, no injection
and no crate, and the two helper files it changes are inlined into e4all's own
generated module, so the combined names and injections are the same.

## Not verified

* **An online-mode LAN was not exercised.** There was no Microsoft account to
  start the game with, so the branch where the host's account authenticates a
  guest was not played through, and the offline switch was not seen doing its
  job online. Its code path is compiled and unit-tested, and the switch is
  hidden exactly where it cannot act.
* **The combined build with e4mc was not re-run for 0.1.1.** It passed for
  0.1.0, and 0.1.1 changes neither the module names nor the injections.
* **One guest, one machine, one network.** `oxynos` joined from another
  computer through the relay; nothing beyond that was load-tested.

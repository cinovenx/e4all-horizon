# Validation

What was checked before publishing e4all 0.1.0 for Horizon, on what, and what
was not. Everything below was run on 2026-11-14.

## What was tested on

| | |
| --- | --- |
| OS | Windows 11 Pro, 10.0.26300, x86-64 |
| Rust | `rustc 1.95.0 (59807616e 2026-04-14)`, `cargo 1.95.0 (f2d3ce0bd 2026-03-21)` |
| Python | 3.11.9 |
| `horizon-mixins` | 0.6.0 |
| Horizon, the floor | `fdc87aecdbd116bf1e08adcd062fcbaea8c2e2ca`, hook points 411 |
| Horizon, a later one | `1a62ef7eb8cf730cb190c4d7fa5e41654e90e90d`, hook points 415 |
| Minecraft | 26.3 |

The pins are the ones in `compatibility.json` and both
`horizon-native.json` files.

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
e4all-0.1.0.hnmod: 32567 bytes,
SHA-256 445e6d137db012d93d1e08adde73e4f90022f5a18157026059bc2e129b03677b,
25 files
OK
```

`tools/check.py` checks the pins, the two module names, the crate and runtime
versions, the page metadata, every archive path, the fixed timestamps, the
absence of `Cargo.lock`, and that every packed file is byte-identical to its
source.

The same archive was opened with Horizon's own reader,
`horizon_mods::index::native::Bundle::read`, built from `fdc87ae`. It reported
`read ok`, the manifest with `hooks: Some(411)`, and all 25 source files.

A fresh clone of <https://github.com/cinovenx/e4all-horizon> packs to the same
32 567 bytes and the same SHA-256, and the asset attached to release `v0.1.0`
hashes the same too.

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
| `1a62ef7` (415) | passed in 1m39s | none |

Neither warning is in e4all or in the four Horizon files the bundle transforms.
The only difference between a staged tree and the pristine checkout is those
four files and the mod's own crate.

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

## Not verified

* **The game was not launched.** No title screen, world, LAN session or relay
  address was exercised interactively, and no screenshots were taken. This run
  verified that the bundle applies and that the game compiles with it, not that
  the in-game UI and an end-to-end guest join work on a player's machine.
* **No live guest joined a Horizon world.** The relay itself was exercised by
  the core's two live tests above, but not through the game.
* **Offline mode was not exercised in game.** Its code path is compiled and
  unit-tested, but no guest without a Microsoft account was actually admitted.

The in-game path is the natural next step for a player who has prepared 26.3
resources: open a world, run `/e4all start`, join the address from another
client, then try a guest with `offline_mode = true`.

# Changelog

## 0.1.1 — 2026-11-14

- **The offline-mode switch is shown only when the game has a signed-in
  account.** Horizon decides a LAN world's online mode from the host's own
  session (`OnlineMode::lan`): a game started without one hosts offline, so
  the switch could not change anything and looked broken — offline guests
  joined with it off. Without an account, guests join offline regardless and
  the page now leaves the switch out; with an account it appears as before.
  `offline_mode` still works from `config/e4all.json`.

Verified on Windows: the bundle applies and the game compiles on Horizon
`fdc87ae` (hook points 411) and `1a62ef7` (415), and the launcher's own build
of the 411 stage was run in the game — `/e4all start` published an address, a
guest joined and played, and the settings page showed the three switches that
apply without an account. Not verified: an online-mode LAN with a signed-in
host. See `VALIDATION.md`.

## 0.1.0 — 2026-11-14

First release. The host side of e4all for Horizon, as a native mod.

- **`/e4all start`** opens the LAN if needed and asks the e4mc relay for a
  public domain; `/e4all stop` closes the address and keeps it closed, and
  `/e4all status` and `/e4all domain` show what it is doing.
- **Opening the LAN by hand works too:** the mod notices a LAN world and
  publishes an address for it.
- **Offline guests, opt-in:** with `offline_mode`, the LAN runs without Mojang
  authentication while it is hosted, so a friend without an account can join.
  It is off by default, and while it is on it applies to every LAN world, as in
  e4all's Java mod.
- **Options → Mods row and settings page:** share over the relay, keep the
  world ticking while hosting, offline mode, and hide the address in chat.
- **A hosted world keeps ticking** while the pause menu is open, so guests are
  not frozen.
- **The relay session recycles** when three keepalive probes go unanswered, and
  the address is copied to chat with one click.

Verified on Windows with Rust 1.95.0: the relay crate's 34 tests and its
two live tests against the real e4mc relay pass on 2026-11-14. The bundle
applies and the game compiles on Horizon `fdc87ae` (hook points 411) and
`1a62ef7` (415), alone and together with the published e4mc 0.1.2. It is
pinned to `fdc87ae` and horizon-mixins `e59da90`, and declares hook points 411
so the launcher can build it on later Nightlies. 0.1.0 was later run in the
game; that session is what produced 0.1.1. See `VALIDATION.md`.

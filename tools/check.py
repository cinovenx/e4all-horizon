#!/usr/bin/env python3
"""Check e4all's pins, names and (when it has been packed) its `.hnmod`.

This is the local, offline part of the registry's checks. The registry
additionally applies the bundle to the newest Horizon Nightly and compiles the
game with it; that needs a Horizon checkout and is described in VALIDATION.md.

Run it after editing a pin, a version or a module name:

    python tools/package.py
    python tools/check.py
"""
import hashlib
import json
import re
from pathlib import Path
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "e4all"
BUNDLE = PACKAGE / "mixins" / "e4all"
COMMIT = re.compile(r"[0-9a-f]{40}")
ID = re.compile(r"[a-z0-9_]{1,64}")
RUNTIME_MODULES = {"e4all", "e4all_auth_suppress"}


def fail(message):
    raise SystemExit(message)


def in_data(rest):
    """A file under the bundle's `data/`, or under a crate's top-level `data/`."""
    parts = rest.split("/")
    return len(parts) >= 2 and (parts[0] == "data" or (len(parts) >= 3 and parts[1] == "data"))


def text(rest):
    """A text file a crate folder usually carries, in the bundle's folder or a crate's."""
    parts = rest.split("/")
    return len(parts) <= 2 and (
        any(parts[-1].startswith(prefix) for prefix in ("README", "LICENSE", "LICENCE", "COPYING", "NOTICE"))
        or parts[-1] == ".gitignore"
    )


def allowed(rest):
    """Whether the bundle may carry the file `rest` (its path under `mixins/<id>/`)."""
    return rest.endswith(".rs") or rest.endswith(".toml") or in_data(rest) or text(rest)


def load():
    compat = json.loads((ROOT / "compatibility.json").read_text())
    mixin = tomllib.loads((BUNDLE / "horizon-mixin.toml").read_text())
    mod = mixin["mod"]

    if not ID.fullmatch(mod["id"]):
        fail("the id must be [a-z0-9_]{1,64}")
    if not mod["version"]:
        fail("horizon-mixin.toml needs a version")
    if mixin.get("compatibility", {}).get("horizon_revision") != compat.get("horizon_revision"):
        fail("horizon-mixin.toml and compatibility.json pin different Horizon revisions")
    for key in ("horizon_revision", "horizon_mixins_revision"):
        if not COMMIT.fullmatch(compat.get(key, "")):
            fail(f"compatibility.json {key} must be a 40-character lowercase commit")
    if not compat.get("game"):
        fail("compatibility.json needs the Minecraft version in `game`")
    if not isinstance(compat.get("hooks"), int) or compat["hooks"] < 1:
        fail("compatibility.json needs the native hook points' version in `hooks`")

    manifest = json.loads((ROOT / "horizon-native.json").read_text())
    expected = dict(
        schema=1,
        id=mod["id"],
        version=mod["version"],
        game=compat["game"],
        horizon=compat["horizon_revision"],
        mixins=compat["horizon_mixins_revision"],
        hooks=compat["hooks"],
    )
    if manifest != expected:
        fail("horizon-native.json does not match compatibility.json and horizon-mixin.toml")
    packed = json.loads((PACKAGE / "horizon-native.json").read_text())
    if packed != expected:
        fail("e4all/horizon-native.json does not match the repository manifest")

    runtime = (BUNDLE / "runtime.rs").read_text()
    found = re.search(r'pub const VERSION: &str = "([^"]*)";', runtime)
    if not found or found.group(1) != mod["version"]:
        fail("runtime.rs VERSION does not match horizon-mixin.toml")
    core = tomllib.loads((BUNDLE / "core" / "Cargo.toml").read_text())
    if core["package"]["version"] != mod["version"]:
        fail("the relay crate's version does not match horizon-mixin.toml")

    modules = {module["name"]: module for module in mixin.get("module", [])}
    for name in modules:
        if not name.startswith(mod["id"]):
            fail(f"module `{name}` must be named after the mod (`{mod['id']}_...`): "
                 "the registry builds every native mod together, and support modules "
                 "with one name in one file cannot both be inserted")
    if set(modules) != RUNTIME_MODULES:
        fail(f"expected modules {sorted(RUNTIME_MODULES)}, found {sorted(modules)}")
    for module in modules.values():
        if not (BUNDLE / module["source"]).is_file():
            fail(f"module `{module['name']}` names a missing source: {module['source']}")
    for injection in mixin.get("injection", []):
        if not (BUNDLE / injection["helper"]).is_file():
            fail(f"injection `{injection['id']}` names a missing helper: {injection['helper']}")
    for path in sorted(BUNDLE.rglob("*.rs")) + sorted(BUNDLE.rglob("*.toml")):
        text = path.read_text()
        for other in re.findall(r"crate::auth::([a-z_][a-z0-9_]*)", text):
            if other not in modules:
                fail(f"{path.relative_to(ROOT)} names crate::auth::{other}, which no module inserts")

    page = ROOT / "horizon-registry.toml"
    if page.is_file():
        entry = tomllib.loads(page.read_text())
        unknown = set(entry) - {"name", "summary", "description", "authors", "license"}
        if unknown:
            fail(f"horizon-registry.toml has unknown keys: {sorted(unknown)}")
        if not 0 < len(entry.get("summary", "")) <= 160:
            fail("horizon-registry.toml summary must be 1-160 characters")
        if not 50 <= len(entry.get("description", "")) <= 10_000:
            fail("horizon-registry.toml description must be 50-10000 characters")
        if entry.get("license") != "MIT":
            fail("horizon-registry.toml must name the repository's licence")

    return compat, mixin


def archive(compat, mixin):
    mod = mixin["mod"]
    out = ROOT / f"{mod['id']}-{mod['version']}.hnmod"
    if not out.is_file():
        print(f"no archive yet: run python tools/package.py ({out.name})")
        return

    with zipfile.ZipFile(out) as package:
        names = package.namelist()
        if len(names) != len(set(names)):
            fail("the archive holds a path twice")
        for name in names:
            parts = name.split("/")
            if name.startswith("/") or "\\" in name or ":" in name or "" in parts or "." in parts or ".." in parts:
                fail(f"bad archive path: {name}")
            info = package.getinfo(name)
            if info.date_time != (2026, 1, 1, 0, 0, 0):
                fail(f"not reproducible: {name} has a timestamp")
            if (info.external_attr >> 16) & 0o170000 != 0o100000:
                fail(f"not a regular file: {name}")
            if info.filename.endswith("Cargo.lock"):
                fail("a bundle crate may not carry Cargo.lock")

        manifest = json.loads(package.read("horizon-native.json"))
        manifest_fields = {"schema", "id", "version", "game", "horizon", "mixins", "hooks"}
        if set(manifest) - manifest_fields:
            fail(f"horizon-native.json has unknown fields: {sorted(set(manifest) - manifest_fields)}")
        if manifest != json.loads((ROOT / "horizon-native.json").read_text()):
            fail("the packed horizon-native.json is stale: rerun tools/package.py")

        prefix = f"mixins/{mod['id']}/"
        if prefix + "horizon-mixin.toml" not in names:
            fail("the archive lacks its horizon-mixin.toml")
        for name in names:
            if name == "horizon-native.json":
                continue
            if not name.startswith(prefix):
                fail(f"unexpected top-level entry: {name}")
            rest = name[len(prefix):]
            if not allowed(rest):
                fail(f"the registry does not accept {name}")
            source = BUNDLE / rest
            if not source.is_file() or package.read(name) != source.read_bytes():
                fail(f"the archive's {name} differs from its source: rerun tools/package.py")

    digest = hashlib.sha256(out.read_bytes()).hexdigest()
    print(f"{out.name}: {out.stat().st_size} bytes, SHA-256 {digest}, {len(names)} files")


def main():
    compat, mixin = load()
    mod = mixin["mod"]
    print(f"{mod['id']} {mod['version']} on Horizon {compat['horizon_revision'][:7]} "
          f"+ horizon-mixins {compat['horizon_mixins_revision'][:7]} "
          f"(game {compat['game']}, hook points {compat['hooks']})")
    archive(compat, mixin)
    print("OK")


if __name__ == "__main__":
    main()

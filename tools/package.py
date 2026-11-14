#!/usr/bin/env python3
"""Package the e4all bundle as a `.hnmod`, the file the launcher and registry take.

The archive holds `horizon-native.json` and the bundle under `mixins/e4all/`:
every `.rs` and `.toml` file (including the relay crate's sources), the text
files a crate folder usually carries (`README*`, `LICENSE*`, `COPYING*`,
`NOTICE*` and `.gitignore`, in the bundle's folder or a crate's), and nothing
else. Fixed timestamps and modes keep the bytes stable, so the same sources
give the same SHA-256.

The manifest is built from `compatibility.json` and `horizon-mixin.toml`; the
`horizon-native.json` in the repository is checked against it by
`tools/check.py`, rather than trusted here.
"""
import hashlib
import json
import re
from pathlib import Path
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
BUNDLE = ROOT / "e4all" / "mixins" / "e4all"
COMMIT = re.compile(r"[0-9a-f]{40}")
ID = re.compile(r"[a-z0-9_]{1,64}")


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


def main():
    compat = json.loads((ROOT / "compatibility.json").read_text())
    if not (BUNDLE / "horizon-mixin.toml").is_file():
        raise SystemExit(f"no bundle at {BUNDLE.relative_to(ROOT)}")
    mixin = tomllib.loads((BUNDLE / "horizon-mixin.toml").read_text())
    mod = mixin["mod"]

    if not ID.fullmatch(mod["id"]):
        raise SystemExit("the id must be [a-z0-9_]{1,64}")
    if mixin.get("compatibility", {}).get("horizon_revision") != compat.get("horizon_revision"):
        raise SystemExit("horizon-mixin.toml and compatibility.json pin different Horizon revisions")
    for key in ("horizon_revision", "horizon_mixins_revision"):
        if not COMMIT.fullmatch(compat.get(key, "")):
            raise SystemExit(f"compatibility.json {key} must be a 40-character lowercase commit")

    manifest = {
        "schema": 1,
        "id": mod["id"],
        "version": mod["version"],
        "game": compat["game"],
        "horizon": compat["horizon_revision"],
        "mixins": compat["horizon_mixins_revision"],
    }
    if "hooks" in compat:
        manifest["hooks"] = compat["hooks"]

    files = {
        "horizon-native.json": (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode()
    }
    for path in sorted(BUNDLE.rglob("*")):
        if not path.is_file():
            continue
        relative = path.relative_to(BUNDLE).as_posix()
        if not allowed(relative):
            raise SystemExit(f"the bundle may not carry {relative}")
        files[f"mixins/{mod['id']}/{relative}"] = path.read_bytes()

    out = ROOT / f"{mod['id']}-{mod['version']}.hnmod"
    with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, content in sorted(files.items()):
            info = zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            archive.writestr(info, content)

    print(json.dumps(dict(
        file=str(out),
        size=out.stat().st_size,
        sha256=hashlib.sha256(out.read_bytes()).hexdigest(),
        files=len(files),
    ), indent=2))


if __name__ == "__main__":
    main()

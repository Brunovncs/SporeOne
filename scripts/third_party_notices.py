"""Writes THIRD_PARTY_NOTICES.md: every crate compiled into sporeone.exe, with its version and
license.
Run it when dependencies change:

    python scripts/third_party_notices.py
"""

import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PROGRAMS = ("sporeone",)
# Every target a release is built for.
TARGETS = ("x86_64-pc-windows-msvc",)

HEADER = """# Third-party notices

SporeOne is MIT licensed (see [LICENSE](LICENSE)). `sporeone.exe` includes the Rust crates below,
each under its own license. Nothing of the Spore ModAPI Launcher Kit is included: SporeOne writes
the files it reads, and downloads its setup from its GitHub release when you ask it to. Mods are
downloaded from their authors' releases and are not part of SporeOne.

The text box is adapted from the `input` example of [GPUI](https://github.com/zed-industries/zed),
under the Apache License 2.0, as GPUI itself is.

## Rust crates

Each crate's license text is in its source, at the repository listed. Where a crate offers a
choice of licenses, SporeOne uses it under the first one that applies.

| Crate | Version | License |
|---|---|---|
"""


def linked(target: str) -> tuple[dict, set, set]:
    """The packages, the workspace members and what the program links when built for `target`,
    following normal dependencies only (no build scripts, no tests)."""
    meta = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", target],
            cwd=ROOT,
            check=True,
            capture_output=True,
        ).stdout
    )
    packages = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    members = set(meta["workspace_members"])
    todo = [i for i in members if packages[i]["name"] in PROGRAMS]
    seen = set()
    while todo:
        i = todo.pop()
        if i in seen:
            continue
        seen.add(i)
        for dep in nodes[i]["deps"]:
            if any(k["kind"] is None for k in dep["dep_kinds"]):
                todo.append(dep["pkg"])
    return packages, members, seen


def main() -> None:
    # Every archive ships the same notices, so they cover what any of the builds links.
    packages, members, seen = {}, set(), set()
    for target in TARGETS:
        p, m, s = linked(target)
        packages |= p
        members |= m
        seen |= s
    rows = sorted(
        {
            (packages[i]["name"], packages[i]["version"], packages[i].get("license") or "see repository", packages[i].get("repository") or "")
            for i in seen - members
        }
    )
    lines = []
    for name, version, license, repo in rows:
        crate = f"[{name}]({repo})" if repo else name
        lines.append(f"| {crate} | {version} | {license.replace('/', ' OR ')} |")
    (ROOT / "THIRD_PARTY_NOTICES.md").write_text(HEADER + "\n".join(lines) + "\n", encoding="utf-8", newline="\n")
    print(f"{len(rows)} crates")


if __name__ == "__main__":
    main()

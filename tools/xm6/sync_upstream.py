#!/usr/bin/env python3
"""Move this fork's feature branch onto a new upstream release.

The fork keeps one feature (the XM6 spatial engine) over
SeiyaFunaokaJP/A2DP-Windows-Bridge. This script is the only supported way to
take a new upstream tag: it merges the tag into feat/xm6 on a short-lived
branch, resolves the mechanical conflict classes (language JSON, generated
ts-rs bindings), runs the gates, commits and prints the release tag.

Usage:
    python tools/xm6/sync_upstream.py                 # newest upstream tag
    python tools/xm6/sync_upstream.py --tag v2.0.2-beta.11
    python tools/xm6/sync_upstream.py --dry-run       # report, change nothing
    python tools/xm6/sync_upstream.py --fix-lang      # re-run the language merge only
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

FEATURE_BRANCH = "feat/xm6"
UPSTREAM_REMOTE = "upstream"
UPSTREAM_REPO = "SeiyaFunaokaJP/A2DP-Windows-Bridge"
STATE_FILE = Path("tools/xm6/STATE.json")
LANG_GLOB = "app/lang/*.json"
BINDINGS_DIR = "app/src/ui/src/ipc/bindings"
CARGO = ["cargo", "+stable-x86_64-pc-windows-gnu"]


class SyncError(RuntimeError):
    pass


def run(args: list[str], *, check: bool = True, capture: bool = False) -> subprocess.CompletedProcess:
    printable = " ".join(args)
    print(f"$ {printable}", flush=True)
    return subprocess.run(
        args,
        check=check,
        text=True,
        capture_output=True if capture else False,
    )


def git(*args: str, check: bool = True, capture: bool = False) -> subprocess.CompletedProcess:
    return run(["git", *args], check=check, capture=capture)


def git_out(*args: str) -> str:
    return git(*args, capture=True).stdout.strip()


def repo_root() -> Path:
    out = git_out("rev-parse", "--show-toplevel")
    if not out:
        raise SyncError("not inside a git repository")
    return Path(out)


def version_key(tag: str) -> tuple:
    """Sort v2.0.2-beta.10 after v2.0.2-beta.9 (git's own sort is lexical)."""
    parts = re.split(r"[.\-+]", tag.lstrip("v"))
    key: list[tuple[int, object]] = []
    for part in parts:
        key.append((0, int(part)) if part.isdigit() else (1, part))
    return tuple(key)


def upstream_tags() -> list[str]:
    out = git_out("ls-remote", "--tags", UPSTREAM_REMOTE, "v*")
    tags = []
    for line in out.splitlines():
        ref = line.split("refs/tags/")[-1]
        if ref.endswith("^{}"):
            continue
        if re.fullmatch(r"v\d+\.\d+\.\d+(?:-beta\.\d+)?", ref):
            tags.append(ref)
    return sorted(set(tags), key=version_key)


def ensure_remote(root: Path) -> None:
    remotes = git_out("remote").split()
    if UPSTREAM_REMOTE not in remotes:
        git("remote", "add", UPSTREAM_REMOTE, f"https://github.com/{UPSTREAM_REPO}.git")


def unmerged_files() -> list[str]:
    out = git_out("diff", "--name-only", "--diff-filter=U")
    return [line for line in out.splitlines() if line.strip()]


def stage_blob(path: str, stage: int) -> str | None:
    """Return the content of a merge stage (2 = ours, 3 = theirs), or None."""
    proc = git("show", f":{stage}:{path}", check=False, capture=True)
    return proc.stdout if proc.returncode == 0 else None


def merge_language_file(path: str) -> bool:
    """Three-way, key by key: a value upstream changed and we did not takes
    upstream's; a value we changed keeps ours; a real clash keeps ours."""
    base_raw = stage_blob(path, 1)
    ours_raw = stage_blob(path, 2)
    theirs_raw = stage_blob(path, 3)
    if ours_raw is None or theirs_raw is None:
        return False
    try:
        ours = json.loads(ours_raw)
        theirs = json.loads(theirs_raw)
        base = json.loads(base_raw) if base_raw else {}
    except json.JSONDecodeError:
        return False
    if not isinstance(ours, dict) or not isinstance(theirs, dict):
        return False

    merged = {}
    for key, their_value in theirs.items():
        if key in ours:
            if ours[key] == base.get(key):
                merged[key] = their_value          # untouched here: upstream's value
            else:
                merged[key] = ours[key]            # ours, changed or not
        else:
            merged[key] = their_value              # upstream only
    for key, our_value in ours.items():
        merged.setdefault(key, our_value)          # ours only

    text = json.dumps(merged, ensure_ascii=False, indent=2) + "\n"
    Path(path).write_text(text, encoding="utf-8", newline="\n")
    git("add", path)
    print(f"  merged {path}: {len(merged)} keys")
    return True


def fix_bindings() -> bool:
    """Generated ts-rs files: regenerate rather than merge as text."""
    proc = run([*CARGO, "test", "-p", "ipc", "export_bindings"], check=False)
    if proc.returncode != 0:
        print("  bindings regeneration failed; resolve the Rust protocol first")
        return False
    git("add", BINDINGS_DIR)
    print(f"  regenerated {BINDINGS_DIR}")
    return True


def resolve_mechanical(root: Path, *, do_bindings: bool = True) -> list[str]:
    files = unmerged_files()
    for path in files:
        if re.fullmatch(r"app/lang/[^/]+\.json", path):
            merge_language_file(path)
    remaining = unmerged_files()
    if do_bindings and any(p.startswith(BINDINGS_DIR) for p in remaining):
        # Take upstream's generated files first: the regeneration overwrites them.
        for path in [p for p in remaining if p.startswith(BINDINGS_DIR)]:
            git("checkout", f"--theirs", "--", path)
            git("add", path)
        fix_bindings()
    return unmerged_files()


def gates(root: Path) -> bool:
    ok = True
    checks: list[tuple[str, list[str], str]] = [
        ("rust check", [*CARGO, "check", "--workspace"], str(root)),
        ("tsc", ["pnpm", "exec", "tsc", "--noEmit"], str(root / "app/src/ui")),
        ("vitest", ["pnpm", "exec", "vitest", "run"], str(root / "app/src/ui")),
        ("lang", [sys.executable, "tools/check_lang.py"], str(root)),
    ]
    for name, cmd, cwd in checks:
        print(f"\n=== gate: {name}", flush=True)
        proc = subprocess.run(cmd, cwd=cwd)
        if proc.returncode != 0:
            print(f"gate failed: {name}")
            ok = False
    return ok


def write_state(root: Path, tag: str, commit: str) -> None:
    path = root / STATE_FILE
    state = {}
    if path.exists():
        try:
            state = json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            state = {}
    state["upstream_tag"] = tag
    state["feature_commit"] = commit
    state.setdefault("stage_layout", 1)
    path.write_text(json.dumps(state, indent=2) + "\n", encoding="utf-8")
    git("add", str(STATE_FILE))


def next_release_tag(tag: str) -> str:
    base = f"{tag}-xm6."
    existing = [t for t in git_out("tag", "--list", f"{base}*").splitlines() if t]
    numbers = [int(t.rsplit(".", 1)[-1]) for t in existing if t.rsplit(".", 1)[-1].isdigit()]
    return f"{base}{max(numbers, default=0) + 1}"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", help="upstream tag to merge (default: newest v* tag)")
    parser.add_argument("--branch", default=FEATURE_BRANCH, help="feature branch to merge into")
    parser.add_argument("--dry-run", action="store_true", help="report conflicts, change nothing")
    parser.add_argument("--no-gates", action="store_true", help="skip cargo/tsc/vitest")
    parser.add_argument("--no-bindings", action="store_true", help="do not regenerate ts-rs bindings")
    parser.add_argument("--fix-lang", action="store_true", help="only merge conflicted language files")
    parser.add_argument("--keep", action="store_true", help="leave the merge branch checked out")
    parser.add_argument("--push", action="store_true", help="push the branch and the release tag")
    args = parser.parse_args()

    root = repo_root()
    print(f"repository: {root}")
    ensure_remote(root)

    if args.fix_lang:
        remaining = resolve_mechanical(root, do_bindings=False)
        print("remaining conflicts:", remaining or "none")
        return 0 if not remaining else 1

    dirty = git_out("status", "--porcelain")
    if dirty and not args.dry_run:
        print("working tree is not clean:\n" + dirty)
        return 2

    branch = git_out("rev-parse", "--abbrev-ref", "HEAD")
    if branch != args.branch and not args.dry_run:
        print(f"checkout {args.branch} first (currently on {branch})")
        return 2

    git("fetch", UPSTREAM_REMOTE, "--tags", "--prune")

    tag = args.tag or (upstream_tags() or [None])[-1]
    if not tag:
        print("no upstream tags found")
        return 2
    print(f"upstream tag: {tag}")

    if args.dry_run:
        base = git_out("merge-base", "HEAD", tag)
        print(f"merge base: {base[:12]}")
        proc = git("merge-tree", "--write-tree", "--name-only", "HEAD", tag, check=False, capture=True)
        conflicted = [line for line in proc.stdout.splitlines() if line and not re.fullmatch(r"[0-9a-f]{40}", line)]
        if proc.returncode == 0:
            print("dry run: merges cleanly")
            return 0
        print("dry run: would conflict in:")
        for line in conflicted:
            print(f"  {line}")
        return 0

    branch_name = f"upstream-sync/{tag}"
    existing = git_out("branch", "--list", branch_name)
    if existing:
        git("branch", "-D", branch_name)
    git("checkout", "-b", branch_name)

    merge = git("merge", "--no-ff", "--no-commit", tag, check=False)
    if merge.returncode != 0:
        remaining = resolve_mechanical(root, do_bindings=not args.no_bindings)
        if remaining:
            print("\nmanual conflicts remain:")
            for path in remaining:
                print(f"  {path}")
            print("\nresolve them, `git add`, then run this script again with --resume is not supported:")
            print("commit the merge yourself and update tools/xm6/STATE.json.")
            return 3

    if not args.no_gates and not gates(root):
        print("\ngates failed; fix, then `git commit` and tag manually.")
        return 4

    message = f"Merge upstream {tag} into the xm6 feature\n\nUpstream: {UPSTREAM_REPO} {tag}\nSync branch: {branch_name}\n"
    git("commit", "-m", message)
    commit = git_out("rev-parse", "--short", "HEAD")
    write_state(root, tag, commit)
    git("commit", "-m", f"Record the upstream source: {tag}")

    release = next_release_tag(tag)
    print(f"\nmerge commit: {commit}")
    print(f"next: git tag -a {release} -m \"xm6 feature on {tag}\"")
    print(f"      git checkout {args.branch} && git merge --no-ff {branch_name}")

    if args.push:
        git("push", "-u", "origin", branch_name)
        git("tag", "-a", release, "-m", f"xm6 feature on {tag}")
        git("push", "origin", release)

    if not args.keep:
        git("checkout", args.branch)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except SyncError as error:
        print(f"error: {error}")
        sys.exit(1)

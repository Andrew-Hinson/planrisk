#!/usr/bin/env python3
"""Smoke test for a built planrisk binary, such as a release download.

Usage: tests/release/smoke_test.py PATH_TO_PLANRISK

Run from the repository root. Checks that the binary prints exactly what the
snapshot tests expect for every fixture in every format, and that exit codes,
stdin, config files and input errors behave as documented. Unlike
`cargo test`, it tests the binary as users get it, so it can run on each
release platform against the real release archives.
"""

import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "tests" / "fixtures"
SNAPSHOTS = ROOT / "tests" / "snapshots"

failures = []


def run(binary, *args, stdin=None):
    return subprocess.run(
        [binary, *args],
        input=stdin,
        capture_output=True,
        text=True,
        encoding="utf-8",
        cwd=ROOT,
    )


def check(name, ok, detail=""):
    print(("ok   " if ok else "FAIL ") + name)
    if not ok:
        failures.append(name)
        if detail:
            print("     " + detail.replace("\n", "\n     "))


def snapshot(name):
    """A snapshot's content, without insta's header."""
    text = (SNAPSHOTS / f"snapshots__{name}.snap").read_text(encoding="utf-8")
    return text.split("---\n", 2)[2]


def first_difference(expected, actual):
    for i, (e, a) in enumerate(zip(expected.splitlines(), actual.splitlines())):
        if e != a:
            return f"line {i + 1}:\n  expected: {e}\n  actual:   {a}"
    return f"lengths differ: expected {len(expected)} chars, got {len(actual)}"


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    binary = str(pathlib.Path(sys.argv[1]).resolve())
    # The first `version = "..."` in Cargo.toml is the package's. (Not
    # tomllib: it needs Python 3.11, and Ubuntu 22.04 runners have 3.10.)
    cargo_toml = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    version = re.search(r'^version = "([^"]+)"', cargo_toml, re.MULTILINE).group(1)

    result = run(binary, "--version")
    check("--version", result.stdout.strip() == f"planrisk {version}", result.stdout)

    # Every fixture, every format, against the snapshots. insta ignores
    # trailing newlines when comparing, so this does too.
    fixtures = sorted(FIXTURES.glob("*.json"))
    check("fixtures found", len(fixtures) > 0)
    for path in fixtures:
        for fmt in ["json", "markdown", "terminal"]:
            args = [str(path.relative_to(ROOT)), "--format", fmt, "--no-color"]
            result = run(binary, *args)
            actual = result.stdout
            if fmt == "json":
                actual = actual.replace(
                    f'"planrisk_version": "{version}"', '"planrisk_version": "[version]"'
                )
            expected = snapshot(f"{fmt}__{path.stem}")
            same = result.returncode == 0 and actual.rstrip("\n") == expected.rstrip("\n")
            detail = result.stderr or first_difference(expected, actual)
            check(f"{path.stem} --format {fmt}", same, detail)

    result = run(binary, "rules")
    check("rules", result.stdout.rstrip("\n") == snapshot("rules").rstrip("\n"))

    mixed = "tests/fixtures/mixed-changes.json"
    plan = (ROOT / mixed).read_text(encoding="utf-8")

    result = run(binary, "-", "--format", "json", stdin=plan)
    check("reads stdin", result.returncode == 0 and '"stateful-replace"' in result.stdout)

    result = run(binary, mixed, "--fail-on", "critical")
    check("--fail-on critical exits 2", result.returncode == 2, result.stderr)

    result = run(
        binary, mixed, "--fail-on", "critical",
        "--config", "tests/fixtures/config/quiet-mixed-changes.toml",
    )
    check("--config lowers severities (exits 0)", result.returncode == 0, result.stderr)

    result = run(binary, "does-not-exist.json")
    check("missing file exits 1", result.returncode == 1 and "could not read" in result.stderr)

    result = run(binary, "tests/fixtures/invalid/format-version-2.json")
    check("unsupported format exits 1", result.returncode == 1, result.stderr)

    errored = '{"format_version": "1.2", "errored": true, "complete": false, "planned_values": {}}'
    result = run(binary, "-", stdin=errored)
    check(
        "errored plan exits 1",
        result.returncode == 1 and "failed with errors" in result.stderr,
        result.stderr,
    )

    # Without --format and with stdout not a terminal, output is Markdown.
    result = run(binary, mixed)
    check("default format is markdown when piped", result.stdout.startswith("<!-- planrisk -->"))

    result = run(binary, mixed, "--format", "terminal")
    check("no color codes when piped", "\x1b[" not in result.stdout)

    print()
    if failures:
        print(f"{len(failures)} check(s) failed")
        sys.exit(1)
    print("all checks passed")


if __name__ == "__main__":
    main()

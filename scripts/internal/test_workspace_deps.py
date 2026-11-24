"""Regression tests for the metadata guard embedded in check_workspace_deps.sh."""
import json
from pathlib import Path
import subprocess
import sys
import unittest

SCRIPT = Path(__file__).with_name("check_workspace_deps.sh")
GUARD = SCRIPT.read_text().split("<<'PY'\n", 1)[1].split("\nPY", 1)[0]


def metadata():
    # The 12-package ss-* workspace after D-118 (`ss-mcp-hub` addition).
    names = [
        "ss-core", "ss-git", "ss-accounts",
        "ss-marketplace", "ss-skills", "ss-channels",
        "ss-mcp", "ss-mcp-hub", "ss-app", "ss-gui-bridge", "ss-gui-host", "ss-bin",
    ]
    return {
        "workspace_members": names,
        "packages": [dict(id=name, name=name, dependencies=[], targets=[]) for name in names],
    }


def edge(meta, source, target, **kwargs):
    package = next(p for p in meta["packages"] if p["name"] == source)
    package["dependencies"].append(dict(name=target, kind=None, **kwargs))


class WorkspaceBoundaries(unittest.TestCase):
    def check(self, meta, accepted, message=""):
        result = subprocess.run(
            [sys.executable, "-c", GUARD, json.dumps(meta)], capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0 if accepted else 1, result.stdout + result.stderr)
        self.assertIn(message, result.stdout)

    def test_current_domain_directions(self):
        meta = metadata()
        for source, target in [
            ("ss-skills", "ss-git"),
            ("ss-accounts", "ss-core"),
            ("ss-channels", "ss-skills"),
            ("ss-mcp", "ss-skills"),
            ("ss-mcp", "ss-git"),
            ("ss-mcp-hub", "ss-core"),
            ("ss-app", "ss-skills"),
            ("ss-app", "ss-channels"),
            ("ss-app", "ss-mcp"),
            ("ss-app", "ss-mcp-hub"),
            ("ss-app", "ss-marketplace"),
            ("ss-gui-bridge", "ss-accounts"),
            ("ss-gui-bridge", "ss-mcp-hub"),
            ("ss-gui-host", "ss-gui-bridge"),
            ("ss-bin", "ss-mcp"),
            ("ss-bin", "ss-gui-host"),
        ]:
            edge(meta, source, target)
        self.check(meta, True)

    def test_rejects_domain_inversions_even_for_dev_build_and_renamed_deps(self):
        for source, target in [
            ("ss-core", "ss-accounts"), ("ss-skills", "ss-marketplace"),
            ("ss-skills", "ss-channels"), ("ss-mcp", "ss-app"),
            ("ss-mcp-hub", "ss-skills"), ("ss-mcp-hub", "ss-mcp"),
            ("ss-accounts", "ss-app"), ("ss-app", "ss-accounts"),
            ("ss-gui-bridge", "ss-gui-host"),
        ]:
            for kind in [None, "dev", "build"]:
                with self.subTest(source=source, target=target, kind=kind):
                    meta = metadata()
                    edge(meta, source, target, rename="alias")
                    next(p for p in meta["packages"] if p["name"] == source)["dependencies"][0]["kind"] = kind
                    self.check(meta, False, f"forbidden edge: {source} -> {target}")

    def test_new_or_removed_crate_requires_an_explicit_boundary(self):
        for name in ["ss-oauth", "skillstar-channels", "skill-spec", "new-domain"]:
            meta = metadata()
            meta["workspace_members"].append(name)
            meta["packages"].append(dict(id=name, name=name, dependencies=[]))
            self.check(meta, False, "unclassified workspace package")

    def test_app_cannot_gain_a_second_binary(self):
        meta = metadata()
        next(p for p in meta["packages"] if p["name"] == "ss-app")["targets"] = [
            dict(name="alternate-entry", kind=["bin"]),
        ]
        self.check(meta, False, "ss-app still has bin targets")


if __name__ == "__main__":
    unittest.main()

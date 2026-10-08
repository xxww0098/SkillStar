"""Regression tests for the metadata guard embedded in check_workspace_deps.sh."""
import json
from pathlib import Path
import subprocess
import sys
import unittest

SCRIPT = Path(__file__).with_name("check_workspace_deps.sh")
GUARD = SCRIPT.read_text().split("<<'PY'\n", 1)[1].split("\nPY", 1)[0]


def metadata():
    names = [
        "ss-core", "ss-git", "ss-skills",
        "ss-marketplace", "ss-usage", "ss-sync",
        "claude-marketplace",
        "ss-app", "ss-gpui", "skillstar",
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
            ("ss-skills", "claude-marketplace"),
            ("ss-usage", "ss-core"), ("ss-app", "ss-skills"),
            ("ss-app", "ss-marketplace"), ("skillstar", "ss-gpui"),
        ]:
            edge(meta, source, target)
        self.check(meta, True)

    def test_rejects_domain_inversions_even_for_dev_build_and_renamed_deps(self):
        for source, target in [
            ("ss-core", "ss-usage"), ("ss-skills", "ss-marketplace"),
            ("ss-usage", "ss-app"), ("ss-app", "ss-usage"),
        ]:
            for kind in [None, "dev", "build"]:
                with self.subTest(source=source, target=target, kind=kind):
                    meta = metadata()
                    edge(meta, source, target, rename="alias")
                    next(p for p in meta["packages"] if p["name"] == source)["dependencies"][0]["kind"] = kind
                    self.check(meta, False, f"forbidden edge: {source} -> {target}")

    def test_new_or_removed_crate_requires_an_explicit_boundary(self):
        for name in ["skillstar-channels", "skill-spec", "new-domain"]:
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

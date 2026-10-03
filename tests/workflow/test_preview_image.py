import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[2] / "scripts/publish-preview-image.sh"
REVISION = "a" * 40
IMAGE = "ghcr.io/matty666/pkgly"


class PreviewImageTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        root = Path(self.directory.name)
        self.commands = root / "commands.jsonl"
        self.summary = root / "summary.md"
        docker = root / "docker"
        docker.write_text(
            "#!/usr/bin/env python3\n"
            "import json, os, sys\n"
            "with open(os.environ['MOCK_COMMANDS'], 'a') as log:\n"
            "    log.write(json.dumps(sys.argv[1:]) + '\\n')\n"
            "if sys.argv[1:3] == ['image', 'inspect']:\n"
            "    print(os.environ['MOCK_IMAGE_REVISION'])\n"
            "    sys.exit(int(os.environ.get('MOCK_INSPECT_EXIT', '0')))\n"
            "if sys.argv[1] == 'push' and sys.argv[2] == os.environ.get('MOCK_FAIL_PUSH'):\n"
            "    sys.exit(1)\n"
        )
        docker.chmod(0o755)
        self.environment = {
            **os.environ,
            "PATH": f"{root}:{os.environ['PATH']}",
            "MOCK_COMMANDS": str(self.commands),
            "MOCK_IMAGE_REVISION": REVISION,
            "GITHUB_STEP_SUMMARY": str(self.summary),
            "GITHUB_REPOSITORY": "Matty666/pkgly",
            "GITHUB_HEAD_REPOSITORY": "Matty666/pkgly",
            "GITHUB_HEAD_REF": "feature/generic-oidc",
            "PKGLY_IMAGE_REVISION": REVISION,
        }

    def run_script(self, **environment):
        result = subprocess.run(
            ["bash", str(SCRIPT)],
            env={**self.environment, **environment},
            capture_output=True,
            text=True,
        )
        commands = []
        if self.commands.exists():
            commands = [json.loads(line) for line in self.commands.read_text().splitlines()]
        return result, commands

    def test_publish_the_tested_image_under_the_fork_namespace(self):
        result, commands = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(commands[1:], [
            ["tag", "pkgly:test", f"{IMAGE}:{REVISION}"],
            ["push", f"{IMAGE}:{REVISION}"],
            ["tag", "pkgly:test", f"{IMAGE}:generic-oidc"],
            ["push", f"{IMAGE}:generic-oidc"],
        ])
        summary = self.summary.read_text()
        self.assertIn(f"docker pull {IMAGE}:{REVISION}", summary)
        self.assertNotIn(":latest", summary)

    def test_untrusted_sources_cannot_publish(self):
        for overrides in [
            {"GITHUB_REPOSITORY": "kshcherban/pkgly"},
            {"GITHUB_HEAD_REPOSITORY": "another/pkgly"},
            {"GITHUB_HEAD_REF": "main"},
        ]:
            with self.subTest(overrides=overrides):
                result, commands = self.run_script(**overrides)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(commands, [])

    def test_invalid_revisions_cannot_publish(self):
        for revision in ["", "a" * 39, "a" * 41, "invalid/tag", "$(touch /tmp/invalid)"]:
            with self.subTest(revision=revision):
                result, commands = self.run_script(PKGLY_IMAGE_REVISION=revision)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(commands, [])

    def test_a_different_image_revision_cannot_publish(self):
        result, commands = self.run_script(MOCK_IMAGE_REVISION="b" * 40)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(len(commands), 1)
        self.assertFalse(self.summary.exists())

    def test_an_inspection_failure_cannot_publish(self):
        result, commands = self.run_script(MOCK_INSPECT_EXIT="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(len(commands), 1)

    def test_failed_commit_push_does_not_advance_the_branch_tag(self):
        result, commands = self.run_script(MOCK_FAIL_PUSH=f"{IMAGE}:{REVISION}")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(commands[-1], ["push", f"{IMAGE}:{REVISION}"])
        self.assertFalse(any("generic-oidc" in argument for command in commands for argument in command))
        self.assertFalse(self.summary.exists())


if __name__ == "__main__":
    unittest.main()

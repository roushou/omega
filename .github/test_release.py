import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from release import Release


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.git("init", "-q")
        self.git("config", "user.name", "Release test")
        self.git("config", "user.email", "release@example.invalid")
        self.write("Cargo.toml", '[workspace.package]\nversion = "0.2.2"\n')
        for name in ["omega-cli", "omega-rs"]:
            self.write(
                f"crates/{name}/Cargo.toml",
                f'[package]\nname = "{name}"\nversion.workspace = true\n',
            )
        self.write(
            "crates/omega-renderer/shell/plugins/omega.view/manifest.json",
            '{"version": "0.2.2"}',
        )
        self.write(
            "CHANGELOG.md",
            "## What's Changed in 0.2.2\n* New feature\n\n"
            "### Details\nExplanation\n\n"
            "## What's Changed in 0.2.1\n* Old feature\n",
        )
        self.commit()
        self.git("tag", "-a", "v0.2.2", "-m", "Release")

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.root, text=True).strip()

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def commit(self):
        self.git("add", ".")
        self.git("commit", "-qm", "Fixture")

    def test_backfill_reads_tagged_files_instead_of_workflow_checkout(self):
        expected = self.git("rev-parse", "HEAD")
        self.write("Cargo.toml", '[workspace.package]\nversion = "0.3.0"\n')
        self.write("CHANGELOG.md", "unrelated working tree\n")
        self.commit()
        release = Release(self.root, "v0.2.2")
        self.assertEqual(release.commit, expected)
        self.assertEqual(release.crates, ["omega-cli", "omega-rs"])
        self.assertIn("### Details", release.notes)
        self.assertNotIn("Old feature", release.notes)
        with patch.dict("os.environ", {}, clear=True):
            release.write(self.root / "output")
        metadata = json.loads((self.root / "output/metadata.json").read_text())
        self.assertEqual(metadata["commit"], expected)
        self.assertEqual((self.root / "output/notes.md").read_text(), release.notes)

    def test_missing_or_mismatched_tag_is_rejected(self):
        for tag in ["main", "v1.2.3;echo bad", "v0.2.2-rc.1", "v00.2.2"]:
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                Release(self.root, tag)
        with self.assertRaises(subprocess.CalledProcessError):
            Release(self.root, "v0.2.3")
        self.git("tag", "v0.2.3")
        with self.assertRaisesRegex(ValueError, "workspace version"):
            Release(self.root, "v0.2.3")

    def test_renderer_and_crate_versions_must_match(self):
        for path, text in [
            ("crates/omega-renderer/shell/plugins/omega.view/manifest.json", '{"version":"0.2.1"}'),
            ("crates/omega-cli/Cargo.toml", '[package]\nname="omega-cli"\nversion="0.2.1"\n'),
        ]:
            with self.subTest(path=path):
                previous = (self.root / path).read_text()
                self.write(path, text)
                self.commit()
                self.git("tag", "-f", "v0.2.2")
                with self.assertRaisesRegex(ValueError, "version does not match"):
                    Release(self.root, "v0.2.2")
                self.write(path, previous)

    def test_notes_must_identify_exactly_one_nonempty_release(self):
        for text in ["", "## What's Changed in 0.2.2\n", "## What's Changed in 0.2.20\n* Other\n",
                     "## What's Changed in 0.2.2\n* A\n## What's Changed in 0.2.2\n* B\n"]:
            with self.subTest(text=text), self.assertRaises(ValueError):
                Release.extract_notes(text, "0.2.2")

    def test_every_crate_must_be_published_and_not_yanked(self):
        release = Release(self.root, "v0.2.2")
        published = b'{"vers":"0.2.2","yanked":false}\n'
        for unavailable in [b'{"vers":"0.2.1","yanked":false}\n', b'{"vers":"0.2.2","yanked":true}\n']:
            with patch("release.urllib.request.urlopen", side_effect=[io.BytesIO(published), io.BytesIO(unavailable)]):
                with self.assertRaisesRegex(ValueError, "publish omega-rs 0.2.2"):
                    release.check_registry()
        with patch("release.urllib.request.urlopen", side_effect=lambda *args, **kwargs: io.BytesIO(published)) as request:
            release.check_registry()
            self.assertEqual(request.call_count, 2)

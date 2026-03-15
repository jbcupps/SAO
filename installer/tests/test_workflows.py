import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]


class WorkflowContractTests(unittest.TestCase):
    def test_manual_image_build_uses_unified_production_dockerfile(self):
        workflow = (REPO_ROOT / ".github" / "workflows" / "build-sao-image.yml").read_text(
            encoding="utf-8"
        )

        self.assertIn("file: docker/Dockerfile", workflow)
        self.assertNotIn("docker/Dockerfile.sao", workflow)

    def test_release_image_publish_uses_unified_production_dockerfile(self):
        workflow = (REPO_ROOT / ".github" / "workflows" / "release.yml").read_text(
            encoding="utf-8"
        )

        self.assertIn("docker build -f docker/Dockerfile", workflow)
        self.assertNotIn("docker/Dockerfile.sao", workflow)


if __name__ == "__main__":
    unittest.main()

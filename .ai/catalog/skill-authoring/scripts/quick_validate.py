#!/usr/bin/env python3
"""Validate dev-template skill sources and their packaging contracts."""

from __future__ import annotations

import fnmatch
import math
import re
import sys
from pathlib import Path

import yaml


MANIFEST_KEYS = {
    "version",
    "name",
    "managed_by",
    "audience",
    "requires_capabilities",
    "budgets",
    "references",
    "package",
    "triggers",
}
FRONTMATTER_KEYS = {"name", "description", "license", "compatibility", "metadata", "allowed-tools"}
PROVIDER_TERMS = re.compile(r"\b(?:anthropic|claude|codex|openai|gpt-[a-z0-9.-]+)\b", re.IGNORECASE)
TEXT_SUFFIXES = {".md", ".yaml", ".yml", ".py", ".js", ".ts", ".toml", ".json", ".txt"}


def estimate_tokens(text: str) -> int:
    words = len(re.findall(r"\S+", text))
    return math.ceil(max(len(text.encode("utf-8")) / 4, words * 1.3))


def load_mapping(path: Path) -> dict:
    data = yaml.safe_load(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise ValueError(f"{path} must contain a YAML mapping")
    return data


def parse_frontmatter(path: Path) -> tuple[dict, str]:
    content = path.read_text(encoding="utf-8")
    match = re.match(r"^---\n(.*?)\n---\n", content, re.DOTALL)
    if not match:
        raise ValueError(f"{path} has invalid YAML frontmatter")
    data = yaml.safe_load(match.group(1))
    if not isinstance(data, dict):
        raise ValueError(f"{path} frontmatter must be a mapping")
    return data, content


def package_files(skill_path: Path, patterns: list[str]) -> set[Path]:
    files = {path.relative_to(skill_path) for path in skill_path.rglob("*") if path.is_file()}
    selected = {
        path
        for path in files
        if any(fnmatch.fnmatch(path.as_posix(), pattern) for pattern in patterns)
    }
    missing_patterns = [
        pattern for pattern in patterns if not any(fnmatch.fnmatch(path.as_posix(), pattern) for path in files)
    ]
    if missing_patterns:
        raise ValueError(f"package patterns match no files: {', '.join(missing_patterns)}")
    ignored = files - selected
    if ignored:
        raise ValueError("files missing from package allowlist: " + ", ".join(sorted(map(str, ignored))))
    return selected


def validate_skill(skill_path: str | Path) -> tuple[bool, str]:
    try:
        root = Path(skill_path).resolve()
        if not root.is_dir():
            raise ValueError(f"skill directory not found: {root}")
        skill_md = root / "SKILL.md"
        manifest_path = root / "manifest.yaml"
        if not skill_md.is_file() or not manifest_path.is_file():
            raise ValueError("SKILL.md and manifest.yaml are required")

        frontmatter, entry = parse_frontmatter(skill_md)
        unexpected_frontmatter = set(frontmatter) - FRONTMATTER_KEYS
        if unexpected_frontmatter:
            raise ValueError(f"unexpected frontmatter keys: {sorted(unexpected_frontmatter)}")
        name = frontmatter.get("name")
        description = frontmatter.get("description")
        if not isinstance(name, str) or not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", name):
            raise ValueError("name must be non-empty kebab-case")
        if len(name) > 64:
            raise ValueError("name exceeds 64 characters")
        if not isinstance(description, str) or not description.strip() or len(description) > 1024:
            raise ValueError("description must contain 1-1024 characters")

        manifest = load_mapping(manifest_path)
        if set(manifest) != MANIFEST_KEYS:
            raise ValueError(
                f"manifest keys differ: missing={sorted(MANIFEST_KEYS - set(manifest))}, "
                f"extra={sorted(set(manifest) - MANIFEST_KEYS)}"
            )
        if manifest["version"] != 1 or manifest["name"] != name or root.name != name:
            raise ValueError("directory, frontmatter, manifest name, or manifest version disagree")
        if manifest["managed_by"] != "dev-template":
            raise ValueError("managed_by must be dev-template")
        if manifest["audience"] not in {"default", "optional", "maintainer"}:
            raise ValueError("audience must be default, optional, or maintainer")
        if not isinstance(manifest["requires_capabilities"], list):
            raise ValueError("requires_capabilities must be a list")
        if set(manifest["budgets"]) != {"entry_tokens", "total_tokens"}:
            raise ValueError("budgets must contain entry_tokens and total_tokens")
        if estimate_tokens(entry) > manifest["budgets"]["entry_tokens"]:
            raise ValueError("SKILL.md exceeds its entry token budget")

        references = manifest["references"]
        if not isinstance(references, list) or any(not (root / path).is_file() for path in references):
            raise ValueError("every reference must name an existing file")
        triggers = manifest["triggers"]
        if set(triggers) != {"positive", "negative"}:
            raise ValueError("triggers must contain positive and negative cases")
        for label in ("positive", "negative"):
            cases = triggers[label]
            if not isinstance(cases, list) or len(cases) < 5 or any(not isinstance(case, str) for case in cases):
                raise ValueError(f"triggers.{label} must contain at least five strings")

        selected = package_files(root, manifest["package"])
        total = 0
        for relative in selected:
            path = root / relative
            if path.suffix in TEXT_SUFFIXES:
                text = path.read_text(encoding="utf-8")
                total += estimate_tokens(text)
                if manifest["audience"] != "maintainer" and PROVIDER_TERMS.search(text):
                    raise ValueError(f"provider-specific term in shared skill file: {relative}")
        if total > manifest["budgets"]["total_tokens"]:
            raise ValueError(f"packaged text estimate {total} exceeds total token budget")

        license_value = frontmatter.get("license")
        if isinstance(license_value, str) and license_value.endswith((".txt", ".md")):
            if not (root / license_value).is_file():
                raise ValueError(f"declared license file is missing: {license_value}")
        return True, f"valid: {name} ({estimate_tokens(entry)} entry tokens, {total} packaged text tokens)"
    except (OSError, ValueError, yaml.YAMLError) as error:
        return False, str(error)


def main() -> int:
    if len(sys.argv) != 2:
        print("Usage: python -m scripts.quick_validate <skill-directory>", file=sys.stderr)
        return 2
    valid, message = validate_skill(sys.argv[1])
    print(message)
    return 0 if valid else 1


if __name__ == "__main__":
    raise SystemExit(main())

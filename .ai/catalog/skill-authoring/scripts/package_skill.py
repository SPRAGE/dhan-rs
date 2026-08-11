#!/usr/bin/env python3
"""Create a deterministic .skill archive from an explicit package allowlist."""

from __future__ import annotations

import sys
import zipfile
from pathlib import Path

import yaml

try:
    from scripts.quick_validate import package_files, validate_skill
except ModuleNotFoundError:
    from quick_validate import package_files, validate_skill


def package_skill(skill_path: str | Path, output_dir: str | Path = ".") -> Path:
    root = Path(skill_path).resolve()
    valid, message = validate_skill(root)
    if not valid:
        raise ValueError(message)
    manifest = yaml.safe_load((root / "manifest.yaml").read_text(encoding="utf-8"))
    selected = package_files(root, manifest["package"])
    destination = Path(output_dir).resolve()
    destination.mkdir(parents=True, exist_ok=True)
    archive = destination / f"{root.name}.skill"
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as handle:
        for relative in sorted(selected):
            source = root / relative
            info = zipfile.ZipInfo(f"{root.name}/{relative.as_posix()}", (1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.create_system = 3
            info.external_attr = (source.stat().st_mode & 0xFFFF) << 16
            handle.writestr(info, source.read_bytes())
    return archive


def main() -> int:
    if len(sys.argv) not in {2, 3}:
        print("Usage: python -m scripts.package_skill <skill-directory> [output-directory]", file=sys.stderr)
        return 2
    try:
        archive = package_skill(sys.argv[1], sys.argv[2] if len(sys.argv) == 3 else ".")
    except (OSError, ValueError, yaml.YAMLError) as error:
        print(error, file=sys.stderr)
        return 1
    print(archive)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

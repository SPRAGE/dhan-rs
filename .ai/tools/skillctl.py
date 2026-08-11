#!/usr/bin/env python3
"""List and safely activate conditional project skills."""

from __future__ import annotations

import argparse
import os
import re
import sys
from pathlib import Path

import yaml


NAME = re.compile(r"[a-z0-9]+(?:-[a-z0-9]+)*\Z")


def load_catalog(root: Path) -> tuple[Path, dict]:
    ai = root / ".ai"
    catalog = ai / "catalog"
    index_path = catalog / "index.yaml"
    if not index_path.is_file():
        raise ValueError(f"catalog index not found: {index_path}")
    data = yaml.safe_load(index_path.read_text(encoding="utf-8"))
    if not isinstance(data, dict) or data.get("version") != 1 or not isinstance(data.get("skills"), dict):
        raise ValueError(f"invalid catalog index: {index_path}")
    return catalog, data


def skill_source(root: Path, catalog: Path, index: dict, name: str) -> Path:
    if not NAME.fullmatch(name) or name not in index["skills"]:
        raise ValueError(f"unknown catalog skill: {name}")
    expected_path = f".ai/catalog/{name}/SKILL.md"
    source = root / expected_path
    if not source.is_file() or source.resolve().parent.parent != catalog.resolve():
        raise ValueError(f"catalog entry escapes the catalog: {name}")
    manifest = source.parent / "manifest.yaml"
    if not manifest.is_file():
        raise ValueError(f"catalog skill has no manifest: {name}")
    return source.parent


def target_path(root: Path, name: str) -> Path:
    return root / ".ai" / "skills" / name


def points_to(target: Path, source: Path) -> bool:
    return target.is_symlink() and (target.parent / os.readlink(target)).resolve() == source.resolve()


def activate(root: Path, catalog: Path, index: dict, name: str) -> None:
    source = skill_source(root, catalog, index, name)
    target = target_path(root, name)
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.exists() or target.is_symlink():
        if points_to(target, source):
            print(f"active: {name}")
            return
        raise ValueError(f"refusing to replace existing skill: {target}")
    relative = os.path.relpath(source, target.parent)
    target.symlink_to(relative, target_is_directory=True)
    print(f"activated: {name} -> {relative}")


def deactivate(root: Path, catalog: Path, index: dict, name: str) -> None:
    source = skill_source(root, catalog, index, name)
    target = target_path(root, name)
    if not target.is_symlink():
        if target.exists():
            raise ValueError(f"refusing to remove non-link skill: {target}")
        print(f"inactive: {name}")
        return
    if not points_to(target, source):
        raise ValueError(f"refusing to remove unrelated link: {target}")
    target.unlink()
    print(f"deactivated: {name}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd(), help="project root (default: current directory)")
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("list", help="list catalog skills and activation state")
    subparsers.add_parser("active", help="list active catalog skills")
    for command in ("activate", "deactivate"):
        child = subparsers.add_parser(command, help=f"{command} catalog skills")
        child.add_argument("names", nargs="+")
    args = parser.parse_args()

    try:
        root = args.root.resolve()
        catalog, index = load_catalog(root)
        if args.command in {"list", "active"}:
            for name, spec in sorted(index["skills"].items()):
                source = skill_source(root, catalog, index, name)
                enabled = points_to(target_path(root, name), source)
                if args.command == "list" or enabled:
                    state = "active" if enabled else "available"
                    print(f"{name}\t{state}\t{spec['use_when']}")
            return 0
        names = list(dict.fromkeys(args.names))
        sources = {name: skill_source(root, catalog, index, name) for name in names}
        for name, source in sources.items():
            target = target_path(root, name)
            if args.command == "activate" and (target.exists() or target.is_symlink()):
                if not points_to(target, source):
                    raise ValueError(f"refusing to replace existing skill: {target}")
            if args.command == "deactivate":
                if target.exists() and not target.is_symlink():
                    raise ValueError(f"refusing to remove non-link skill: {target}")
                if target.is_symlink() and not points_to(target, source):
                    raise ValueError(f"refusing to remove unrelated link: {target}")
        for name in names:
            if args.command == "activate":
                activate(root, catalog, index, name)
            else:
                deactivate(root, catalog, index, name)
        return 0
    except (OSError, ValueError, yaml.YAMLError) as error:
        print(f"skillctl: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

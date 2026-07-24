#!/usr/bin/env python3
"""Multi-repo workspace tooling for the scorekit constellation.

The committed manifest (scorekit-workspace.json, repo root) is the single
source of truth for which projects exist: their remotes, roles, and the env
vars data directories are wired through. Machine-specific absolute paths live
in scorekit-workspace.local.json (gitignored) next to it.

Commands:
  doctor  Validate the manifest and resolve every project on this machine.
          Exit 0 when everything resolves, 1 when something is missing
          (with a clone/provisioning hint), 2 when a config file is invalid.
  gen     Deterministically generate <workspace-root>/scorekit.code-workspace
          and <workspace-root>/scorekit-workspace-env.sh from the resolved
          layout. Fail-closed: nothing is written unless every project
          resolves (or --allow-missing is given). Writes are atomic and
          skipped when content is unchanged.

Resolution order per project: local override > declared env var > sibling
directory convention (<parent-of-scorekit>/<name>). The scorekit checkout
itself always resolves to the repo root this script lives in.

Stdlib only; no third-party dependencies.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import tempfile
from pathlib import Path

MANIFEST_NAME = "scorekit-workspace.json"
LOCAL_NAME = "scorekit-workspace.local.json"
CODE_WORKSPACE_NAME = "scorekit.code-workspace"
ENV_FRAGMENT_NAME = "scorekit-workspace-env.sh"

VALID_KINDS = ("git", "data")
COMMON_KEYS = {"kind", "role", "description"}
KEYS_BY_KIND = {
    "git": COMMON_KEYS | {"remote"},
    "data": COMMON_KEYS | {"env", "provisioning"},
}


class ConfigError(Exception):
    """Manifest or local-override file is structurally invalid."""


def default_repo_root() -> Path:
    return Path(__file__).resolve().parent.parent


def load_json(path: Path) -> dict:
    try:
        with path.open(encoding="utf-8") as fh:
            data = json.load(fh)
    except FileNotFoundError:
        raise ConfigError(f"{path}: file not found")
    except json.JSONDecodeError as exc:
        raise ConfigError(f"{path}: invalid JSON: {exc}")
    if not isinstance(data, dict):
        raise ConfigError(f"{path}: top level must be an object")
    return data


def load_manifest(repo_root: Path) -> dict:
    path = repo_root / MANIFEST_NAME
    data = load_json(path)
    errors = []
    if data.get("version") != 1:
        errors.append('"version" must be 1')
    projects = data.get("projects")
    if not isinstance(projects, dict) or not projects:
        errors.append('"projects" must be a non-empty object')
        projects = {}
    if projects and "scorekit" not in projects:
        errors.append('"projects" must include the "scorekit" hub')
    for name, spec in projects.items():
        where = f'project "{name}"'
        if not name or "/" in name or name in (".", ".."):
            errors.append(f"{where}: name must be a plain directory name")
            continue
        if not isinstance(spec, dict):
            errors.append(f"{where}: must be an object")
            continue
        kind = spec.get("kind")
        if kind not in VALID_KINDS:
            errors.append(f'{where}: "kind" must be one of {list(VALID_KINDS)}')
            continue
        unknown = sorted(set(spec) - KEYS_BY_KIND[kind])
        if unknown:
            errors.append(f"{where}: unknown keys for kind={kind}: {unknown}")
        if not isinstance(spec.get("description"), str) or not spec["description"]:
            errors.append(f'{where}: "description" is required')
        if kind == "git":
            remote = spec.get("remote")
            ok = isinstance(remote, str) and (
                remote.startswith("https://") or remote.startswith("git@")
            )
            if not ok:
                errors.append(f'{where}: "remote" must be an https:// or git@ URL')
        if kind == "data":
            env = spec.get("env")
            if env is not None and (not isinstance(env, str) or not env):
                errors.append(f'{where}: "env" must be a non-empty string')
    if errors:
        raise ConfigError(f"{path}:\n  " + "\n  ".join(errors))
    return data


def load_local(repo_root: Path, project_names) -> dict:
    path = repo_root / LOCAL_NAME
    if not path.exists():
        return {}
    data = load_json(path)
    paths = data.get("paths", {})
    if not isinstance(paths, dict):
        raise ConfigError(f'{path}: "paths" must be an object')
    errors = []
    for name, value in paths.items():
        if name not in project_names:
            errors.append(f'unknown project "{name}" (not in {MANIFEST_NAME})')
        elif not isinstance(value, str) or not os.path.isabs(value):
            errors.append(f'"{name}": value must be an absolute path')
    if errors:
        raise ConfigError(f"{path}:\n  " + "\n  ".join(errors))
    return paths


def resolve(manifest: dict, local: dict, repo_root: Path, environ) -> list:
    """Resolve every project to (name, spec, path, source, exists, hint)."""
    resolved = []
    for name, spec in manifest["projects"].items():
        env_var = spec.get("env")
        if name == "scorekit":
            path, source = repo_root, "self"
        elif name in local:
            path, source = Path(local[name]), "override"
        elif env_var and environ.get(env_var):
            value = environ[env_var]
            if not os.path.isabs(value):
                raise ConfigError(f"${env_var} must be an absolute path, got: {value}")
            path, source = Path(value), "env"
        else:
            path, source = repo_root.parent / name, "sibling"
        exists = path.is_dir()
        hint = ""
        if not exists:
            if spec["kind"] == "git":
                hint = f"git clone {spec['remote']} {path}"
            else:
                hint = spec.get("provisioning", "provision manually")
                if env_var:
                    hint += f' — then export {env_var} or add it to "paths" in {LOCAL_NAME}'
        resolved.append(
            {
                "name": name,
                "kind": spec["kind"],
                "path": str(path),
                "source": source,
                "env": env_var,
                "exists": exists,
                "hint": hint,
            }
        )
    return resolved


def cmd_doctor(resolved: list, as_json: bool) -> int:
    missing = [p for p in resolved if not p["exists"]]
    if as_json:
        report = {"ok": not missing, "projects": resolved}
        print(json.dumps(report, indent=2, ensure_ascii=False))
        return 1 if missing else 0
    width = max(len(p["name"]) for p in resolved)
    for p in resolved:
        status = "ok     " if p["exists"] else "MISSING"
        print(f"{p['name']:<{width}}  {status}  [{p['source']:<8}]  {p['path']}")
        if not p["exists"]:
            print(f"{'':<{width}}           -> {p['hint']}")
    print(f"\n{len(resolved) - len(missing)}/{len(resolved)} projects resolved")
    return 1 if missing else 0


def render_code_workspace(resolved: list, ws_root: Path, previous: dict) -> str:
    folders = []
    for p in resolved:
        if not p["exists"]:
            continue
        target = Path(p["path"])
        try:
            rel = os.path.relpath(target, ws_root)
        except ValueError:  # e.g. different drive on Windows
            rel = str(target)
        folders.append({"name": p["name"], "path": rel})
    settings = previous.get("settings", {})
    if not isinstance(settings, dict):
        settings = {}
    doc = {"folders": folders, "settings": settings}
    return json.dumps(doc, indent=2, ensure_ascii=False) + "\n"


def render_env_fragment(resolved: list) -> str:
    lines = [
        "# Generated by scorekit scripts/workspace.py gen — do not edit.",
        "# source this file to point tools at the resolved data directories.",
    ]
    exports = sorted(
        f'export {p["env"]}="{p["path"]}"'
        for p in resolved
        if p["kind"] == "data" and p["env"] and p["exists"]
    )
    return "\n".join(lines + exports) + "\n"


def atomic_write(path: Path, content: str) -> str:
    """Write atomically; never leave a partial artifact. Returns action taken."""
    if path.exists() and path.read_text(encoding="utf-8") == content:
        return "unchanged"
    fd, tmp = tempfile.mkstemp(dir=str(path.parent), prefix=path.name + ".")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            fh.write(content)
        os.replace(tmp, path)
    except BaseException:
        try:
            os.unlink(tmp)
        except OSError:
            pass
        raise
    return "written"


def cmd_gen(resolved: list, repo_root: Path, allow_missing: bool, as_json: bool) -> int:
    missing = [p["name"] for p in resolved if not p["exists"]]
    if missing and not allow_missing:
        print(
            f"gen: refusing to generate with missing projects: {', '.join(missing)}\n"
            "     run `workspace.py doctor` for hints, or pass --allow-missing",
            file=sys.stderr,
        )
        return 1
    ws_root = repo_root.parent
    ws_file = ws_root / CODE_WORKSPACE_NAME
    previous = {}
    if ws_file.exists():
        try:
            previous = load_json(ws_file)
        except ConfigError:
            previous = {}
    # Render everything first, then write: no partial artifacts on failure.
    outputs = {
        ws_file: render_code_workspace(resolved, ws_root, previous),
        ws_root / ENV_FRAGMENT_NAME: render_env_fragment(resolved),
    }
    results = {str(path): atomic_write(path, content) for path, content in outputs.items()}
    if as_json:
        print(json.dumps({"ok": True, "skippedMissing": missing, "files": results},
                         indent=2, ensure_ascii=False))
    else:
        for path, action in results.items():
            print(f"{action}  {path}")
        if missing:
            print(f"skipped missing projects: {', '.join(missing)}")
    return 0


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--repo-root", type=Path, default=default_repo_root(),
                        help="scorekit checkout to operate on (for tests)")
    parser.add_argument("--json", action="store_true", help="machine-readable output")
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("doctor", help="validate manifest and resolve all projects")
    gen = sub.add_parser("gen", help="generate .code-workspace and env fragment")
    gen.add_argument("--allow-missing", action="store_true",
                     help="generate even when some projects are missing")
    args = parser.parse_args(argv)

    repo_root = args.repo_root.resolve()
    try:
        manifest = load_manifest(repo_root)
        local = load_local(repo_root, set(manifest["projects"]))
        resolved = resolve(manifest, local, repo_root, os.environ)
    except ConfigError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    if args.command == "doctor":
        return cmd_doctor(resolved, args.json)
    return cmd_gen(resolved, repo_root, args.allow_missing, args.json)


if __name__ == "__main__":
    sys.exit(main())

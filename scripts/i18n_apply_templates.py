#!/usr/bin/env python3
"""
Replace Japanese UI strings in admin HTML templates with i18n lookup calls.

Reads src/i18n/strings.json and updates templates/admin/**/*.html in place.
Rust sources are not modified.

Usage (from repo root):
  python3 scripts/i18n_apply_templates.py
  python3 scripts/i18n_apply_templates.py --dry-run
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STRINGS_PATH = ROOT / "src" / "i18n" / "strings.json"
ADMIN_TEMPLATES = ROOT / "templates" / "admin"

LOGIN_TEMPLATE = ADMIN_TEMPLATES / "login.html"
PREVIEW_ERROR = ADMIN_TEMPLATES / "pages" / "preview_error.html"


def load_strings() -> list[tuple[str, str]]:
    data: dict[str, dict[str, str]] = json.loads(
        STRINGS_PATH.read_text(encoding="utf-8")
    )
    best: dict[str, tuple[str, str]] = {}

    def rank(key: str) -> tuple[int, str]:
        if key.startswith("nav."):
            return (0, key)
        if key.startswith("login."):
            return (1, key)
        if key.startswith("admin."):
            return (2, key)
        if key.startswith("breadcrumb."):
            return (3, key)
        if key.startswith("public."):
            return (4, key)
        return (5, key)

    for key, entry in data.items():
        ja = entry.get("ja", "")
        if not ja:
            continue
        if ja not in best or rank(key) < rank(best[ja][1]):
            best[ja] = (ja, key)

    pairs = list(best.values())
    pairs.sort(key=lambda x: len(x[0]), reverse=True)
    return pairs


def template_expr(path: Path, key: str) -> str:
    if path in (LOGIN_TEMPLATE, PREVIEW_ERROR):
        return '{{ t("' + key + '") }}'
    return '{{ layout.t("' + key + '") }}'


def apply_to_file(path: Path, pairs: list[tuple[str, str]], dry_run: bool) -> int:
    original = path.read_text(encoding="utf-8")
    updated = original
    replacements = 0
    for ja, key in pairs:
        if ja not in updated:
            continue
        expr = template_expr(path, key)
        new_updated = updated.replace(ja, expr)
        if new_updated != updated:
            replacements += 1
            updated = new_updated
    if updated != original and not dry_run:
        path.write_text(updated, encoding="utf-8")
    return replacements if updated != original else 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Apply i18n keys to admin templates")
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Report changes without writing files",
    )
    args = parser.parse_args()

    if not STRINGS_PATH.is_file():
        print(f"Missing {STRINGS_PATH}", file=sys.stderr)
        return 1

    pairs = load_strings()
    modified: list[tuple[str, int]] = []

    for path in sorted(ADMIN_TEMPLATES.rglob("*.html")):
        n = apply_to_file(path, pairs, args.dry_run)
        if n:
            modified.append((str(path.relative_to(ROOT)), n))

    total_keys = len(json.loads(STRINGS_PATH.read_text(encoding="utf-8")))
    print(f"strings.json keys: {total_keys}")
    print(f"Modified template files: {len(modified)}")
    for rel, n in modified:
        print(f"  {rel} ({n} string replacements)")
    if args.dry_run:
        print("(dry run — no files written)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

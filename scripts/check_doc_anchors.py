#!/usr/bin/env python3
"""
check_doc_anchors.py — Verify internal Markdown anchor links in documentation.

Validates that every internal `#anchor` link in Markdown files (default: docs/*.md)
resolves to an existing heading in the document using GitHub-Flavored Markdown (GFM)
anchor slug generation rules. For cross-file links with anchors (e.g., [text](OTHER.md#anchor)),
validates the target anchor if the target file exists.

Exits with code 0 if all internal anchors are valid, or 1 if any broken anchors are found.
"""

import os
import re
import sys
from pathlib import Path


def slugify(text: str, existing_counts: dict) -> str:
    """Generate a GitHub-flavored Markdown anchor slug for a heading."""
    # 1. Remove HTML tags if present
    clean = re.sub(r"<[^>]+>", "", text)
    # 2. Lowercase
    s = clean.lower()
    # 3. Strip punctuation: keep alphanumeric, whitespace, hyphens, and underscores
    s = re.sub(r"[^\w\s-]", "", s)
    # 4. Replace whitespace characters with hyphens
    s = re.sub(r"\s", "-", s)

    # 5. Handle duplicate headings
    if s in existing_counts:
        existing_counts[s] += 1
        slug = f"{s}-{existing_counts[s]}"
    else:
        existing_counts[s] = 0
        slug = s
    return slug


def parse_markdown_headings(content: str) -> dict:
    """
    Parse all headings in markdown content outside code fences.
    Returns a dict mapping slug to heading info (line_num, level, title).
    """
    in_code_block = False
    counts = {}
    headings = {}

    for idx, line in enumerate(content.splitlines(), 1):
        stripped = line.strip()
        if stripped.startswith("```"):
            in_code_block = not in_code_block
            continue
        if in_code_block:
            continue

        match = re.match(r"^(#{1,6})\s+(.*)$", line)
        if match:
            lvl = len(match.group(1))
            title = match.group(2).strip()
            slug = slugify(title, counts)
            headings[slug] = (idx, lvl, title)

    return headings


def check_markdown_file(file_path: Path, all_file_headings: dict) -> list:
    """
    Check internal anchor links in a markdown file.
    Returns a list of error tuples: (line_num, link_text, target, reason).
    """
    with open(file_path, "r", encoding="utf-8") as f:
        content = f.read()

    file_slugs = all_file_headings.get(file_path.resolve(), {})
    errors = []
    in_code_block = False
    lines = content.splitlines()

    # Matches [link text](#anchor) or [link text](relative_path.md#anchor)
    link_regex = re.compile(r"\[([^\]]+)\]\(([^)]+)\)")

    for idx, line in enumerate(lines, 1):
        if line.strip().startswith("```"):
            in_code_block = not in_code_block
            continue
        if in_code_block:
            continue

        for match in link_regex.finditer(line):
            link_text = match.group(1)
            raw_target = match.group(2).strip()

            if raw_target.startswith("#"):
                # Internal anchor within same file
                anchor = raw_target[1:]
                if anchor and anchor not in file_slugs:
                    errors.append(
                        (idx, link_text, raw_target, f"Anchor '#{anchor}' not found in {file_path.name}")
                    )
            elif "#" in raw_target and not (raw_target.startswith("http://") or raw_target.startswith("https://")):
                target_file_str, anchor = raw_target.split("#", 1)
                # Resolve relative file path if the file exists on disk
                target_path = (file_path.parent / target_file_str).resolve()
                if target_path.exists():
                    target_slugs = all_file_headings.get(target_path)
                    if target_slugs is None:
                        with open(target_path, "r", encoding="utf-8") as tf:
                            target_slugs = parse_markdown_headings(tf.read())
                        all_file_headings[target_path] = target_slugs
                    if anchor and anchor not in target_slugs:
                        errors.append(
                            (idx, link_text, raw_target, f"Anchor '#{anchor}' not found in {target_path.name}")
                        )

    return errors


def main() -> int:
    repo_root = Path(__file__).resolve().parent.parent
    docs_dir = repo_root / "docs"

    if len(sys.argv) > 1:
        target_files = [Path(p).resolve() for p in sys.argv[1:]]
    else:
        if not docs_dir.exists():
            print(f"Error: {docs_dir} not found.", file=sys.stderr)
            return 1
        target_files = sorted(docs_dir.glob("*.md"))

    print(f"Checking {len(target_files)} Markdown document(s) for anchor links...\n")

    # Pre-parse headings for all target files
    all_file_headings = {}
    for file_path in target_files:
        if file_path.exists():
            with open(file_path, "r", encoding="utf-8") as f:
                all_file_headings[file_path.resolve()] = parse_markdown_headings(f.read())

    total_errors = 0
    total_files_checked = 0

    for file_path in target_files:
        if not file_path.exists():
            print(f"Skipping non-existent file: {file_path}")
            continue

        total_files_checked += 1
        errors = check_markdown_file(file_path, all_file_headings)
        rel_path = file_path.relative_to(repo_root) if file_path.is_relative_to(repo_root) else file_path

        if errors:
            total_errors += len(errors)
            print(f"[FAIL] {rel_path} ({len(errors)} broken anchor(s)):")
            for line_num, text, target, reason in errors:
                print(f"  Line {line_num}: [{text}]({target}) -> {reason}")
        else:
            heading_count = len(all_file_headings.get(file_path.resolve(), {}))
            print(f"[OK]   {rel_path} ({heading_count} headings checked)")

    print(f"\nChecked {total_files_checked} file(s).")
    if total_errors > 0:
        print(f"FAILED: Found {total_errors} broken anchor link(s).", file=sys.stderr)
        return 1

    print("PASSED: All internal anchor links resolve successfully.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

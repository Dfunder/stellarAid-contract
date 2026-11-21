#!/usr/bin/env python3
"""
scripts/lint_tables.py — Enforce maximum cell length and table accessibility in documentation.

Closes #1141.

Acceptance criteria:
- No table cell in docs/ exceeds the agreed limit (default 280 characters).
- Flag table cells that force horizontal scrolling or unnavigable text walls for screen-readers.
"""

import sys
import os
import glob
import argparse

MAX_CELL_LENGTH = 280

def check_file(filepath, max_len=MAX_CELL_LENGTH):
    violations = []
    with open(filepath, 'r', encoding='utf-8') as f:
        lines = f.readlines()

    in_table = False
    in_code_block = False

    for line_idx, line in enumerate(lines, 1):
        stripped = line.strip()

        # Handle fenced code blocks
        if stripped.startswith("```"):
            in_code_block = not in_code_block
            continue

        if in_code_block:
            continue

        if stripped.startswith('|') and stripped.endswith('|'):
            # Table row
            raw_cells = stripped.split('|')[1:-1]
            # Check if this is a separator row e.g. |---|---|
            if all(c.strip().replace('-', '').replace(':', '') == '' for c in raw_cells):
                in_table = True
                continue

            for col_idx, cell in enumerate(raw_cells, 1):
                cell_text = cell.strip()
                cell_len = len(cell_text)
                if cell_len > max_len:
                    violations.append({
                        'file': filepath,
                        'line': line_idx,
                        'col': col_idx,
                        'len': cell_len,
                        'preview': cell_text[:80] + ('...' if cell_len > 80 else '')
                    })
        else:
            in_table = False

    return violations

def main():
    parser = argparse.ArgumentParser(description="Lint markdown table cell lengths for screen-reader accessibility.")
    parser.add_argument('files', nargs='*', help="Markdown files to lint. Defaults to docs/*.md if omitted.")
    parser.add_argument('--max-len', type=int, default=MAX_CELL_LENGTH, help=f"Max cell character length (default: {MAX_CELL_LENGTH})")
    args = parser.parse_args()

    target_files = args.files
    if not target_files:
        target_files = sorted(glob.glob("docs/**/*.md", recursive=True))
        if not target_files:
            target_files = sorted(glob.glob("docs/*.md"))

    if not target_files:
        print("No markdown files found to check.")
        sys.exit(0)

    total_violations = 0
    checked_files = 0

    print(f"Linting table cells (max length: {args.max_len} chars)...")
    for f in target_files:
        if not os.path.isfile(f):
            continue
        checked_files += 1
        violations = check_file(f, max_len=args.max_len)
        if violations:
            total_violations += len(violations)
            print(f"\n[FAIL] {f} ({len(violations)} violation{'s' if len(violations) > 1 else ''}):")
            for v in violations:
                print(f"  Line {v['line']}, Col {v['col']}: {v['len']} chars (cap: {args.max_len})")
                print(f"    Preview: {v['preview']}")
        else:
            print(f"[PASS] {f}")

    print(f"\nChecked {checked_files} file(s).")
    if total_violations > 0:
        print(f"FAILED: {total_violations} table cell(s) exceed {args.max_len} characters.")
        sys.exit(1)
    else:
        print(f"PASSED: All table cells within {args.max_len} character cap.")
        sys.exit(0)

if __name__ == '__main__':
    main()

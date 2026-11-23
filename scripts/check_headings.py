#!/usr/bin/env python3
import sys

def check_file(path):
    with open(path, 'r', encoding='utf-8') as f:
        lines = f.readlines()
    curr_level = 0
    in_code = False
    violations = []
    for idx, l in enumerate(lines, 1):
        s = l.strip()
        if s.startswith('```'):
            in_code = not in_code
            continue
        if in_code:
            continue
        if s.startswith('#'):
            level = len(s) - len(s.lstrip('#'))
            if s[level:level+1] == ' ':
                if level > curr_level + 1 and curr_level != 0:
                    violations.append((idx, curr_level, level, s))
                curr_level = level

    print(f"{path}: {len(violations)} skipped heading transitions")
    for idx, prev, nxt, text in violations:
        print(f"  Line {idx}: jumped from h{prev} to h{nxt}: {text}")
    return len(violations)

if __name__ == '__main__':
    v1 = check_file('docs/CONTRACTS.md')
    v2 = check_file('docs/EVENTS.md')
    if v1 + v2 > 0:
        sys.exit(1)
    print("All headings continuous and valid.")
    sys.exit(0)

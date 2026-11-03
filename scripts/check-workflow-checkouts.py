#!/usr/bin/env python3
"""Require literal persist-credentials: false on every workflow checkout step."""

import argparse
from pathlib import Path
import sys

import yaml


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('paths', nargs='*', type=Path, default=[Path('.github/workflows')])
    args = parser.parse_args()
    failed = False
    for path in args.paths:
        files = sorted(p for p in path.rglob('*') if p.suffix in {'.yml', '.yaml'}) if path.is_dir() else [path]
        for file in files:
            # Keep scalar spellings: YAML 1.1 also treats 'off' and 'no' as false.
            workflow = yaml.load(file.read_text(encoding='utf-8'), Loader=yaml.BaseLoader)
            for job_name, job in workflow.get('jobs', {}).items():
                for index, step in enumerate(job.get('steps', [])):
                    action = step.get('uses', '').split('@', 1)[0].lower()
                    if action != 'actions/checkout':
                        continue
                    value = step.get('with', {}).get('persist-credentials')
                    if isinstance(value, str) and value.lower() == 'false':
                        continue
                    print(f'{file}: jobs.{job_name}.steps[{index}]: '
                          'actions/checkout requires literal persist-credentials: false', file=sys.stderr)
                    failed = True
    return int(failed)


if __name__ == '__main__':
    raise SystemExit(main())

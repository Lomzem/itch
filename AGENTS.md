## Agent skills

### Issue tracker

Track work as local Markdown tickets under `.scratch/<feature>/`.
Read `docs/agents/issue-tracker.md` before creating, fetching, or updating tickets.

### Triage labels

Use the default triage vocabulary.
Read `docs/agents/triage-labels.md` before assigning triage status.

### Domain docs

Use a single-context layout.
Read `docs/agents/domain.md` before exploring the codebase.

## Public repo

`github.com/Lomzem/itch` is public. AJA-internal material (schematics, real DUT Profiles, AJA product names, SSIDs) lives only in gitignored paths: `.scratch/` and `docs/hardware/schematics/`. Examples, docs and tests use a made-up DUT.

## Design priority

itch is an internal tool, usually driven by one QA engineer with one script per Rig. Optimise for that engineer's experience over rare multi-client or failure edge cases; don't add errors, ceremony or client bookkeeping to guard against them. See `docs/adr/0004-optimise-for-one-qa-engineer.md`.

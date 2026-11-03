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

## Repository split

This repo owns the firmware, simulator and protocol. The Python client lives in [Lomzem/itch-py](https://github.com/Lomzem/itch-py), usually checked out alongside it.
Before changing the protocol, coordinating work across repos, or handling CI or releases, read [project context](docs/agents/project.md).

## Public repo

`github.com/Lomzem/itch` is public. AJA-internal material (schematics, real DUT Profiles, AJA product names, SSIDs) lives only in gitignored paths: `.scratch/` and `docs/hardware/schematics/`. Examples, docs and tests use a made-up DUT.

## Design priority

itch is an internal tool, usually driven by one QA engineer with one script per Rig. Optimise for that engineer's experience over rare multi-client or failure edge cases; don't add errors, ceremony or client bookkeeping to guard against them. See `docs/adr/0004-optimise-for-one-qa-engineer.md`.

## Enforced invariants

| Rule | Enforcement |
| --- | --- |
| Credential diagnostics omit passphrases. | `Credentials` and `SetWifi` own redacted `Debug`; `crates/itch-core/tests/wifi.rs` checks direct and enclosing diagnostics. |
| Blocking work preserves the Rig's check-in deadline. | `serve` bounds reply waits; `settings::Supervised` checks in around flash operations; firmware connection, supervisor, flash and Wi-Fi tests cover starvation and measured association stalls. |
| Workflow checkouts keep credentials out of Git configuration. | The `workflow-lint` CI job runs `scripts/check-workflow-checkouts.py`, which requires literal `persist-credentials: false` on every checkout regardless of upload order; zizmor adds general workflow audits. |

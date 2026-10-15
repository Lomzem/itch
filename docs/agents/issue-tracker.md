# Issue tracker: Local Markdown

Issues and specs live as Markdown files in `.scratch/`.

## Conventions

- One feature per directory: `.scratch/<feature-slug>/`.
- Specs: `.scratch/<feature-slug>/spec.md`.
- Implementation tickets: `.scratch/<feature-slug>/issues/<NN>-<slug>.md`,
  numbered from `01`, with one file per ticket.
- Record triage state in a `Status:` line near the top of each ticket.
  Use the role strings in `triage-labels.md`.
- Append conversation under `## Comments`.

## Publishing and fetching

To publish, create the appropriate spec or ticket file, including parent
directories as needed. To fetch, read the referenced file. Resolve a bare
ticket number within the relevant feature directory.

## Wayfinding operations

- Map: `.scratch/<effort>/map.md`, containing Notes, Decisions-so-far,
  and Fog sections.
- Child tickets: `.scratch/<effort>/issues/NN-<slug>.md`.
- Record ticket type with `Type: research`, `prototype`, `grilling`, or `task`.
- Record dependencies with `Blocked by: NN, NN`.
- Frontier: select the lowest-numbered open, unblocked, unclaimed ticket.
  A ticket is unblocked when all its dependencies are resolved.
- Claim: save `Status: claimed` before starting work.
- Resolve: append the answer under `## Answer`, set `Status: resolved`,
  and add a gist and link to the map's Decisions-so-far section.

Wayfinding uses `claimed` and `resolved` as workflow statuses; triage uses
the vocabulary in `triage-labels.md`.

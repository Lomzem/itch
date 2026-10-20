# Project context

## Repository ownership

- [itch](https://github.com/Lomzem/itch) owns the Rust core, Rig firmware,
  simulator, protocol specification and conformance vectors.
- [itch-py](https://github.com/Lomzem/itch-py) owns the Python client,
  Profiles, USB discovery and CLI. Its Python distribution and
  import name are both `itch`.

Sibling checkouts are convenient for development; each repo also works
independently. Read the other repo's `AGENTS.md` before editing it.

The shared [glossary](../../GLOSSARY.md) and [project decisions](../adr/)
live in itch. Profiles remain a client concern ([ADR 0002](../adr/0002-profiles-are-client-side.md)).
For wire changes, coordinate both codecs and the authoritative vectors in
itch under the compatibility rules in [ADR 0005](../adr/0005-payload-encoding.md).

## CI and release coordination

itch-py CI consumes the simulator and vectors from the itch release pinned
in `tests/itch_release.py`; it commits no copy of the vectors. Local tests
can instead build a simulator from a checkout. A local pass against newer
source therefore does not establish compatibility with the pinned release.

When work needs a new itch release:

1. Pass itch CI and publish the matching version tag using its
   [release instructions](../../README.md#releases).
2. Wait for the Release workflow to publish the simulators, vectors and
   manifest. Verify the manifest's tag, commit, protocol and asset hashes.
   A tag alone does not make these downloads available: a missing-manifest
   404 in itch-py can mean publication is still pending.
3. Update itch-py's release pin and README compatibility table as needed.
   Require its CI to pass against the published assets before merging.

For an itch-py release, verify post-merge CI, publish a version tag and
GitHub release, then check installation and package version from that
public tag. QA installs from GitHub; hatch-vcs derives the Python
package version from Git tags.

Release versions and the wire `protocol` integer are separate: the initial
protocol-v1 implementation shipped as `v0.1.0`. Use each repo's version and
the Python compatibility table to choose releases. Published tags stay fixed.

# Triage labels

Use these strings in local tickets' `Status:` lines.

| Canonical role | Local status | Meaning |
| --- | --- | --- |
| needs-triage | needs-triage | Maintainer needs to evaluate the issue |
| needs-info | needs-info | Waiting on reporter for more information |
| ready-for-agent | ready-for-agent | Fully specified and ready for an agent |
| ready-for-human | ready-for-human | Requires human implementation |
| wontfix | wontfix | Will not be actioned |

When a skill asks to apply a triage label, update the ticket's `Status:`
line using this mapping.

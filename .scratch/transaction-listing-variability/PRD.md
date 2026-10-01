Status: needs-triage

# PRD — Investigate transaction-listing fixed-target variability

## Parent

#68 / PRD #62 decision gate, `docs/nav-rollout-performance-evidence.md`.

## Problem

Four complete runs of the established offline Criterion report recorded
immutable `transaction_listing_representative` and
`transaction_listing_stress` p95 misses that varied widely run-to-run
(representative 22607817.0–31015634.0 ns and stress 100651388.0–135363533.0 ns
against immutable 21554013 / 98552625 ns targets), while the small
`transaction_listing`, NAV-specific, and startup targets passed in every run.
On 2026-10-01 the user accepted the latest two misses as a documented rollout
exception; the root cause is unresolved.

## What to build (investigation only; no implementation expansion)

- Determine the cause of run-to-run p95 variability on the
  `transaction_listing_shared_representative|stress` fixtures (host load,
  fixture/cache effects, criterion sampling, or a real regression).
- Establish whether the code path is stable in a known reference environment
  and whether the fixtures/protocol capture variance adequately.
- Deliver a written provenance-backed conclusion; propose remedies only with
  the immutability rule unchanged (target changes continue to require explicit
  user approval).

## Non-goals

- No production code change, no harness target edit, no additional acceptance
  for #68 except recording findings.
- No network calls, no user database access.

## Acceptance criteria

- [ ] Root-cause conclusion (or justified uncertainty) recorded with execution
  environment and methodology provenance.
- [ ] First-class evidence: repeated complete runs performed after host-load
  conditions are understood, with all runs preserved.
- [ ] Recommendation to the user about the accepted exception (uphold, revisit,
  or propose target change through approval) without implementing it.

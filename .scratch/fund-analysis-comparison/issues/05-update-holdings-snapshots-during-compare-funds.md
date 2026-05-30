# Update Holdings Snapshots During Compare Funds

Status: ready-for-agent

## Parent

`.scratch/fund-analysis-comparison/PRD.md`

## What to build

Make `compare funds` update holdings snapshot history for both compared funds using the same snapshot rules as `analyze fund`, while keeping the comparison output focused on fund-vs-fund analysis by not displaying holdings snapshot diffs.

## Acceptance criteria

- [ ] Running `compare funds` records or reuses a holdings snapshot for each compared fund.
- [ ] Snapshot identity remains Morningstar code plus reported portfolio date, falling back to today when no portfolio date exists.
- [ ] Snapshot fingerprinting uses the same holding name and weight rules as single-fund analysis.
- [ ] Existing duplicate-prevention behavior is reused so repeated comparison of the same reported snapshot does not create duplicates.
- [ ] Weight-change tolerance and diff classification rules remain consistent with single-fund analysis internally.
- [ ] Quote metadata is excluded from snapshot fingerprints and snapshot history.
- [ ] The comparison output does not show holdings snapshot diffs.
- [ ] Command help or documentation makes the snapshot side effect clear if command help is updated as part of the implementation.

## Blocked by

None - can start immediately

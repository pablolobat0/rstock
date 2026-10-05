# ADR-0004: Use Library Dispatch and CLI Output Adapters

## Status

Accepted

## Context

`lib.rs` and `main.rs` currently declare parallel copies of the application module graph. Integration tests and benchmarks use the library graph, while the executable dispatches through distinct binary-local types. Portfolio domain models are also serialized directly for JSON, so serde names, null behaviour, sorting, and output compatibility shape those models; human output performs a second history query after it has begun printing.

## Decision

The `rstock` library will own command dispatch and the sole application module graph. `main.rs` will be a thin process bootstrap and will not redeclare `cli`, `models`, `services`, or the other library modules. Both dashboard aliases will reach the same library-owned dispatch path used by tests.

The CLI output Seam will use separate human and JSON Adapters over presentation-neutral outcomes. Both Adapters write through an injected `Write` target rather than printing directly. Portfolio domain outcomes will not be serialized directly. The JSON Adapter may introduce the simpler new nested schema because the existing unversioned schema has no compatibility requirement; it maps unavailable and not-applicable financial facts to `null`, includes requested NAV history so `--period` affects JSON, and retains structured limitation scopes. The human Adapter renders not applicable distinctly from unavailable and prints actionable limitations under their four scopes.

The Portfolio view outcome is fully obtained before either Adapter emits output. Sorting, field names, JSON null mapping, terminal formatting, and chart rendering remain CLI concerns.

## Consequences

Production, integration tests, and benchmarks compile and exercise one nominal set of application types. Domain refactors no longer change JSON accidentally, while the two output Adapters form a real Seam because they intentionally present availability differently.

The migration order is: first move production to library-owned dispatch, then introduce and test the composed Portfolio view outcome, then switch the CLI output Adapters, and finally remove the flat models and manual copying. Verification is layered into Module Interface tests, output Adapter tests, and process tests proving both dashboard aliases use library dispatch. JSON Adapter tests assert the complete schema and values structurally rather than relying on field order; human Adapter tests assert semantic labels and availability states rather than spacing or ANSI bytes. No database migration is required.

## Alternatives Considered

- Keep dispatch in `main.rs` while importing library modules: rejected because command dispatch would remain outside the application graph tested through the library Interface.
- Continue direct domain serialization: rejected because JSON concerns would keep shaping Portfolio outcome models.
- Preserve or version the old JSON schema: rejected because there is no compatibility requirement and no concrete external consumer requiring dual-schema machinery.
- Let the CLI fetch chart history after rendering the summary: rejected because human output can look successful before a later history error and `--period` has no JSON meaning.

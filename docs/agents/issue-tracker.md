# Issue tracker: Local Markdown

Issues and PRDs for this repo live as markdown files in `.scratch/`.

## Conventions

- One feature per directory: `.scratch/<feature-slug>/`
- The PRD is `.scratch/<feature-slug>/PRD.md`
- Implementation issues are `.scratch/<feature-slug>/issues/<NN>-<slug>.md`, numbered from `01`
- Triage state is recorded as a `Status:` line near the top of each issue file (see `triage-labels.md` for the role strings)
- Comments and conversation history append to the bottom of the file under a `## Comments` heading

## Dependency execution

- Record prerequisites as `Blocked by:` issue references. `Readiness: ready` means every prerequisite is integrated and verified; otherwise use `Readiness: blocked`.
- Keep readiness separate from `Status:` and `Claimed by:`. A running issue can remain ready. Claims identify the owning Orca Task/Dispatch.
- Mark `Status: done` only after independent standards/spec review, feature-branch integration and combined checks. Record the integrated revision and acceptance evidence under `## Comments`, then remove readiness and release the claim.
- Worker submission or a closed prerequisite without its integrated artifact does not release successors. Record source-access or product-decision blockers separately from dependency readiness.

## When a skill says "publish to the issue tracker"

Create a new file under `.scratch/<feature-slug>/` (creating the directory if needed).

## When a skill says "fetch the relevant ticket"

Read the file at the referenced path. The user will normally pass the path or the issue number directly.

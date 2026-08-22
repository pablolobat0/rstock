---
name: orchestrate-prd-issues
description: Use in a dedicated orchestration session when asked to implement a PRD and its issues through Orca, OpenCode workers, dependency waves, issue branches, GitHub PRs, approval-gated merges, and cleanup.
---

# Orchestrate PRD Issues

Run the implementation phase of a two-session workflow.

The planning session is already complete: the user used skills such as `grill-with-docs`, `to-spec`, and `to-tickets` to produce a PRD and dependency-linked issues. This orchestration session receives that PRD and those issues, then drives implementation through reviewed GitHub PRs.

## Required Outcome

- One pushed PRD branch acts as the integration base.
- One Orca worktree, issue branch, and fresh OpenCode worker exist per active issue.
- Each worker loads the `implement` skill, implements exactly one issue, runs its internal OpenCode reviews, commits, pushes, and opens a GitHub PR targeting the PRD branch.
- The coordinator starts every dependency-ready issue that can safely run concurrently.
- Issue Tasks remain incomplete while their PRs await user approval.
- The coordinator reports: `X pending PRs are ready for approval` with links and review/test summaries.
- Only after the user approves and each PR is verified merged does the coordinator complete its Task, update issue state, remove its worktree, and unlock dependents.

## Ownership Boundaries

The coordinator owns:

- The Orca Run and Task DAG.
- PRD branch creation and synchronization.
- Dependency scheduling and concurrency.
- User-facing merge approval gates.
- Merging approved PRs.
- Task and issue completion after merge.
- Orca terminal/worktree cleanup.

Each issue worker owns:

- One issue branch and worktree.
- Loading and following the `implement` skill.
- Code changes, tests, commits, and internal review/fix cycles.
- Pushing its issue branch.
- Opening or updating its GitHub PR against the PRD branch.
- Reporting `merge_ready` with complete evidence.

Workers may use OpenCode subagents required by the `implement` skill. They must not create Orca Runs, Tasks, Dispatches, terminals, or worktrees. Internal reviews are OpenCode subagents inside the worker session, not separate Orca workers.

## Non-Negotiable Rules

- Load the live `orchestration` and `orca-cli` guides before using Orca.
- Never reuse one OpenCode session for multiple issues.
- Never implement in the user's current checkout.
- Never start a blocked issue before every blocking PR is merged into the PRD branch.
- Never treat an opened or approved PR as merged; verify its GitHub merge state.
- Never complete an issue Task when the worker merely finishes implementation.
- Never clean up an issue worktree before its PR is merged or otherwise safely preserved by explicit user direction.
- Preserve unrelated user changes and never force-push.
- Keep the coordinator in a mailbox-draining loop while workers run.

## Phase 1: Preflight

1. Read repository agent instructions, domain docs, architecture/conventions, ADRs, the PRD, and every provided issue.
2. Support local `.scratch/<feature>/` issues now and GitHub issues when the planning workflow moves there. For GitHub, fetch full issue bodies, comments, labels, and dependency references with `gh`.
3. Inspect Git status, branches, remotes, recent commits, Orca status, Runs, worktrees, and open PRs.
4. Build the issue DAG from `Blocked by` relationships. Record the exact external issue identifier and Orca Task ID mapping.
5. A dependency is satisfied only when its PR is merged into the PRD branch. A worker report, commit, pushed branch, open PR, review approval, or green CI is not sufficient.
6. Register all issue Tasks under one Orca Run with real Task dependency IDs. Keep blocked Tasks pending.

## Phase 2: Create The PRD Branch

If orchestration starts from `main` or the repository default branch:

1. Create a clean top-level Orca worktree from the current remote default branch.
2. Create a PRD branch named consistently, such as `agent/<prd-slug>`.
3. Push it immediately with upstream tracking so issue PRs can target it on GitHub.
4. Record the PRD branch name and remote SHA as Run context.

If a PRD branch already exists:

1. Verify its local and remote identity.
2. Use or create a clean Orca integration worktree for it.
3. Fetch and fast-forward it before scheduling each new dependency wave.

Never repurpose a dirty `main` checkout as the PRD worktree.

## Phase 3: Schedule Dependency Waves

At the start of every wave:

1. Fetch GitHub PR and issue state.
2. Recompute the ready frontier from PRs verified merged into the PRD branch.
3. Start all ready issues that can reasonably run concurrently.
4. Prefer parallel work for independent modules and behavior slices.
5. Serialize ready issues only when they are expected to rewrite the same central code and parallel merge conflict cost would dominate.

For each ready issue:

1. Update its tracker state to `in-progress` without erasing dependency history.
2. Create a child Orca worktree from the latest PRD branch HEAD with `--setup run`.
3. Create one fresh OpenCode terminal using the `orca-prd-worker` profile and an installed model appropriate to the issue.
4. Wait for TUI readiness and verify the agent terminal is writable before dispatch.
5. Close a fallback shell only after `terminal list` proves it is unused.
6. Dispatch the issue through Orca with the lifecycle contract below.

Launch command:

```text
opencode --auto --agent orca-prd-worker --model <installed-model>
```

`--auto` is mandatory. The worker profile grants editing, deletion, shell, Git, GitHub, skill, and OpenCode subagent capabilities while retaining narrow protection against destructive repository loss such as force-push and hard reset.

## Model Selection

Run `opencode models` and `agy models` before assigning models. Select by reasoning need, not variety.

| Issue shape | Preferred worker |
| --- | --- |
| Cross-module architecture, difficult domain invariants, risky refactor | OpenCode `openai/gpt-5.6-terra` |
| Focused feature with clear seams and good tests | OpenCode `openai/gpt-5.6-sol` |
| Narrow mechanical change or documentation-heavy issue | OpenCode `openai/gpt-5.6-luna` |
| Hard diagnosis before implementation | OpenCode Terra, with `diagnosing-bugs` inside the worker |

The normal issue worker is OpenCode because it must load the `implement` skill and use OpenCode subagents. Antigravity (`agy`) can be used by the coordinator for supplemental planning, final PRD audit, or a second opinion:

```text
agy --dangerously-skip-permissions --mode plan --model claude-opus-4-6-thinking --effort high
agy --dangerously-skip-permissions --mode plan --model gemini-3.6-flash-high --effort high
```

Do not substitute an `agy` process for the required OpenCode issue worker unless the user explicitly changes this contract.

## Issue Worker Contract

Every Task specification must require the worker to:

1. Load the `implement` skill before editing.
2. Implement exactly the named issue against the named PRD branch/base SHA.
3. Use the `implement` skill's internal OpenCode subagents for review. Those subagents receive the same effective non-interactive permissions needed to inspect, test, and fix the issue.
4. Never create Orca workers or orchestration state.
5. Preserve unrelated files and remain within issue scope.
6. Run repository-required formatting, linting, tests, and no-network verification.
7. Inspect `git status`, the complete diff, and recent history before committing.
8. Create focused commits as needed; end with a clean worktree.
9. Push the issue branch without force.
10. Create a GitHub PR targeting the PRD branch, or update the existing issue PR.
11. Include in the PR body:
   - PRD and issue references.
   - User-visible behavior and implementation summary.
   - Important design decisions.
   - Tests and commands run.
   - Internal review findings and fixes.
   - Known limitations or residual risks.
   - Dependency and base-branch assumptions.
12. Verify the PR URL and target branch.
13. Send an Orca `merge_ready` message containing the issue ID, Task/Dispatch IDs, branch, commit SHA, PR URL, checks, review verdict, files changed, and residual risks.
14. Immediately open one durable Orca `ask` with options `merged`, `changes-requested`, and `abort`, then wait. If waiting times out, resume the same ask by message ID; never create a duplicate question.
15. Do not send `worker_done` while the PR is merely pending.
16. When the coordinator replies `merged`, verify the PR is merged into the expected PRD branch and then send `worker_done` with the PR URL and merge commit. When the reply is `changes-requested`, implement the supplied feedback, rerun internal review/checks, update the PR, send a new `merge_ready`, and open a new ask.

If internal review rejects the implementation, the same issue worker fixes it, recommits, reruns review, updates the remote branch/PR, and only then sends `merge_ready`.

## Coordinator Communication Loop

After dispatching a wave:

1. Wait on Orca messages for `merge_ready`, `escalation`, and `question`.
2. Process every message in each Delivery before acknowledging it.
3. Reply to worker questions through Orca, not ad hoc terminal text.
4. Acknowledge the Delivery and immediately continue waiting until all workers in the wave have reported or failed.
5. Treat timeout as a liveness checkpoint. Use `worker-show` and bounded `worker-read`; never duplicate a live worker.
6. Verify every reported branch, commit, clean worktree, PR URL, PR base, and checks independently with Git and `gh`.
7. Keep the Orca Task incomplete after `merge_ready`; leave the worker's durable merge-decision ask pending.

Workers must not launch review workers whose completion is invisible to the coordinator. Internal OpenCode review subagents must finish before the parent issue worker sends `merge_ready`.

## User Merge Gate

When one or more dependency-ready issue PRs have valid `merge_ready` reports:

1. Present one concise batch to the user: `X pending PRs are ready for approval.`
2. List each issue, PR URL, commit, checks, review verdict, and blocking risks.
3. Ask the user to approve which PRs may be merged. Do not merge based on silence or a previous wave's approval.
4. Keep workers and worktrees available while approval is pending.

After explicit approval:

1. Recheck PR head SHA, base branch, review/check state, and mergeability.
2. Merge each approved PR with the repository's normal non-force strategy.
3. Verify GitHub reports it merged into the PRD branch.
4. Fetch and fast-forward the PRD integration worktree.
5. Run integration checks appropriate to the merged wave.
6. Reply `merged` to that worker's pending Orca ask and include the verified merge commit.
7. Wait for and acknowledge the worker's `worker_done`; this completes the Task through the Dispatch lifecycle rather than a manual override.
8. Update the external issue to `done` or close it according to tracker policy.
9. Stop the issue worker terminal and remove its clean Orca worktree.
10. Recompute dependencies and immediately schedule the next ready wave.

If a PR is not approved, leave its Task, worker, branch, PR, worktree, and merge-decision ask pending. If the user requests changes, reply `changes-requested` to the ask with the feedback; the worker updates the same PR and sends a new `merge_ready` report.

## Failure Recovery

- Worker exits before `merge_ready`: inspect branch and PR. Retry with a fresh OpenCode session in the same issue worktree and link the retry Dispatch.
- Worker reports but PR is missing or targets the wrong base: keep Task incomplete and send corrective guidance.
- PR checks fail: route failure details to the same issue worker; require a new commit, internal review, and updated `merge_ready`.
- Merge conflict: use a fresh Terra integration worker in the issue worktree, rebase or merge the latest PRD branch without force-push unless the user explicitly permits it, rerun review/checks, and update the PR.
- Dirty worktree during cleanup: preserve it and escalate. Never force-remove uncommitted work.
- Orca restart or stale handles: re-resolve terminal handles and Dispatch state; never dual-send.

## Final PRD Completion

When every issue PR is merged:

1. Verify all Tasks and external issues are complete.
2. Verify the PRD branch contains every issue merge and matches its remote.
3. Run full formatting, linting, tests, and repository-specific verification in the PRD integration worktree.
4. Use a fresh high-reasoning audit, optionally `agy` with Claude Opus thinking, against the full PRD diff.
5. Remediate blocking final findings through a reviewed PR targeting the PRD branch and use the same user merge gate.
6. Create or update the final PR from the PRD branch to the repository default branch with a complete issue/PR/test summary.
7. Report the final PR URL and ask for final merge approval unless the user already authorized it.
8. Remove disposable issue worktrees and terminals. Retain the PRD worktree until the final PR is merged or the user asks to preserve it.

## Prohibited Failure Patterns

- Reusing one agent session across issues.
- Interactive permission prompts blocking workers.
- Separate Orca review workers for each issue when `implement` owns internal review.
- Parent workers reporting before internal OpenCode subagents finish.
- Completing Tasks when PRs are merely opened.
- Starting dependents before blocker PRs merge.
- Merging without a current user approval gate.
- Creating issue PRs against `main` instead of the PRD branch.
- Running from `main` without first creating and pushing a PRD branch.
- Leaving merged issue workers/worktrees alive.

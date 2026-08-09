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
- Only after the user approves and each PR is verified merged does the worker send `worker_done`, which authoritatively completes its Task; the coordinator then updates issue state, releases the worker, removes its worktree, and unlocks dependents.

## Ownership Boundaries

The coordinator owns:

- The Orca Run and Task DAG.
- PRD branch creation and synchronization.
- Dependency scheduling and concurrency.
- User-facing merge approval gates.
- Merging approved PRs.
- Issue completion and post-`worker_done` resource cleanup after merge.
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
2. Fetch every GitHub issue's full body, comments, labels, and native dependency relationships with `gh`. Query `repos/{owner}/{repo}/issues/{issue_number}/dependencies/blocked_by` through `gh api`; issue-body `Blocked by` text is fallback evidence, not a replacement for native edges.
3. Inspect Git status, branches, remotes, recent commits, Orca status, Runs, worktrees, and open PRs.
4. Build the issue DAG from native GitHub `blocked_by` relationships, reconciling any fallback `Blocked by` text and escalating contradictory edges. Record the exact issue number and Orca Task ID mapping.
5. A dependency is satisfied only when its PR is merged into the PRD branch. A worker report, commit, pushed branch, open PR, review approval, or green CI is not sufficient.
6. Register all issue Tasks under one Orca Run with real Task dependency IDs. Keep blocked Tasks pending.

Persist coordinator recovery metadata with a durable Run message rather than relying on mutable Run fields:

```text
ORCA orchestration send --to run:<run-id> --type status --subject "PRD orchestration state" --body "<PRD branch, remote SHA, issue-to-Task mapping, and any active repair gate>" --json
```

## Phase 2: Create The PRD Branch

If orchestration starts from `main` or the repository default branch:

1. Create a clean top-level Orca worktree from the current remote default branch.
2. Create a PRD branch named consistently, such as `agent/<prd-slug>`.
3. Push it immediately with upstream tracking so issue PRs can target it on GitHub.
4. Record the PRD branch name and remote SHA in a durable `status` message addressed to the Run.

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

1. Remove `ready-for-agent`, apply `in-progress`, and preserve dependency history.
2. Create a child Orca worktree from the latest PRD branch HEAD with `--setup run` and an explicit `--base-branch`.
3. Create one fresh OpenCode terminal using the `orca-prd-worker` profile and an installed model appropriate to the issue.
4. Wait for TUI readiness and verify the agent terminal is writable.
5. Attach the existing terminal to the Task with `orchestration worker-start --terminal`; this creates and injects the authoritative Dispatch lifecycle.
6. Close a fallback shell only after `terminal list` proves it is unused.

Launch command:

```text
opencode --auto --agent orca-prd-worker --model <installed-model>
```

`--auto` is mandatory. The worker profile uses an explicit shell-command allowlist for repository checks, non-destructive Git operations, GitHub PR work, and Dispatch messaging; unspecified shell commands remain denied even under `--auto`.

Use this custom-command path because `worker-start --agent` cannot express the required OpenCode profile and model arguments. Replace `ORCA` with the executable resolved from the live guides and use the exact IDs returned by each JSON receipt:

```text
ORCA worktree create --name <issue-slug> --parent-worktree <prd-worktree-selector> --base-branch <prd-branch> --setup run --json
ORCA terminal create --worktree id:<full-worktree-id> --title <issue-slug> --command 'opencode --auto --agent orca-prd-worker --model <installed-model>' --json
ORCA terminal wait --terminal <agent-handle> --for tui-idle --timeout-ms 60000 --json
ORCA orchestration worker-start --task <task-id> --terminal <agent-handle> --json
```

Do not dispatch separately after `worker-start`; its receipt contains the active Dispatch. If any stage fails, inspect its receipt and residual resources before retrying. Never guess an ID or duplicate a live Dispatch.

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
5. Use Dispatch-scoped `orca orchestration ask` for every blocking question; never use OpenCode's interactive question tool.
6. Preserve unrelated files and remain within issue scope.
7. Run repository-required formatting, linting, tests, and no-network verification.
8. Inspect `git status`, the complete diff, and recent history before committing.
9. Create focused commits as needed; end with a clean worktree.
10. Push the issue branch without force.
11. Create a GitHub PR targeting the PRD branch, or update the existing issue PR.
12. Include in the PR body:
   - PRD and issue references.
   - User-visible behavior and implementation summary.
   - Important design decisions.
   - Tests and commands run.
   - Internal review findings and fixes.
   - Known limitations or residual risks.
   - Dependency and base-branch assumptions.
13. Verify the PR URL and target branch.
14. Send `merge_ready` with the exact active Task and Dispatch IDs, issue ID, branch, commit SHA, PR URL, checks, review verdict, files changed, and residual risks.
15. Immediately open one durable Orca `ask` with options `merged`, `changes-requested`, and `abort`, record its message ID, and wait. If waiting times out, resume that same ask by message ID; never create a duplicate question.
16. Do not send `worker_done` while the PR is merely pending.
17. When the coordinator replies `merged`, verify the PR is merged into the expected PRD branch and then send `worker_done --outcome succeeded` with the PR URL and merge commit. When the reply is `changes-requested`, implement the supplied feedback, rerun internal review/checks, update the PR, send a new `merge_ready`, and open a new ask. When the reply is `abort`, preserve the pushed branch and PR, send `worker_done --outcome failed` with the abort reason, and stop.

Every Task specification must include the concrete issue, PRD branch, base SHA, Task ID, and these command templates. The injected Dispatch preamble supplies the Dispatch ID; the worker must substitute that exact value and the CLI executable resolved for its environment:

```text
ORCA orchestration send --type merge_ready --subject "Issue <issue-id> ready" --body "<PR URL, commit, checks, review verdict, and residual risks>" --task-id <task-id> --dispatch-id <dispatch-id> --files-modified "<csv>" --json
ORCA orchestration ask --question "Merge decision for <PR URL>?" --options "merged,changes-requested,abort" --timeout-ms 900000 --json
ORCA orchestration ask --resume <message-id> --timeout-ms 900000 --json
ORCA orchestration send --type worker_done --subject "Issue <issue-id> merged" --body "<PR URL and verified merge commit>" --task-id <task-id> --dispatch-id <dispatch-id> --outcome succeeded --files-modified "<csv>" --json
```

For `abort`, use the same `worker_done` shape with `--outcome failed`, an abort subject, and the preservation details in the body.

If internal review rejects the implementation, the same issue worker fixes it, recommits, reruns review, updates the remote branch/PR, and only then sends `merge_ready`.

## Coordinator Communication Loop

After dispatching a wave:

1. Wait on Orca messages for `merge_ready`, `escalation`, and `question`.
2. Process every message in each Delivery before acknowledging it.
3. Reply to worker questions through Orca, not ad hoc terminal text.
4. Acknowledge the Delivery and immediately continue waiting until all workers in the wave have reported or failed.
5. Treat timeout as a liveness checkpoint. Use `worker-show` and bounded `worker-read`; never duplicate a live worker.
6. Verify every reported branch, commit, clean worktree, PR URL, PR base, and checks independently with Git and `gh`. A PR is ready only when every required check is terminal and successful; pending, stale, missing, or failed required checks block the merge gate and must be resolved or escalated.
7. Keep the Orca Task incomplete after `merge_ready`; leave the worker's durable merge-decision ask pending.

Workers must not launch review workers whose completion is invisible to the coordinator. Internal OpenCode review subagents must finish before the parent issue worker sends `merge_ready`.

## User Merge Gate

When one or more dependency-ready issue PRs have valid `merge_ready` reports:

1. Present one concise batch to the user: `X pending PRs are ready for approval.`
2. List each issue, PR URL, commit, checks, review verdict, and blocking risks.
3. Ask the user to approve which PRs may be merged. Do not merge based on silence or a previous wave's approval.
4. Keep workers and worktrees available while approval is pending.

After explicit approval:

1. Immediately before each individual merge, fetch the PRD branch and recheck that PR's head SHA, base branch, required checks, review state, and mergeability. Repeat after every preceding PR changes the base.
2. Run the issue's repository checks against its current head and merge each approved PR with the repository's normal non-force strategy only after they pass.
3. Verify GitHub reports it merged into the PRD branch.
4. Fetch and fast-forward the PRD integration worktree.
5. Run integration checks appropriate to the merged wave.
6. If post-merge integration fails, do not reply to the original worker's pending merge ask; its incomplete Task remains the durable blocker for every dependent. Create a fresh repair Task and worker from the updated PRD branch, record the repair Task and affected issue in a durable `status` message addressed to the Run, and remediate through a reviewed PR and a new user merge gate. Keep the original issue `in-progress` until the repair merges and integration passes.
7. Reply `merged` to the original worker's pending Orca ask with the verified merge commit only after integration passes, either immediately or after repair.
8. Wait for the worker's `worker_done`. Process its Delivery, run `ORCA orchestration worker-release --dispatch <dispatch-id> --json`, and only then acknowledge the Delivery. The valid `worker_done` completes the Task automatically; never follow it with a manual completion update.
9. If integration passed, remove `in-progress`, apply `done`, and close the GitHub issue with the PR and merge commit. If a repair is active, perform this transition only after the repair merges and its checks pass.
10. After successful `worker-release`, verify the worktree is clean and remove it without force. Preserve and escalate any dirty worktree.
11. Recompute dependencies and immediately schedule the next ready wave only when no repair blocker remains.

If a PR is not approved, leave its Task, worker, branch, PR, worktree, and merge-decision ask pending. If the user requests changes, reply `changes-requested` to the ask with the feedback; the worker updates the same PR and sends a new `merge_ready` report. If the user chooses `abort`, reply `abort`; after the failed `worker_done`, release the worker, reset the same Task to `ready` when its dependencies remain satisfied or `pending` otherwise, remove `in-progress`, restore `ready-for-agent`, comment with the preserved branch and PR, and remove only a clean worktree. A later attempt reuses that Task so downstream dependency edges remain valid, starts a fresh Dispatch linked with `--retry-of <failed-dispatch-id>`, and receives explicit instructions about whether to update or supersede the preserved PR. Do not close the issue or PR unless the user separately requests it.

## Failure Recovery

- Worker exits before `merge_ready`: inspect branch and PR. Retry with a fresh OpenCode session in the same issue worktree and link the retry Dispatch.
- Worker reports but PR is missing or targets the wrong base: keep Task incomplete and send corrective guidance.
- PR checks fail: route failure details to the same issue worker; require a new commit, internal review, and updated `merge_ready`.
- Merge conflict: use a fresh Terra integration worker in the issue worktree, merge the latest PRD branch into the issue branch, rerun review/checks, and update the PR without force. Do not rebase a pushed branch unless the user explicitly authorizes a force-with-lease workflow.
- Dirty worktree during cleanup: preserve it and escalate. Never force-remove uncommitted work.
- Orca restart or stale handles: bind or take over the existing Run as directed by the live guide, inspect `task-list`, `dispatch-show`, the oldest unacknowledged Delivery, and pending question messages before mutating state. Re-resolve stale handles, resume an existing ask by message ID, and use `worker-show` to distinguish a live Dispatch from `failed`, `stopped`, or `outcome_unknown`. Start a replacement only when justified, link it with `--retry-of`, and never dual-send or duplicate a Run, Dispatch, Delivery acknowledgment, or question.

## Final PRD Completion

When every issue and repair PR is merged:

1. Verify all Tasks and external issues are complete.
2. Verify the PRD branch contains every issue merge and matches its remote.
3. Run full formatting, linting, tests, and repository-specific verification in the PRD integration worktree.
4. Use a fresh high-reasoning audit, optionally `agy` with Claude Opus thinking, against the full PRD diff.
5. For blocking final findings, create a fresh remediation Task, worktree, OpenCode worker, Dispatch, and PR from the current PRD branch. Apply the same review, `merge_ready`, user gate, `worker_done`, `worker-release`, verification, and cleanup lifecycle as an issue worker.
6. Create or update the final PR from the PRD branch to the repository default branch with a complete issue/PR/test summary.
7. Report the final PR URL and request explicit current approval for the final merge; issue-wave approvals do not carry forward.
8. Remove disposable issue worktrees and terminals. Retain the PRD worktree until the final PR is merged or the user asks to preserve it.

## Prohibited Failure Patterns

- Reusing one agent session across issues.
- Interactive permission prompts blocking workers.
- Separate Orca review workers for each issue when `implement` owns internal review.
- Parent workers reporting before internal OpenCode subagents finish.
- Completing Tasks when PRs are merely opened.
- Manually completing a Task after an authoritative `worker_done`.
- Starting dependents before blocker PRs merge.
- Merging without a current user approval gate.
- Creating issue PRs against `main` instead of the PRD branch.
- Running from `main` without first creating and pushing a PRD branch.
- Stopping a settled worker terminal instead of using `worker-release`.
- Leaving merged issue workers/worktrees alive.

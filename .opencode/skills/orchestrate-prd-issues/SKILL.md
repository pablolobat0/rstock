---
name: orchestrate-prd-issues
description: Use when asked to orchestrate Orca agents to implement a PRD or issue directory, especially `.scratch/<feature>/`, with isolated worktrees, commits, reviews, merges or GitHub PRs, cleanup, OpenCode models, or the `agy` Antigravity CLI.
---

# Orchestrate PRD Issues

Implement a PRD through Orca as a supervised task DAG. Every issue gets an isolated implementation worktree, a fresh worker session, a scoped commit, an independent review gate, an explicit integration outcome, and safe cleanup.

## Non-Negotiable Invariants

- Load the live `orchestration` and `orca-cli` guides before running Orca commands. Never guess the installed command surface.
- Use Orca Runs, Tasks, Dispatches, and `worker_done`; never substitute non-Orca subagents for coordinated work.
- Create one fresh implementation agent and one dedicated worktree per issue. Never reuse an agent session across issues.
- Workers must not create subagents, review workers, worktrees, Tasks, or Dispatches. Only the coordinator owns the DAG.
- Launch implementation workers with non-interactive permissions. OpenCode uses `--auto --agent orca-prd-worker`; Antigravity uses `agy --dangerously-skip-permissions --mode accept-edits`.
- Never run implementation workers in the user's current checkout. Create a PRD integration worktree first, then child issue worktrees.
- Every implementation worker must commit its own scoped changes before `worker_done`.
- A worker may send `worker_done` only after its own commands and verification have ended. It must not have nested work still running.
- Never merge or open a PR before an independent reviewer reports an explicit approval.
- Never mark an issue `done` before approval and successful integration or verified PR creation.
- Never remove an implementation worktree until its commit is safely merged into the PRD branch or pushed and referenced by a verified PR URL.
- Preserve unrelated user changes. Never stage, commit, revert, or clean files outside the issue scope.

## Phase 1: Preflight And Delivery Choice

1. Read `AGENTS.md`, `CONTEXT.md`, `docs/ARCHITECTURE.md`, `docs/CONVENTIONS.md`, relevant ADRs, the PRD, every issue file, and the local tracker conventions.
2. Inspect `git status`, current branch, recent log, Orca status, existing Runs, and existing worktrees.
3. Build the issue DAG from every `Blocked by:` line and current `Status:` value.
4. Treat a dependency as satisfied only when its issue is `done` and its commit is present in the selected base branch. Do not merely delete dependency metadata to make work appear ready.
5. Ask one decision question if the user did not choose a delivery mode:
   - `Merge to PRD branch`: review and merge each approved issue into one local PRD integration branch.
   - `GitHub PR per issue`: push each approved issue branch and open a documented PR, normally targeting the PRD branch.
6. Create a clean top-level Orca worktree for the PRD integration branch. Do not repurpose a dirty main checkout.
7. Create one Orca Run for the whole PRD. Register all issue Tasks with real dependency IDs before dispatching workers.

Use issue statuses as follows:

- `ready-for-agent`: dependency-complete and undispatched.
- `in-progress`: implementation or remediation is active.
- `done`: reviewed and integrated, or reviewed with a verified PR URL.

Append concise implementation, review, commit, PR, and blocker notes under each issue's `## Comments` section.

## Phase 2: Choose Agent And Model Deliberately

Do not rotate models arbitrarily. Match model strength and tool to the work.

| Work | Preferred launcher | Model / effort |
| --- | --- | --- |
| Cross-module architecture, domain logic, risky refactor | OpenCode | `openai/gpt-5.6-terra` |
| Complex independent review or spec audit | `agy` | `claude-opus-4-6-thinking`, `--effort high` |
| Focused implementation with clear boundaries | OpenCode | `openai/gpt-5.6-sol` or `openai/gpt-5.6-luna` |
| Fast second review, dependency audit, documentation check | `agy` | `gemini-3.6-flash-high`, `--effort high` |
| Merge conflict resolution or integration repair | OpenCode | `openai/gpt-5.6-terra` |

Before choosing, run `opencode models` or `agy models`; use only models actually installed. If `agy` is absent, use an independent OpenCode reviewer. Never claim Antigravity was used unless the `agy` process was launched and verified.

## Phase 3: Dispatch Ready Issues In Parallel

At each scheduling wave:

1. Recompute readiness from issue status and commits present in the PRD integration branch.
2. Start every ready issue whose likely file ownership does not create an avoidable integration conflict. Separate worktrees permit parallelism, but do not parallelize two large rewrites of the same central module unless the expected merge cost is accepted.
3. Create each issue worktree from the latest PRD integration commit. Use child Orca lineage under the integration worktree and `--setup run`.
4. Launch exactly one fresh worker terminal in that worktree.
5. Close the bare-create fallback shell only after `terminal list` proves which terminal is the unused shell and which is the agent.
6. Wait for TUI readiness, verify the agent terminal is writable, then inject the Dispatch.

OpenCode implementation command:

```text
opencode --auto --agent orca-prd-worker --model <installed-model>
```

Antigravity implementation command:

```text
agy --dangerously-skip-permissions --mode accept-edits --model <installed-model> --effort <low|medium|high>
```

If Orca cannot inject a lifecycle preamble into the installed `agy` TUI, dispatch for tracking without `--inject`, then send one prompt containing the exact Task spec and exact lifecycle/reporting commands. Verify the resulting Task and Dispatch with `task-list` and `dispatch-show`; do not silently downgrade to an untracked handoff.

Every implementation Task spec must state:

- The exact issue and base commit.
- Read repository guidance and the full PRD.
- Do not create or delegate to subagents.
- Preserve issue scope and unrelated files.
- Required tests and no-network constraints.
- Inspect status, diff, and recent log before committing.
- Commit only intended files with a repository-style message.
- Send exactly one `worker_done` with outcome, commit hash, files, tests, failures, and residual risks, then idle.

## Phase 4: Communication Loop

The coordinator owns communication continuously. Do not use terminal previews as completion signals.

1. After dispatching the full ready wave, call `orchestration check --wait --types worker_done,escalation,question`.
2. Process every message in the returned Delivery, not only the first one.
3. Reply to every `question` through `orchestration reply`.
4. Record every completion, verify its commit exists and its worktree is clean, then acknowledge the Delivery ID.
5. Immediately continue with `check --ack <delivery_id> --wait ...` until every expected Dispatch settles.
6. A timeout is only a checkpoint. Use `worker-show` and bounded `worker-read`; do not duplicate or replace a live worker.
7. If a worker exits without `worker_done`, inspect its branch and worktree. If a valid clean commit exists, create a recovery review Task; otherwise retry with a fresh worker linked by `--retry-of`. Never infer success from prose in the terminal.

## Phase 5: Independent Review Gate

For each completed implementation:

1. Create a fresh review worktree from the issue branch.
2. Launch a different agent/model from the implementer when available.
3. Use the read-only `orca-prd-reviewer` OpenCode profile, or `agy --mode plan` without edit authority.
4. Explicitly prohibit subagents and edits in the review Task.
5. Require findings-first output with severity, file/line references, acceptance-criteria coverage, verification gaps, and an explicit `approve` or `reject` verdict.
6. Consume and acknowledge the reviewer's `worker_done` before taking integration action.

On rejection, dispatch a fresh remediation agent into the same issue implementation worktree, require a new commit, and repeat independent review. Do not let the reviewer fix its own findings.

## Phase 6: Integrate Or Publish

### Merge To PRD Branch

1. Verify the issue branch is clean and approved.
2. Merge it into the PRD integration worktree with a non-fast-forward merge so issue provenance remains visible.
3. Resolve conflicts in the integration worktree with a dedicated Terra integration agent when necessary.
4. Run targeted checks after each merge and the full repository verification after each scheduling wave.
5. Record the issue commit and merge commit in the issue comments, then mark it `done`.
6. Recompute the DAG and dispatch newly unblocked issues from the new integration HEAD.

### GitHub PR Per Issue

1. Verify the issue branch is clean and approved.
2. Push the issue branch without force.
3. Use `gh pr create` with the intended base branch.
4. The PR body must include: issue/PRD link or path, behavior changed, files or modules affected, tests run, known limitations, independent review result, and dependency/base assumptions.
5. Verify and record the returned PR URL in the issue comments, then mark it `done` according to the user's chosen policy.
6. Do not describe a local branch as a PR.

## Phase 7: Cleanup

Cleanup is part of completion, not optional housekeeping.

1. Confirm the coordinator consumed and acknowledged `worker_done`.
2. Confirm the implementation commit is merged into the PRD branch or pushed and attached to a verified PR.
3. Stop/close the exact supervised agent terminal.
4. Remove the review worktree after its report is consumed.
5. Remove the implementation worktree after its commit is safe.
6. Keep the PRD integration worktree until the whole PRD passes final review and verification.
7. Never remove a dirty worktree. Escalate and preserve it.

## Final PRD Gate

After all issues are integrated:

1. Dispatch one fresh high-reasoning reviewer against the complete PRD branch, comparing base-to-head with the PRD and repository standards.
2. Remediate and re-review blocking findings.
3. Run the repository's formatting, lint, and complete test commands in the PRD integration worktree.
4. Verify every issue is `done`, every comment records commit/PR evidence, no Dispatch remains active, and no disposable issue/review worktree remains.
5. Report the PRD branch or final PR URL, commits, verification, residual risks, and cleanup result.

## Failure Patterns To Avoid

- One long-lived agent implementing multiple issues and accumulating context.
- Workers blocked on permission prompts.
- Empty fallback shell tabs mistaken for workers.
- Workers creating untracked nested review agents.
- Creating dependent work from an unreviewed or unmerged commit.
- Marking Tasks complete manually before `worker_done`.
- Reading terminal output instead of draining Orca Deliveries.
- Leaving completed terminals and worktrees indefinitely.
- Deleting a worktree before its commit is merged or pushed.
- Choosing models for variety rather than task complexity.

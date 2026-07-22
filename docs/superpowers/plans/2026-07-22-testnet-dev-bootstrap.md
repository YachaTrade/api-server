# Testnet Environment And Dev Branch Bootstrap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Track a secret-free GIWA testnet template and make `dev` the repository's normal pull-request target.

**Architecture:** Keep the real `.env.testnet` outside Git while committing a template under `config/`. Record branch policy in `AGENTS.md`, bootstrap it through the currently existing `main` branch, then create `dev` at the merged commit and change the GitHub default branch.

**Tech Stack:** Git, GitHub CLI, Rust/Cargo, SQLx offline metadata, PostgreSQL test harness.

## Global Constraints

- Never read, print, stage, or commit the real `.env.testnet` contents.
- Use port `8090`, app origin `https://app.yacha.trade`, cookie name `yacha-dev-api`, RPC `https://dev-node.yacha.trade`, chain ID `91342`, and R2 bucket `yacha`.
- Treat `APP_DOMAIN` and `COOKIE_NAME` as mandatory; do not rely on legacy source fallbacks.
- Use only the active `BONDING_CURVE` and `TOKEN_IMPL` contract variables.
- Omit retired NADS/V1/V2 configuration and environment-driven CORS variables.
- Normal pull requests target `dev`; `main` is promotion/release-only.
- Keep squash merge as the only enabled pull-request merge method.

---

### Task 1: Add The Safe Testnet Template And Branch Policy

**Files:**

- Create: `config/testnet.env.template`
- Modify: `AGENTS.md`

**Interfaces:**

- Consumes: environment variable names read by `src/config.rs`, `src/main.rs`, `src/db/`, authentication, metadata, and moderation modules.
- Produces: an operator-copyable template and repository-local pull-request policy.

- [x] **Step 1: Create the template**

Create `config/testnet.env.template` with approved public values, existing cache/pool defaults, and `REPLACE_ME` for every database and cloud credential component. Include no `CORS_ALLOWED_ORIGINS`, `ALLOW_CORS_PORT`, NADS product variable, or version-prefixed contract variable.

- [x] **Step 2: Update repository guidance**

Add these rules to `AGENTS.md`:

```markdown
## Branch Workflow

- Target `dev` for normal pull requests.
- Treat `main` as promotion/release-only; target it only with explicit user authorization.
- Use squash merge for pull requests.
```

Confirm the active market description remains `CURVE` and `DEX`, mapped to `Curve` and `Dex`.

- [x] **Step 3: Validate tracked and ignored files**

Run:

```bash
git check-ignore -q /Users/gyu/project/giwa/api-server/.env.testnet
git diff --check
git status --short
```

Expected: the real file is ignored; only the template, `AGENTS.md`, and approved design/plan documents are changed.

### Task 2: Validate And Publish The Bootstrap Change

**Files:**

- Validate: `config/testnet.env.template`
- Validate: `AGENTS.md`

**Interfaces:**

- Consumes: Task 1 files and initialized `migrations` submodule at gitlink `9694aa71aa9c6529a1a83569100c008d294cf3a9`.
- Produces: one reviewed commit and one squash-merged bootstrap pull request.

- [x] **Step 1: Run formatting**

Run `cargo fmt --all -- --check`.

Expected: exit 0.

- [x] **Step 2: Run tests**

Run `DATABASE_URL=postgres:///postgres SQLX_OFFLINE=true cargo test` against the authorized local disposable PostgreSQL test server.

Expected: 48 executable tests pass and two doctests remain ignored.

- [x] **Step 3: Run the release build**

Run `DATABASE_URL=postgres:///postgres SQLX_OFFLINE=true cargo build --release`.

Expected: exit 0.

- [x] **Step 4: Review and commit**

Inspect `git diff`, verify that no `.env*` runtime file or credential value is staged, then commit the scoped files with:

```bash
git commit -m "chore: bootstrap testnet config and dev workflow"
```

- [ ] **Step 5: Publish the one-time bootstrap PR**

The user explicitly authorized this one-time bootstrap in the current conversation. Record the current `main` commit, default branch, and merge settings, then push `chore/bootstrap-dev-workflow`, open a pull request targeting `main`, verify its diff and checks, and squash-merge it. This authorization does not carry over if the plan is reused later.

### Task 3: Create And Activate The Dev Branch

**Files:**

- No repository file changes.

**Interfaces:**

- Consumes: the updated remote `main` commit from Task 2.
- Produces: remote `dev` at the same commit and GitHub repository default branch `dev`.

- [ ] **Step 1: Record state and resolve the merged main commit**

Before remote writes, record `git rev-parse origin/main` and GitHub's current default/merge settings. After the bootstrap PR merges, fetch `origin` and record the new `git rev-parse origin/main`.

- [ ] **Step 2: Publish dev from updated main**

Push the exact updated `origin/main` commit to `refs/heads/dev`.

- [ ] **Step 3: Change the GitHub default branch**

Run:

```bash
gh api --method PATCH repos/YachaTrade/api-server -f default_branch=dev --silent
```

- [ ] **Step 4: Verify final repository state**

Verify that `main` and `dev` initially resolve to the same commit, the default branch is `dev`, and only squash merging is enabled.

Rollback restores the recorded prior default branch first, then reverts the bootstrap commit through a pull request. Do not delete `dev` without separate confirmation once other work may depend on it.

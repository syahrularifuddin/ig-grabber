# AI PR reviewer setup

This workflow submits GitHub App reviews after the existing **CI** workflow (`ci.yml`) succeeds for the current PR head. Existing CI and production deployment workflows are unchanged. The trusted script accepts `CI_WORKFLOW` (default `ci.yml`); workflow_run workflow names must match the CI name. Invalid model output, findings, stale heads and processing errors fail closed.
## Administrator setup (not performed by this change)

1. Create a dedicated GitHub App with repository permissions **Contents: read**, **Pull requests: read/write**, **Actions: read**. Install it only on this repository. No webhook is needed. Do not use the PR author's identity.
2. Add Actions secrets `REVIEW_APP_ID`, `REVIEW_APP_PRIVATE_KEY` (the generated PEM), and `LLM_API_KEY`. Current VPS secrets do not satisfy these requirements. Never paste keys into source, issues, chat or logs.
3. Optional repository Actions variables: `LLM_BASE_URL` (default `https://midas-stage.telkomdigital.id/karpathy`) and `LLM_MODEL` (default `dpe/alias/deepseek-flash`). The script appends `/chat/completions` to the base, sends Bearer authentication and requests OpenAI-compatible JSON chat completions. Compatibility of this endpoint, including `response_format`, has **not been verified**; if the provider requires `/v1`, configure that in the base URL. No OpenCode configuration or credentials are read.
4. Merge these trusted workflow/script files to the default branch. `workflow_run` uses that branch, not PR code. For bootstrap, dispatch **AI PR Review** with PR number and full current head SHA; the script also requires a successful current-head CI run. Dispatch from the default branch only.
5. In branch protection/rulesets require **AI Review** (commit status, emitted by GitHub Actions) and the existing CI check separately. Replace the required human approval count with zero if adopting the authorized AI-only gate; review CODEOWNERS and other rules for conflicting approval requirements. App reviews are informational, not the merge gate. Do not grant the App Contents write or status write. The job uses the default `GITHUB_TOKEN` with `statuses: write`, passed separately as `STATUS_TOKEN`; the App retains Contents read, Pull requests write and Actions read only. No remote settings are changed by this patch.
6. Bootstrap: this PR cannot install its own trusted reviewer. Only merge through the authorized process after provisioning the secrets and verifying the App installation. Then dispatch from `main` for a real current-head CI-tested PR before requiring `AI Review`. Never enable a nonexistent required status or dispatch an untrusted branch.

## Current activation blockers

Repository secret-name inspection found none of `REVIEW_APP_ID`, `REVIEW_APP_PRIVATE_KEY`, or `LLM_API_KEY`. Existing VPS secrets cannot be reused. GitHub secret values cannot be retrieved or copied. App installation could not be verified with the current OAuth token (installation APIs require App credentials). Supply these three secrets securely and confirm the App is installed on this repository with the permissions above. No merge, protection, deployment, or Actions permission settings were changed. Keep this implementation unmerged until prerequisites are available.


## Security and limits

Only trusted default-branch scripts execute. No PR checkout, dependency install, artifacts, cache, model tools or generated code execution occurs. The scoped App token is short-lived; credentials are not sent to the model. Diff and limited immutable file contexts are sent to the configured external provider, so obtain authorization to share repository source and review the provider's retention policy.

PR text is explicitly treated as untrusted data. Prompt injection resistance is best-effort, not a guarantee; schema validation cannot prove review quality. Human oversight remains necessary. Binary, renamed/copied, missing/truncated patches, more than 80 changed files, or diffs over 100 KB fail closed. Context is optional and bounded; this is not a whole-repository analysis. Errors log status/reason, not response bodies or keys.

The repository-wide concurrency lock serializes automatic/manual reviews, with a final CI/head recheck immediately before submission and a commit-bound review. GitHub has no atomic compare-head-and-review API: a push can race that final check, so stale-approval dismissal is essential. Duplicate markers suppress repeat submission only for an identical clean approved review; rejecting, dismissed, edited or unverifiable existing reviews fail closed. Retries still validate a fresh strict model decision and current CI. There are no merge, deploy or protection changes.

Local validation: `node --test .github/scripts/llm-review.test.mjs` and `node --check .github/scripts/llm-review.mjs`. These are schema and mocked status/CI tests, not live endpoint/App integration tests.

Production deployments remain separate from reviews: a bot approval does not merge a PR or deploy it.

## Required status semantics

`AI Review` is posted on the exact PR head: pending before review, success only after a strict zero-finding approval and final current-head/CI verification, failure on findings or processing errors. Failed/cancelled CI completion events also run the reviewer and invalidate an existing success when the latest CI attempt is not successful. CI remains separately required because reruns can start after a successful review, and GitHub offers no atomic CI/head/status transaction. Status-write errors fail the job; if GitHub refuses writes, the script cannot overwrite a previous status, so operators must inspect failed reviewer jobs and retain the independent required CI check.

The required status context is named exactly `AI Review`.

The required AI review check context is exactly `AI Review`.

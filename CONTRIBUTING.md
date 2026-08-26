# Contributing to RustFRP

## Branch model

`dev` is the default integration branch. Create short-lived branches from the latest `dev` and open pull requests back to `dev`. Suggested prefixes are `feat/`, `fix/`, `docs/`, `refactor/`, `test/`, `ci/`, and `chore/`.

`main` contains released code only. The sole normal path into `main` is a pull request from `dev`. After that pull request passes its required checks and is merged, a maintainer may create a signed or annotated `vMAJOR.MINOR.PATCH` tag on the resulting `main` commit. Tags trigger the release workflow.

```text
feat/* ──PR──> dev ──release PR──> main ──vX.Y.Z tag──> GitHub Release
```

Do not commit feature work directly to either long-lived branch. Hotfixes also start from `dev`; if an emergency fix must start from `main`, merge the same commit back into `dev` immediately after release.

## Pull requests

- Keep each PR focused on one independently reviewable outcome.
- Use a Conventional Commit PR title, such as `feat(import): support frpc TOML`.
- Prefer squash merge for feature branches so the PR title becomes the commit subject.
- Require review of owned paths and resolve all review conversations.
- Do not mix schema changes, broad refactors, and unrelated features.
- Add tests for behavior changes and document operational or migration impact.

## Local quality gate

Run these checks before pushing:

```bash
just lint
just test-all
cd plugins/webui && bun install --frozen-lockfile
cd plugins/webui && bun x tsc --noEmit && bun scripts/check-i18n-keys.ts && bun run build
```

CI repeats the formatting, Clippy, Rust test, WebUI type, i18n, and build checks.

## Recommended GitHub branch protection

GitHub settings cannot be committed to this repository, so an administrator must configure rulesets:

- `dev`: require pull requests, one approval, CODEOWNERS review, resolved conversations, linear history, and successful `Rust quality gate`, `WebUI quality gate`, `Branch policy`, and `PR Title` checks; block force pushes and deletion.
- `main`: apply the same rules, restrict the PR source to `dev` through the `Branch policy` check, and block direct pushes, force pushes, and deletion.
- Set `dev` as the repository default branch.
- Limit tag creation matching `v*` to release maintainers.

The workflow checks are defense in depth; rulesets are the mechanism that actually prevents merges and direct pushes.

## Releases

1. Update the workspace version and `CHANGELOG.md` on `dev`.
2. Open a release PR from `dev` to `main` and wait for all required checks.
3. Merge without adding unrelated commits to `main`.
4. Create `vMAJOR.MINOR.PATCH` on that exact `main` commit and push the tag.
5. Verify artifacts and checksums in the generated GitHub Release.

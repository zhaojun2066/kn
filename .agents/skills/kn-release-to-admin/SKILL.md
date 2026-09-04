---
name: kn-release-to-admin
description: Prepare a requested KN desktop version, watch its GitHub Actions build, and create or explicitly publish the matching Admin release.
---

# KN release to Admin

Use this skill to release a requested KN desktop version end-to-end: version preparation, GitHub candidate build, Admin draft, and an explicitly approved public release. It does not deploy Cloud, Admin, or the website.

## First use

If the local release configuration or Keychain password is absent, explain the required values and follow [the setup reference](references/setup.md). It keeps the Admin URL and email in `~/workspace/me/miyao/admin/release-publish.json`, while the password stays in macOS Keychain. Never put credentials in this repository, a release note, or command output.

## Invocation

The user invokes this skill in conversation, for example:

```text
$kn-release-to-admin 升级并发布 v1.2.8。
```

## Prepare the requested version

When the user supplies a target version, use it directly; do not ask them to edit files or run scripts.

1. Confirm the target is `x.y.z`, greater than the root `Cargo.toml` workspace version and every existing `v*` tag, and that the worktree is clean on `main`.
2. Change only the root `Cargo.toml` `[workspace.package] version`. Run the release checks listed in `RELEASE.md`.
3. Show the version, release notes preview, intended commit, and tag. Obtain one explicit confirmation before committing, tagging, and pushing.
4. Commit `release: v<version>`, create annotated tag `v<version>`, and push `main` and the tag without force.

## Build, upload, and publish

After the tag is pushed, run the bundled helper with the exact semantic version without the `v` prefix. It polls the tag's `Build Desktop App` run every 30 seconds. Each update shows job completion, elapsed time, job state, and an estimate computed from `expectedBuildMinutes` in the local configuration. It stops after `maxWaitMinutes` without cancelling the GitHub run. It exits without an Admin mutation if the workflow fails, is cancelled, times out, or its artifact is incomplete.

After reporting a successful draft and the required ARM/Intel acceptance outcome, wait for explicit user approval. A user can then say:

```text
草稿 v1.2.0 已验收，请公开发布。
```

Only then run the helper with `--publish --confirm-publish v<version>`. The helper verifies the existing draft against the GitHub artifact, publishes it, and reads the release list back to confirm `published` status.

## Guardrails

- The version must be `x.y.z`; artifact name, tag, `Cargo.toml`, Admin draft version, and returned minimum protocol version must agree with local expectations.
- Require exactly one ARM and one Intel DMG plus a non-empty `release-notes.md`.
- Use the GitHub Actions artifact, not arbitrary local DMGs or GitHub Release assets.
- Stop when a same-version Admin record exists; existing published files are never overwritten.
- A failed or timed-out build, download, login, upload, or verification stops the workflow. A created draft remains available for inspection; automatic cleanup and retries do not publish it.
- Treat public publication as a distinct, explicitly approved action. Do not pass `--publish` unless the user has just approved the displayed draft details.

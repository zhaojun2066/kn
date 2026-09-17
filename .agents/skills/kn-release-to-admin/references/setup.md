# One-time local setup

Create `~/workspace/me/miyao/admin/release-publish.json` with non-secret values:

```json
{
  "adminUrl": "https://admin.example.com",
  "adminEmail": "release@example.com",
  "githubRepo": "owner/kn",
  "expectedBuildMinutes": 30,
  "maxWaitMinutes": 90,
  "pollSeconds": 30,
  "keychainService": "kn.release-publish.admin"
}
```

`githubRepo` is optional when `gh repo view` can identify the current repository. `expectedBuildMinutes` is a local progress baseline, not a promise from GitHub. `maxWaitMinutes` stops waiting safely when Actions appears stalled; it does not cancel the GitHub run. The helper does not use Agent protocol levels as a release gate. Admin may retain `minProtocolVersion` in legacy Desktop-update responses, but it is not Agent WSS admission control and must not be used to reject an otherwise matching release.

Store the Admin password in macOS Keychain, replacing the email, service name, and password placeholders before executing:

```bash
security add-generic-password -U -s 'kn.release-publish.admin' -a 'release@example.com' -w '<Admin password>'
```

The helper reads the password with `security`, keeps the returned bearer token in memory only, and sends it over the configured HTTPS Admin URL. The local configuration file should be owner-readable only:

```bash
chmod 600 ~/workspace/me/miyao/admin/release-publish.json
```

Ensure `gh auth status` reports an account authorized to read Actions artifacts for the configured repository.

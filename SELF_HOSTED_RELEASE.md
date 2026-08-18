# Self-Hosted Release Contract

GitHub Actions only produces the signed/notarized `release-candidate-v<version>` artifact. It never uploads packages or creates a GitHub Release.

## Required server settings

```text
KN_RELEASE_ROOT=/opt/kn-cloud/releases
KN_RELEASE_DOWNLOAD_BASE_URL=https://api.knshark.com/releases
```

Nginx serves `/opt/kn-cloud/releases/` read-only at `/releases/`; the matching location is in `kn-cloud/deploy/nginx.conf`.

## Publication flow

1. Merge main and tag the desktop version, for example `v1.2.0`.
2. Download the ARM and Intel DMGs from the Actions artifact and verify notarization.
3. Open kn-admin and enter the version, Agent version, protocol version and Release Notes.
4. Upload both DMGs. The Admin API calculates SHA-256 on the server and stores the hashes with the release row.
5. Test ARM and Intel hardware: first install, binding, iOS input/output, update Agent replacement and self-unbind.
6. Publish the draft pointer, verify API, site links and in-app update, then observe for 24 hours.

The desktop app compares SemVer, downloads only the current architecture package, calculates SHA-256 locally and opens the DMG only after the hash matches the value returned by Cloud.

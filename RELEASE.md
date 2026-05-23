# Release procedure

How to ship a new wormhole-gui version end-to-end. The GitHub Actions
workflow ([release.yml](.github/workflows/release.yml)) does the heavy
lifting; this doc covers the one-time setup and the per-release commands.

## One-time GitHub setup

These steps only need to happen once for the repo. Skip if already done.

### 1. Rename the repo (if not already)

If the repo is still at `lemond-dev/chat_one`, rename it to
`lemond-dev/wormhole-gui` via **Settings → General → Repository name**.
GitHub auto-redirects the old URL so existing clones won't break.

### 2. Add the signing private key as a Secret

Create the secret in **Settings → Secrets and variables → Actions →
New repository secret**:

| Name | Value |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | Contents of `%USERPROFILE%\.tauri\wormhole-gui.key` (the file generated in Phase 4-B; whole multi-line base64 blob, no surrounding whitespace) |

Keep an offline backup of the file too — if this secret is ever lost
and the local copy is also gone, every installed client will reject
future updates and users will have to manually reinstall.

### 3. Enable GitHub Pages

In **Settings → Pages**:

- Source: **Deploy from a branch**
- Branch: **`gh-pages`** / **`/ (root)`**

The branch doesn't exist yet — the first time the release workflow
runs, it creates `gh-pages` automatically (orphan branch with just
`latest.json`). After that first run, return here and confirm the
branch is selected.

### 4. Verify Actions permissions

In **Settings → Actions → General → Workflow permissions**:
- **Read and write permissions** must be enabled, OR
- `permissions: contents: write` must remain declared in the workflow
  (it already is — leave it alone).

## Per-release procedure

1. **Confirm the working tree is clean** and on `main`:

   ```bash
   git status
   ```

2. **Bump the version** in three places to match the tag you're about
   to push:
   - `wormhole-gui/Cargo.toml` → `[workspace.package] version`
   - `wormhole-gui/tauri-app/src-tauri/tauri.conf.json` → `"version"`
   - `wormhole-gui/tauri-app/package.json` → `"version"`
   - `wormhole-gui/tauri-app/src/lib/screens/Settings.svelte` → `VERSION`
     constant

3. **Commit the bump**:

   ```bash
   git add -A
   git commit -m "release: v0.3.x"
   ```

4. **Tag with annotated message**. The first line of the tag message
   becomes the release notes shown in the update banner — keep it short
   (one sentence):

   ```bash
   git tag -a v0.3.x -m "Fix relay reconnect race; add Japanese UI."
   git push origin main
   git push origin v0.3.x
   ```

5. **Watch the workflow** at
   [github.com/lemond-dev/wormhole-gui/actions](https://github.com/lemond-dev/wormhole-gui/actions).
   First run takes ~10 min (cold cargo cache); subsequent runs ~5 min.

6. **Verify** once it's green:
   - The Release page lists 4 files: `wormhole-gui.exe`,
     `wormhole-gui.exe.sig`, `wormhole-gui_<ver>_x64-setup.exe`,
     `<setup>.sig`
   - `https://lemond-dev.github.io/wormhole-gui/latest.json` is reachable
     and lists the new version
   - Open the previous installed build → within ~10 seconds of startup
     the banner shows "🔄 发现新版本 v0.3.x"

## Rolling back a bad release

GitHub Pages serves whatever `latest.json` is on `gh-pages` HEAD. To
roll back without yanking the Release:

```bash
git fetch origin gh-pages
git checkout gh-pages
git revert HEAD       # or hard-reset to a known-good commit
git push origin gh-pages
```

Within minutes, new `check_update` calls will see the previous
manifest again.

## Local build (development only)

Local builds intentionally fail at the signing step unless
`TAURI_SIGNING_PRIVATE_KEY` is exported — production signing should
only happen in CI. The exe and setup.exe are produced before the
sign step fails, so they're usable for local UI testing.

To sign locally for testing the updater flow end-to-end against your
own gh-pages branch:

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content "$env:USERPROFILE\.tauri\wormhole-gui.key" -Raw
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""
cd wormhole-gui/tauri-app
pnpm tauri:build
pnpm tauri signer sign ../target/release/wormhole-gui.exe
```

## Key rotation

If the signing key is ever suspected compromised:

1. Generate a new one: `pnpm tauri signer generate -w ~/.tauri/wormhole-gui-v2.key --password ""`
2. Replace the public-key file at `wormhole-gui/tauri-app/src-tauri/keys/updater.pub`
   with the new `.pub` content
3. Update `tauri.conf.json` `plugins.updater.pubkey` to the new value
4. Replace the GitHub Secret with the new private key
5. Cut a new release — any **already installed** client signed with the
   old key will reject the new release because the embedded pubkey changed,
   so users have to **manually reinstall once**. This is by design: a
   compromised key means we can't trust the auto-update channel anymore.

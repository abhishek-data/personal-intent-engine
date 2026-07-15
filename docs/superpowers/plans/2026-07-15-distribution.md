# Distribution / Installable Builds — Short Plan

**Goal:** Make PIE downloadable and installable like Handy — a GitHub Actions release pipeline that builds macOS (`.dmg`/`.app`) and Windows (`.msi`/`.exe`) installers on tag push and publishes them to a GitHub Release. Unsigned for now.

**Decisions (from the user):** Unsigned first (no notarization/Authenticode this round). Both platforms via CI now; the Windows CI build itself reveals any port work, handled as follow-up. Homebrew cask / winget deferred until signed releases exist.

**Repo facts:** Tauri 2 app in `src-tauri/` (config `src-tauri/tauri.conf.json`, `productName: PIE`, `identifier: com.pie.desktop`, version `0.1.0`); Cargo workspace rooted at repo root, build output in `target/`; whisper/VAD deps already gated per-platform (Metal on macOS, CPU elsewhere); `icon.icns` + `icon.ico` both present; frontend in `ui/` built via `beforeBuildCommand`. GitHub remote: `abhishek-data/personal-intent-engine`. No CI yet.

**Out of scope:** signing/notarization, Homebrew/winget, fixing Windows compile/runtime issues (only reported if CI surfaces them — a separate port effort), auto-updater.

---

### Task 1: Bundle config — produce a `.dmg` (and platform installers)

**Files:** Modify `src-tauri/tauri.conf.json`.

- [ ] **Step 1:** In `src-tauri/tauri.conf.json`, change the bundle `targets` from `["app"]` to `"all"`:

```json
  "bundle": {
    "active": true,
    "targets": "all",
    "macOS": {
      "minimumSystemVersion": "11.0"
    },
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  }
```

`"all"` builds every bundle type valid for the current OS — `app` + `dmg` on macOS, `nsis` (.exe) + `msi` on Windows — so the same config works on both CI runners.

- [ ] **Step 2:** Verify locally on macOS that a `.dmg` is produced. The Rust release binary is already built and cached, so this only re-runs bundling:

Run: `npm run build --prefix ui && cargo tauri build --bundles dmg 2>&1 | tail -20`
(If `cargo tauri` is unavailable, use `npx --prefix ui @tauri-apps/cli build --bundles dmg`, or `npm run tauri build -- --bundles dmg` from `src-tauri` if a script exists.)
Expected: a `.dmg` appears under `target/release/bundle/dmg/`. Confirm with `ls target/release/bundle/dmg/`.

- [ ] **Step 3:** Commit:

```bash
git add src-tauri/tauri.conf.json
git commit -m "build: bundle all installer targets (dmg on macOS, exe/msi on Windows)"
```

---

### Task 2: Release workflow + install docs

**Files:** Create `.github/workflows/release.yml`, create `docs/RELEASING.md`, modify `README.md`.

- [ ] **Step 1:** Create `.github/workflows/release.yml` with exactly:

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'
  workflow_dispatch:

permissions:
  contents: write

jobs:
  release:
    strategy:
      fail-fast: false
      matrix:
        platform: [macos-14, windows-latest]
    runs-on: ${{ matrix.platform }}

    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Set up Node
        uses: actions/setup-node@v4
        with:
          node-version: 20

      - name: Set up Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache Rust build
        uses: Swatinem/rust-cache@v2

      - name: Install frontend dependencies
        run: npm install --prefix ui

      - name: Build app and publish release
        uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        with:
          tagName: ${{ github.ref_name }}
          releaseName: 'PIE ${{ github.ref_name }}'
          releaseBody: 'Download the installer for your platform below. Builds are unsigned — see the README for first-launch instructions.'
          releaseDraft: true
          prerelease: false
```

Notes for the implementer: `macos-14` is Apple-Silicon (arm64); `windows-latest` is x64. No `projectPath` is set — `tauri-action` auto-detects `src-tauri/`, and the UI is built by the config's `beforeBuildCommand` after the explicit `npm install --prefix ui` step. `releaseDraft: true` creates a draft release to review before publishing.

- [ ] **Step 2:** Create `docs/RELEASING.md` with exactly:

```markdown
# Releasing

Releases are built and published by GitHub Actions (`.github/workflows/release.yml`).

1. Bump the version in `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml` (keep them in sync).
2. Commit the bump.
3. Tag and push:

   ```bash
   git tag v0.1.0
   git push origin v0.1.0
   ```

4. The workflow builds macOS (`.dmg`/`.app`) and Windows (`.msi`/`.exe`) installers and creates a **draft** GitHub Release with the artifacts attached. Review and publish it from the Releases page.

## Notes

- Builds are **unsigned**. macOS users must right-click → **Open** the first time; Windows users click **More info → Run anyway** past SmartScreen.
- To trigger a build without tagging, run the workflow manually from the **Actions** tab (`workflow_dispatch`).
- First Windows CI build is unverified — if it fails to compile, that is the "Windows support" port work, tracked separately.
```

- [ ] **Step 3:** In `README.md`, add an `## Installation` section immediately before the `## How it's built` heading (line ~57):

```markdown
## Installation

Download the latest installer from the [releases page](https://github.com/abhishek-data/personal-intent-engine/releases):

- **macOS** — download the `.dmg`, open it, and drag PIE to Applications. The build is currently unsigned, so the first time you launch it, right-click the app and choose **Open** to get past Gatekeeper.
- **Windows** — download and run the `.exe` installer. Windows SmartScreen may warn about an unrecognized app; click **More info → Run anyway**.

> Builds are not yet code-signed or notarized. Homebrew cask / winget packages may follow once signed releases are available.

Prefer to build it yourself? See [Quick start](#quick-start-desktop-app) below.
```

- [ ] **Step 4:** Validate the workflow YAML parses (no syntax errors):

Run: `python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/release.yml')); print('YAML OK')"`
Expected: `YAML OK`.

- [ ] **Step 5:** Commit:

```bash
git add .github/workflows/release.yml docs/RELEASING.md README.md
git commit -m "ci: release workflow + install docs for macOS and Windows"
```

---

## Verification summary

- Local: `.dmg` is produced under `target/release/bundle/dmg/` (Task 1) — proves the macOS installer half end-to-end.
- YAML parses (Task 2).
- Windows CI + the actual GitHub Release can only be verified by pushing a `v*` tag (requires the user's go-ahead to push). That is the handoff, not part of local execution.

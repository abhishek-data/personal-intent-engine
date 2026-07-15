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

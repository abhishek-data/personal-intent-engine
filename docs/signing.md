# macOS code signing & why the cert must never change

PIE's macOS builds are signed with a **stable, self-signed certificate**
(`CN=PIE Developers`). This is not for Gatekeeper — a self-signed cert does not
notarize, so first-launch still needs the quarantine strip (`xattr -cr`, done by
the install script). It exists for **one reason: keeping the user's permission
grants across updates.**

## The problem it solves

To paste at your cursor and to record audio, PIE needs macOS **Accessibility**
and **Microphone** permission. macOS stores those grants in TCC, keyed to the
app's code-signing **Designated Requirement**:

```
identifier "com.pie.desktop" and certificate leaf = H"d318e19eaf8d165d5d5e16cd7d1817e7d8d4d854"
```

That `certificate leaf` hash is the SHA-1 of the signing certificate. As long as
**every** release is signed with the **same** cert, the DR is identical across
versions, so a user grants permission once and it survives every update.

If a build is signed with a **different** identity — a regenerated `.p12`, or an
**ad-hoc** fallback when the signing secret is missing — the DR changes. macOS
then treats the update as a different app: the old grant no longer matches, the
"PIE ✓" toggle in System Settings silently goes stale, and the user is
re-prompted (and paste fails) until they reset and re-grant:

```bash
tccutil reset Accessibility com.pie.desktop
tccutil reset Microphone    com.pie.desktop
# then relaunch PIE and grant once
```

## The pinned certificate

| Field       | Value                                                       |
|-------------|-------------------------------------------------------------|
| Common name | `PIE Developers` (self-signed; `TeamIdentifier=not set`)    |
| Leaf SHA-1  | `d318e19eaf8d165d5d5e16cd7d1817e7d8d4d854`                   |
| Valid       | 2026-07-20 → 2036-07-17                                      |

The cert (as a base64 `.p12`) and its password live in the repo's GitHub Actions
secrets, `APPLE_CERTIFICATE` and `APPLE_CERTIFICATE_PASSWORD`. The
`.github/workflows/release.yml` "Import macOS signing certificate" step imports
them into a temporary keychain and signs with the `PIE Developers` identity.

**Do not regenerate this `.p12` to "refresh" it.** A regenerated cert = a new
leaf hash = a broken grant for every existing user. Keep a secure backup of the
`.p12` and password outside GitHub so the identity is never lost.

## The CI guard

`release.yml` has a **"Verify macOS signing identity is the pinned cert"** step
that runs after the build and extracts the built app's leaf certificate:

- **Signed with the pinned cert** → passes.
- **Signed with a different cert** (regenerated `.p12`) → **hard fails** the run.
  This is the case that would silently break every user, so it must never ship
  by accident.
- **Ad-hoc** (secret absent) → emits a loud `::error::` annotation but **still
  releases**, matching the deliberate ad-hoc fallback in the import step.

The pinned value lives in one place: `EXPECTED_LEAF_SHA1` in `release.yml`.

## If the cert genuinely must change

Rotating the identity is a **breaking change** for every existing user. If it is
unavoidable (cert lost, expired, compromised):

1. Generate the new cert and update the `APPLE_CERTIFICATE` /
   `APPLE_CERTIFICATE_PASSWORD` secrets.
2. Update `EXPECTED_LEAF_SHA1` in `release.yml` **and** the table above in the
   **same commit**.
3. Call it out in the release notes: users must reset and re-grant Accessibility
   and Microphone once after updating (the `tccutil reset` commands above).

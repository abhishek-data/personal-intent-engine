# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

<!-- Tauri 2 desktop app. The UI is web technology (Svelte 5 + plain CSS) rendered
in a system webview, shipped as a macOS and Windows desktop application. It is not
a native iOS/Android surface, but it must honor desktop OS conventions on both
macOS and Windows. -->

## Users

Developers who drive AI coding tools — Claude Code, Cursor, ChatGPT, and similar —
and who dictate rather than type their prompts.

The usage scene is specific and load-bearing: the user's hands are on the keyboard,
**another application is focused**, and PIE is not on screen. They press a global
hotkey, speak, and the result is pasted at their cursor in that other app. The PIE
window is a place they visit occasionally to configure, review, or teach; the
floating overlay is what they actually see during real use.

## Product Purpose

PIE (Personal Intent Engine) is middleware between a person's voice and an AI model.
It transcribes speech entirely on-device, repairs mangled technical vocabulary,
extracts the underlying intent, and emits a structured prompt into the user's active
text field — or routes it to a configured LLM.

Success is that the loop disappears: hotkey → speak → correct, usable prompt at the
cursor, without the user opening PIE at all.

## Positioning

Three things together, none of which a neighboring dictation tool has all of:

1. **Intent extraction** — rambling speech becomes a structured, production-ready
   prompt, not a verbatim transcript. Three optimization modes as shipped in
   the UI: `auto`, `direct`, `enhanced`. (`README.md` still advertises an older
   four-mode set — `compact` / `balanced` / `enhanced` / `adaptive`. The UI is
   the current truth; the README is stale.)
2. **A jargon corrector that learns** — developer terms other dictation tools mangle
   (`next jazz` → `Next.js`, `coobernetes` → `Kubernetes`) are fixed in layers:
   a built-in dictionary, the user's own `heard → correct` vocabulary, a
   context-gated phonetic tier, and an opt-in LLM deep-correct pass. Corrections
   are always shown (`heard → corrected`); nothing changes silently. Teaching it
   once makes the fix instant and offline forever after.
3. **100% local speech** — audio and transcription never leave the machine
   (whisper.cpp with Metal acceleration, Silero VAD).

All three must stay legible in the product. Privacy is not a footnote and intent
extraction is not a hidden implementation detail.

## Operating Context

- **Trigger:** two global hotkeys — one pastes the raw transcript
  (default `CmdOrCtrl+Shift+V`), one pastes the optimized prompt
  (default `CmdOrCtrl+Shift+Space`). Both are rebindable by pressing a combo.
- **During recording:** a small floating overlay appears over whatever app is
  focused, showing recording / transcribing state. `Escape` cancels.
- **Output:** pasted at the cursor in the focused app, or copied, or sent to a
  configured LLM.
- **Control panel surfaces:** four tabs — Record, History, Models, Settings.
  Settings contains transcription, LLM provider, output, two hotkey recorders,
  vocabulary editing, vocabulary sync, and history retention.
- **First run has real friction:** the user must download a Whisper model and the
  Silero VAD model from the Models tab, and grant OS Microphone and Accessibility
  permissions. Nothing works before that.
- **Storage:** settings at `~/Library/Application Support/pie/settings.json`,
  vocabulary in `pronunciation.json`, recording history in a local SQLite store.

## Capabilities and Constraints

**Confirmed capabilities**

- On-device STT (whisper.cpp), voice activity detection (Silero VAD).
- Pronunciation correction: dictionary + personal vocabulary + context-gated
  phonetic tier + opt-in AI deep-correct, with a "Re-correct with AI" action on any
  result and one-tap save of a fix into vocabulary.
- Intent extraction and prompt optimization in three modes (`auto`, `direct`,
  `enhanced`).
- Vocabulary import from past AI conversations (Cursor history, ChatGPT/Claude
  exports, or a folder of text), plus opt-in background mining of new
  corrections from your own transcripts, and a "code mode" that translates
  spoken code.
- Optional routing to OpenAI or any OpenAI-compatible endpoint.
- In-app model download, selection, and deletion with progress.
- Local recording history with a configurable retention limit.
- Menu-bar tray presence plus a floating recording overlay.
- Also ships as a CLI (`pie-cli`) and a reusable Rust crate (`pie-engine`).

**Durable constraints**

- **Tray app, not a window app.** PIE lives in the menu bar. The main window is
  occasional; the overlay is the primary in-use surface. Design weight should
  follow that, not the other way around.
- **macOS and Windows are both first-class.** macOS 11+ Apple Silicon is the
  currently tested platform, but the redesign must improve both. Nothing may
  depend on a macOS-only visual affordance without a real Windows counterpart.
- **No new runtime dependencies.** UI stays Svelte 5 + plain CSS. No UI kit, no
  animation library, no icon package. Anything expressive is hand-built.
- **Local-first, zero telemetry.** No analytics. Network requests only to a
  user-configured LLM provider, only on explicit action. No UI element may imply
  otherwise.
- **CSP is strict:** `default-src 'self'; connect-src 'self'; img-src 'self' data:;
  style-src 'self' 'unsafe-inline'`. No remote fonts, scripts, or images.
- Window is small and resizable: 540×660 default, 460×560 minimum, with an overlay
  macOS title bar and hidden title.

## Brand Commitments

- Name: **PIE — Personal Intent Engine**. Window title `PIE — Personal Intent Engine`.
- Existing icon at `assets/icon.png` and platform icon set in `src-tauri/icons/`.
- Apache 2.0 licensed; NOTICE credits derived work from Handy (MIT) and
  OpenSuperWhisper (MIT). This attribution is legally required and must survive.
- No confirmed color, typographic, or personality commitment beyond the icon —
  the visual world is open.

## Evidence on Hand

- Real product copy in `README.md` (feature list, privacy claims, install flow).
- Real architecture record in `AGENTS.md` and `docs/ARCHITECTURE.md`.
- Real UI implementation: `ui/src/App.svelte`, `ui/src/Overlay.svelte`,
  `ui/src/app.css`, and ten components in `ui/src/lib/`.
- Real assets: `assets/icon.png`, `src-tauri/icons/`.
- **No** testimonials, user counts, benchmarks, press, pricing, or case studies
  exist. Future work must not fabricate them.
- Builds are self-signed, **not** Apple-notarized. Do not claim notarization.

## Product Principles

1. **The best session is the one where PIE is never seen.** Optimize the hotkey →
   overlay → paste loop before optimizing the control panel.
2. **Never correct silently.** Every transformation — jargon fix, intent
   extraction — stays inspectable, because trust in an invisible tool is the
   whole product.
3. **Local by default, remote only on request.** Anything that leaves the machine
   is an explicit, visible, opt-in act.
4. **It learns and keeps what it learns.** Teaching PIE a term once must feel
   permanent and cheap.
5. **First run must survive its own friction.** Model downloads and OS permissions
   are unavoidable; the product's job is to make that sequence obvious rather than
   pretend it isn't there.

## Accessibility & Inclusion

No product-specific standard was established by the user. Baseline expectations
apply: the overlay must be legible over arbitrary application backgrounds, state
must never be signaled by color alone (recording vs. transcribing), and the control
panel must remain fully keyboard-operable — the audience is keyboard-primary by
definition.

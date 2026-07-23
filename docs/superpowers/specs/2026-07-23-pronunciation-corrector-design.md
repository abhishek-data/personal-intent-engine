# Pronunciation Corrector — Design

> Date: 2026-07-23
> Sub-project B of the PIE v2 roadmap (`BRAINSTORM_V2_ROADMAP.md`, Enhancement 1).
> Status: approved design, ready for implementation planning.

## Problem

Whisper transcribes general speech well but butchers developer jargon:
"Next.js" → "next jazz", "Nginx" → "engine X", "Kubernetes" → "coobernetes".
Generic OS dictation has the same weakness and never improves for a specific
user. PIE's differentiator is a correction layer that fixes these terms — using
a shipped dictionary, the user's own additions, and (opt-in) the LLM PIE already
routes to — and gets more accurate for *you* over time.

This sub-project delivers the correction layer itself. Automatic learning from
post-paste edits (roadmap Enhancement D) is explicitly out of scope; a small
user-confirmed learning bridge is included.

## Decisions locked in

| Decision | Choice | Why |
|----------|--------|-----|
| Engine strategy | **Hybrid**: instant local dictionary always on; opt-in LLM deep pass | Correction runs on every transcript inline before paste; the always-on path must stay microsecond-fast and offline. |
| LLM trigger | **Manual toggle + on-demand button** (no auto/confidence) | Predictable; our whisper binding doesn't surface token confidence, so "auto on low-confidence" would need new plumbing. |
| Deterministic match | **Exact + context-gated phonetic** | Exact = zero false positives. Phonetic fires only toward terms the user cares about, so a bare "next" is never turned into "Next.js". |
| User dict storage | **New `pronunciation.json`** | `custom_terms` is dead code with the wrong semantics (term→definition). |
| Learning | **User-confirmed one-tap save only** | Auto clipboard-diff capture is sub-project D. |

## What "LLM" means here

PIE has three model pieces:

- **Whisper (STT)** — fully local; transcribes only, cannot self-correct jargon.
- **Silero VAD** — local; not involved.
- **LLM** — `src/llm` (`LlmRouter` + OpenAI-compatible client). Defaults to
  `https://api.openai.com/v1` but honors `OPENAI_BASE_URL`, so it can be a local
  server (Ollama / LM Studio). It is the same provider the optimize/"Send to
  LLM" step already uses.

The deep-correct pass reuses this router as-is. Cost/network concerns apply only
when the configured endpoint is remote; with a local endpoint the deep pass is
free and offline, costing only local inference latency. Either way it is off the
always-on path and off by default.

## Architecture

### Module layout

```
src/corrector/
  mod.rs          // PronunciationCorrector: public API (correct())
  dictionary.rs   // CorrectionDict: entries + exact/phonetic lookup
  phonetic.rs     // metaphone-style key; the fuzzy tier
  static_seed.rs  // loads the embedded curated seed
  tech_terms.json // curated seed, include_str!'d at compile time
  llm_correct.rs  // opt-in deep pass: meta-prompt + diff extraction
```

Sits beside `intent/` and `optimizer/`. One primary public type,
`PronunciationCorrector`, held by `PieEngine`.

### Data model

```rust
pub struct Correction {
    pub heard: String,        // lowercased match key, e.g. "next jazz"
    pub canonical: String,    // replacement, e.g. "Next.js"
    pub source: Source,       // Static | User
}

pub enum Source { Static, User }

pub struct CorrectionOutcome {
    pub text: String,             // corrected transcript
    pub applied: Vec<AppliedFix>, // what changed, for UI transparency
}

pub struct AppliedFix {
    pub from: String,   // "next jazz"
    pub to: String,     // "Next.js"
    pub tier: Tier,     // Exact | Phonetic | Llm
}

pub enum Tier { Exact, Phonetic, Llm }
```

Deliberately absent for v1: `confidence`, persisted `phonetic_key`, `last_seen`,
decay. Those belong to auto-learning (sub-project D). Phonetic keys are computed
on load, never stored.

### Storage — two layers

- **Static seed:** `src/corrector/tech_terms.json`, embedded via `include_str!`.
  Ship a modest, high-quality set (~60–100 solid dev terms). Quality over
  coverage; users extend it. Static entries are read-only at runtime.
- **User dict:** new `pronunciation.json` in PIE's config dir, loaded/saved
  alongside memory. A user entry overrides a static entry with the same `heard`.
  `custom_terms` in `UserProfile` is left untouched (candidate for later
  removal as cleanup; not required by this work).

### The context gate (self-bootstrapping)

The phonetic tier only maps a heard token toward a canonical term in the user's
**allow-set**, so it never over-corrects generic words. The allow-set is built
at correction time from:

- every **User-dict canonical**, plus
- any **Static canonical the user has already used** (present in
  history/memory), plus
- `profile.technologies` if ever populated.

This works from day one without a profile-editing UI: adding a correction gates
that term in; static terms stay dormant until used. `technologies` and
`custom_terms` are currently never populated, so the design does not depend on
them.

## Pipeline integration

In `src/pipeline/engine.rs`, one new step between transcribe and process:

```
process_audio():
  text = stt.transcribe(samples)     // unchanged
  outcome = corrector.correct(&text) // NEW — deterministic, synchronous, no I/O
  self.process(outcome.text)         // extract → optimize (unchanged)
```

`PieEngine` gains a `corrector: PronunciationCorrector` field, built in `new()`
(loads embedded seed + user dict). Deterministic `correct()` is synchronous and
allocation-light — it does not touch the recently-tuned latency budget.

`outcome.text` becomes `intent.raw_input` as today. `outcome.applied` is passed
through the Tauri result so the UI can show a quiet "corrected: next jazz →
Next.js" line — transparency so a wrong auto-correction is visible and fixable,
not silent.

## Matching algorithm

`correct(&str) -> CorrectionOutcome`, two ordered tiers:

1. **Exact phrase pass.** Phrase map from all entries, keys lowercased. Scan for
   the longest matching phrase first (multi-word before single-word, so "next
   jazz" wins over "next"), case-insensitive, on word boundaries. Replace with
   `canonical`; record an `Exact` fix. Zero false positives.

2. **Context-gated phonetic pass.** For tokens not already replaced: compute a
   metaphone key, look it up in the phonetic index, and accept a match **only if
   its `canonical` is in the allow-set**. Record a `Phonetic` fix. This turns
   "next jaz" → "Next.js" for a Next.js user while leaving a plain "next" alone
   for everyone else.

### Edge cases

- Empty transcript: bailed before the corrector runs (existing behavior).
- No dictionary hits: return input unchanged, `applied` empty (byte-identical).
- Overlapping phrases: longest-match wins; no double replacement.
- Sentence-start capitalization: canonical form is emitted verbatim ("Next.js"
  stays "Next.js").
- Word boundaries: "nextel" must not match "next".

## Opt-in LLM deep pass

Lives in `llm_correct.rs`, reuses `LlmRouter`. Never on the always-on path. Two
explicit entry points:

- **Setting `deep_correct_ai: bool`** (default `false`, in `src-tauri`
  settings). While on, `process_audio` runs the LLM pass after the deterministic
  pass.
- **On-demand "Re-correct with AI"** button on the result view — corrects the
  one transcript in front of the user without changing the setting.

**Meta-prompt** is scoped to correction only, not rewriting: "Fix likely
speech-to-text errors in technical terms. Preserve meaning, wording, and
structure. Only change words that are garbled. User context: role={}, tech={}."
It returns corrected text; the diff vs. input becomes `applied` fixes with tier
`Llm`, shown the same quiet way.

**Learning bridge (opt-in, user-confirmed).** When the LLM pass changes a term,
the result view offers a one-tap "Save 'next jazz' → 'Next.js'" chip that writes
it to the user dict. This is the only learning in v1 — one entry, user-confirmed.
It is not the automatic clipboard-diff capture of sub-project D. It makes the
expensive LLM correction pay for itself: the next occurrence is instant and
offline.

## UI surface

New "Vocabulary" section in the existing settings pane:

- Toggle for "Deep-correct with AI".
- An editable list of **user** corrections (heard → canonical): add, edit,
  delete. Static entries are not listed, to keep it uncluttered.
- New Tauri commands `list_corrections`, `add_correction`, `delete_correction` —
  thin wrappers over the corrector's user dict, following the existing settings
  command pattern and locking the corrector the same way `settings`/`vad_cache`
  are handled today.

Result view additions:

- Quiet "corrected: X → Y" line(s) when `applied` is non-empty.
- "Re-correct with AI" button.
- One-tap "Save correction" chip after an LLM correction.

## Testing strategy

End-to-end behavior favored over shallow unit tests.

### Unit (`src/corrector/`, deterministic)

- Exact tier: single/multi-word replacement; longest-match wins; case-insensitive
  match with canonical casing preserved; word-boundary safety ("nextel" ≠
  "next"); no-match returns input unchanged with empty `applied`.
- Phonetic tier: gated match fires when canonical is in the allow-set; **same
  input with an empty allow-set does NOT fire** (the core anti-over-correction
  guarantee); metaphone collisions resolve to the gated candidate.
- Dictionary load: embedded seed parses; user dict round-trips through
  save/load; user entry overrides a same-`heard` static entry.
- `applied` records the right `{from, to, tier}` per fix.

### Integration (`tests/`, real pipeline)

- Feed a transcript with a known garble through `process()` and assert the
  optimized prompt contains the canonical term — proves the corrector sits in the
  real flow.
- Corrector is a byte-identical no-op when the transcript has no dictionary hits.

### LLM pass

- Unit-test the meta-prompt builder and the diff→`applied` extraction against the
  router's `echo` debug provider. No live network in tests.

### Manual (interactive, owed by user)

- Voice→paste round trip with the toggle on.
- "Save correction" chip persists across restarts.

## Out of scope (future sub-projects)

- Automatic learning from post-paste clipboard edits (Enhancement D).
- Confidence-driven auto-triggering of the LLM pass.
- Community-shared / versioned static dictionary distribution.
- Removing the dead `custom_terms` field (optional cleanup, not required here).

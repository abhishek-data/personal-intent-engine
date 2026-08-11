# Intent Extraction Test Results — Rule-Based Extractor (Phase 1 Audit)

Date: 2026-08-11
Test: `cargo test --test test_intent_extractor -- --nocapture`
Corpus: `tests/fixtures/intent_corpus.json`

## Simple commands — OK

| Input | Objective | Type | Confidence |
|---|---|---|---|
| "create a new file called main.rs" | verbatim passthrough | Task | High |
| "what is the capital of France" | verbatim passthrough | Question | High |

Short direct commands survive because the "extraction" is essentially a
passthrough with filler-stripping. Acceptable.

## Rambling speech — FAILS

### Case 1
- **Input:** "so I was thinking about this thing right, like, I want to build
  something that listens to me and figures out what I mean, you know, like,
  not just dictation but actually understanding, and then it should, like,
  make a prompt or something"
- **Expected objective:** "build intent extraction system"
- **Actual objective:** the entire rambling input echoed back, with only
  "I want to" deleted mid-sentence (leaving a double space):
  "so I was thinking about this thing right, like,  build something that
  listens to me and figures out what I mean, …, make a prompt or something"
- **Topics:** `[]` (expected: voice assistant / intent extraction / NLP)

### Case 2
- **Input:** "okay so um, I have this idea, it's like, okay so imagine you're
  talking and, uh, the system just knows what you're trying to say, right?
  like it corrects the words but also understands the, the meaning behind it"
- **Expected objective:** "build context-aware speech correction"
- **Actual objective:** the entire input echoed back verbatim.
- **Confidence:** High — actively wrong; the extractor is most confident on
  input it least understands (no uncertainty keywords → "High").
- **Topics:** `[]`

## Diagnosis

1. `extract_objective` = fixed filler-phrase deletion + first-sentence
   truncation. Rambling speech has no clean sentence boundaries and its filler
   isn't on the list, so the "objective" is the raw ramble.
2. `extract_topics` = hardcoded tech-keyword list. Concepts described in plain
   words ("something that listens to me") match nothing.
3. `assess_confidence` = keyword/word-count heuristics. It reports *speech
   pattern* confidence, not *understanding* confidence — hence High on Case 2.

Keyword matching extracts words, not meaning. Conclusion: complex/rambling
input needs an LLM-backed extractor (Phase 2); rules remain fine as the fast
path for short direct commands and as the offline fallback.

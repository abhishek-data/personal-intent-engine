# Follow-up: Single-Writer `learned_vocab.json` — Plan

> Resolves the two-writer lost-update race flagged in the Phase 2 and Phase 3
> final reviews. Must land before background mining is presented as reliable.

## Problem

`learned_vocab.json` currently has TWO independent writers when background mining
is on: the engine (reinforcement of firing corrections, plus synced imports, plus
applying mined terms) and the background learner task (which loads the store,
adds mined terms, and saves). Each does load-modify-save from its own snapshot,
so a concurrent write can silently drop an update — and synced (imported) entries
do not self-heal. Atomic saves (already added) prevent torn reads but not lost
updates.

## Fix: engine is the sole writer

The background learner becomes a pure miner: it emits `ExtractedCorrection`s on a
channel back to the engine instead of touching the file. The engine drains that
channel at the top of `process()` and applies each via `add_auto_correction`
(which it already owns). Now every write to `learned_vocab.json` — reinforcement,
sync, and mined terms — goes through the single engine-owned corrector, which is
serialized behind the app's engine mutex. One writer, no race. Reinforcement keeps
working with mining off (the engine always writes).

## Tasks

### Task 1: Engine drains a mined-corrections channel (TDD, testable)
`src/pipeline/engine.rs`:
- Add `mined_rx: Option<mpsc::Receiver<crate::corrector::learner::ExtractedCorrection>>` field (default `None` in all constructors).
- `pub fn set_mined_rx(&mut self, rx: mpsc::Receiver<ExtractedCorrection>)`.
- In `process()`, before correction (near `maybe_reload_learned`), drain the channel: `while let Ok(term) = rx.try_recv() { self.corrector.add_auto_correction(&term.heard, &term.canonical)?; }` (ignore individual errors, don't fail the pipeline; `add_auto_correction` dedups/reinforces).
- Test: build ephemeral engine, `set_mined_rx`, send an `ExtractedCorrection` on the paired sender, run `process("x", "compact")`, assert the term now corrects / `learned_count` rose.

### Task 2: Learner emits instead of writing
`src/corrector/learner.rs`:
- Replace the `learned_path: PathBuf` field with `out: mpsc::Sender<ExtractedCorrection>`.
- `new(...)` takes `out` instead of `learned_path` (drop the `LearnedStore` import if unused).
- `run()`: for each extracted term not already in the in-memory `known` set, `try_send` it on `out` and insert into `known` (drop the on-disk `has_entry` check and the `LearnedStore::load`/`add_or_reinforce`). Dedup against `known` only; the engine's `add_auto_correction` is idempotent, so cross-session dups just reinforce.
- Keep the batching, rate-limit, and pure-helper tests unchanged.

### Task 3: Wire the results channel in the app
`src-tauri/src/main.rs` setup closure (`if mining` block):
- Create `let (mined_tx, mined_rx) = mpsc::channel::<pie_engine::corrector::learner::ExtractedCorrection>(100);`.
- `engine.set_mined_rx(mined_rx)` on the managed engine (block_on the engine lock, like `set_learner_tx`).
- Construct `BackgroundLearner::new(rx, llm, mined_tx, provider, model, HashSet::new())` (mined_tx replaces learned_path).

## Verify
`cargo test -p pie-engine --lib` (all pass + the new engine test); `cargo build -p pie-desktop` clean; clippy no new warnings. Update the code `NOTE:` comments in engine.rs + learner.rs that described the two-writer limitation — it no longer applies.

## Acceptance
- [ ] Background learner never opens/writes `learned_vocab.json` (only sends on the channel).
- [ ] Engine applies mined terms in `process()` via `add_auto_correction`.
- [ ] Reinforcement + sync + mined all write through the one engine-owned corrector.
- [ ] Mining off: no channel, no miner, unchanged behavior.

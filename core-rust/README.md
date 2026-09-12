# kbcore — configurable predictive text engine (Rust, 9-key T9 default)

Lean, Android-free core: T9 mapping, FST-backed dictionary stack,
frequency ranking, personal overlay, session logging, extension packs,
UniFFI bindings.

## Layout

```text
core-rust/
  Cargo.toml          # crate kbcore (cdylib + rlib), uniffi 0.32 build feature
  build.rs            # uniffi::generate_scaffolding("src/kbcore.udl")
  src/
    lib.rs            # module wiring
    mapping.rs        # layout-driven encode (t9-9 default), Devanagari collapse, adjacency/1-edit
    stack.rs          # DictionaryStack { base + extensions + personal } + fst index
    rank.rs           # ranking formula + weights
    personal.rs       # personal dict (counts, last_seen, LFU 10k, redb)
    session.rs        # accepted/rejected JSONL log, export/import
    pack.rs           # packs/<id>.json loader
    predictor.rs      # UniFFI Predictor object (proc-macro export)
../packs/            # showcase extension packs (workspace root)
```

## Storage choice: redb (not rusqlite)

`redb` (pure Rust, embedded, ACID) persists the personal dictionary.
Justification: no C toolchain required (unlike `rusqlite`'s bundled
SQLite), which keeps Android NDK cross-compilation simple; typed tables
suffice for key-value personal/bigram persistence, while JSONL remains
the interchange/export format per SPEC section 5.

## Build / test

```sh
cd core-rust
cargo build
cargo test
```

`cargo test` covers mapping (`hello`->`43556`, Devanagari collapse,
1-edit neighbors), ranking (exact > neighbor, personal boost, reject
penalty), and stack priority (extension display category wins, personal
promote/block).

## Ranking formula

```text
S(w) = 1.0*log10(freq_base + freq_personal + 1)
     + 1.2*P_personal(w)        # 0/1 if in personal
     + 0.8*Bigram(ctx_prev, w)  # log10(c+1)-log10(c_prev+V), personal only
     + 0.5*Recency(w)           # exp(-dt/7d) since last accept
     + 0.3*CategoryBoost(w)     # 1.0 if w.cat == activeTab, 0.5 if personal tab
     + 0.2*KeyFit(seq, w)       # 1.0 exact, 0.9 prefix, 0.6 neighbor
     - 1.5*RejectPenalty(w)     # rejects/(accepts+rejects+1)
```

Candidates with `S < -0.5` are hidden. Tie-break: shorter word, then
lexicographic, then pack priority. Weights live in
`rank::RankWeights::default()`.

## Key mapping

2:abc 3:def 4:ghi 5:jkl 6:mno 7:pqrs 8:tuv 9:wxyz, 0:space, 1:punct,
`*`:back/cycle (control), `#`:mode (control). Devanagari initials are
collapsed phonetically onto keys 2-9 (see `mapping::char_to_digit`).

## Extension packs

`packs/<id>.json`: `{id, title, version, words[]}`, words are
`{w, freq?, cat?, lang?, seq?}`. `seq` is an optional explicit digit
sequence for non-encodable display words (emoji, LaTeX). Load with
`pack::load_pack(path)` then `stack.add_pack(&pack, priority)`.
`cat` defaults to the pack id, `lang` to `"en"`, `freq` to 100.

## UniFFI (Android/JNI via generated bindings)

```rust
let p = Predictor::new(base_json);
let cands = p.suggest(ctx, digits, active_tab, limit);
p.learn(word, category);   // personal overlay + session accept event
p.forget(word);            // block tombstone
let jsonl = p.export_session();
```

## Storage migration: redb → SQLite (rusqlite bundled)

Delta (2026-09-12): replaced `redb = "2"` with
`rusqlite = { version = "0.32", features = ["bundled"] }` in
`core-rust/Cargo.toml` (lock updated). `PersonalDict::save_to_redb` /
`load_from_redb` removed; replaced by `save_to_sqlite` / `load_from_sqlite`
backed by tables `personal(key, word, category, count, acc, rej,
last_seen, deleted)` + `bigrams(prev, word, count)`. `SessionLogger`
gains `save_to_sqlite` / `load_from_sqlite` on table
`session_events(ts, seq, ctx, chosen, shown_json, action)` (same DB file,
shared `PersonalDict::SQLITE_SCHEMA`). `Predictor::reject(word)` added
(personal `record_reject` + rejected session event); UniFFI surface is
otherwise unchanged (`new`/`suggest`/`learn`/`forget`/`export_session`).

Behavior preserved: LFU cap 10k + just-learned exclusion + deterministic
`(count, last_seen, key)` tie-break, tombstones (`del` canonical,
`deleted` alias) exempt from eviction, password-mode `learn` bypass,
hide threshold `-0.5`, 200-cap pre-truncate, recency `exp(-dt/7d)` /
reject `rej/(acc+rej+1)` decay rules. JSONL stays the interchange/export
format; SQLite is local persistence only.

Concurrency/pragmas: `PRAGMA journal_mode=WAL` on open; single-writer
discipline — one short-lived `Connection` + one transaction per
save/load, serialized by `Predictor`'s `Mutex` (no pool, no shared
connection).

Size/lean note: `bundled` compiles static SQLite (no system `libsqlite3`,
NDK-friendly, no extra toolchain beyond the C compiler the NDK already
provides). Cost is ~+1 MB object code and a longer `core-rust` build vs
pure-Rust `redb`; runtime footprint stays lean (one file DB, no server,
WAL keeps readers non-blocking). `cargo test` (35 tests incl.
`sqlite_roundtrip`, `sqlite_event_log_roundtrip`,
`tombstone_serde_compat_del_deleted`) and `cargo check --locked` green.

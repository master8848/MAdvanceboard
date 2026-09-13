# keyboard-project task runner (modern make). Run `just --list` to see all.
# Recipes are lean status wrappers; no heavy builds run by default.

default:
    @just --list

# Trust mise config and install all pinned toolchains.
bootstrap:
    @echo "-> mise trust + install"
    mise trust
    mise install
    @echo "OK bootstrap done (rust / java17 / gradle / just / python / bun)"

# Rust core tests only.
test-core:
    @echo "-> cargo test -p kbcore"
    cargo test -p kbcore --manifest-path core-rust/Cargo.toml

# Rust core check + clippy (warnings non-blocking).
check-core:
    @echo "-> cargo check -p kbcore"
    cargo check -p kbcore --manifest-path core-rust/Cargo.toml
    cargo clippy -p kbcore --manifest-path core-rust/Cargo.toml -- -D warnings || echo "(!) clippy warnings - non-blocking"

# Validate every pack JSON + the schema file itself with stdlib json only.
validate-packs:
    @echo "-> validating packs/*.json + kbpack.schema.json"
    @python3 -c "import glob, json; fs = sorted(glob.glob('packs/*.json')) + ['kbpack.schema.json']; [json.load(open(f, encoding='utf-8')) for f in fs]; print('ok: ' + ', '.join(fs)); print('VALID', len(fs), 'files')"

# Fence balance + link-URL surface for manual review.
docs-check:
    @echo "-> docs fence/link sanity (docs/*.md, README.md, SPEC.md)"
    @for f in docs/*.md README.md SPEC.md; do n=$(grep -o '```' "$f" | wc -l); if [ $((n % 2)) -ne 0 ]; then echo "(!) odd fences in $f: $n"; exit 1; fi; echo "  ok $f ($n fence marks)"; done
    @grep -rhoE '\[[^]]+\]\(([^)]+)\)' docs README.md SPEC.md 2>/dev/null | sort -u | head -20 || true
    @echo "OK docs-check done (manually review any links listed above)"

# Deep sweep: golden T9 vectors per layout, pack seq/tr/matra checks,
# tail-honesty report, cat_map resolve, schema self-check, stale wording.
# Stdlib only; fails loudly on any vector/coverage/resolve mismatch.
sweep-packs:
    #!/usr/bin/env python3
    import glob, json, re, sys
    from pathlib import Path
    errs = []
    def load_layout(lid):
        doc = json.loads(Path(f"layouts/{lid}.json").read_text(encoding="utf-8"))
        m = {}
        for key in doc["keys"]:
            for ch in key.get("symbols", ""):
                m.setdefault(ch, key["code"])
        for d in "0123456789":
            m[d] = d
        return m
    def enc(m, w):
        return "".join(d for ch in w if (d := m.get(ch, m.get(ch.lower()))) is not None)
    maps = {lid: load_layout(lid) for lid in ("t9-9", "t9-12", "t9-16")}
    for lid, m, word, want in [
        ("t9-9", maps["t9-9"], "hello", "43556"), ("t9-12", maps["t9-12"], "hello", "43556"),
        ("t9-16", maps["t9-16"], "hello", "43556"),
        ("t9-9", maps["t9-9"], "fun", "386"), ("t9-12", maps["t9-12"], "fun", "386"),
        ("t9-16", maps["t9-16"], "fun", "396"),
        ("t9-9", maps["t9-9"], "namaste", "6262783"), ("t9-16", maps["t9-16"], "namaste", "6262893"),
    ]:
        got = enc(m, word)
        print(f"  {lid} {word}->{got} (want {want})")
        if got != want:
            errs.append(f"{lid}: {word} -> {got}, want {want}")
    matra = set(chr(c) for c in list(range(0x93E, 0x94D)) + [0x94D])
    for f in sorted(glob.glob("packs/*.json")):
        p = json.loads(Path(f).read_text(encoding="utf-8"))
        rows = p.get("words", [])
        m9 = maps["t9-9"]
        def seq(e):
            return e.get("seq") or (enc(m9, e["tr"]) if e.get("tr") else "") or (enc(m9, e["key"]) if e.get("key") else "") or enc(m9, e.get("w", ""))
        empty = [e["w"] for e in rows if not seq(e)]
        matra_only = [e["w"] for e in rows if e.get("w") and all(c in matra for c in e["w"])]
        if empty:
            errs.append(f"{f}: {len(empty)} empty-seq {empty[:3]}")
        if matra_only:
            errs.append(f"{f}: {len(matra_only)} matra-only {matra_only[:3]}")
        if p.get("id") == "ne":
            notr = [e["w"] for e in rows if not e.get("tr")]
            if notr:
                errs.append(f"{f}: {len(notr)} NE rows lack tr {notr[:3]}")
            floor = sum(1 for e in rows if e.get("freq") == 200)
            print(f"  {f}: n={len(rows)} tr-missing={len(notr)} floor-200={floor} (low-confidence tail, plan/23 step 5)")
        elif p.get("id") == "words":
            from collections import Counter
            c = Counter(e.get("freq") for e in rows)
            lo, floormin = min(c), c[min(c)]
            print(f"  {f}: n={len(rows)} clamp-{lo}={floormin} (Zipf tail, packs/README)")
        else:
            print(f"  {f}: n={len(rows)} empty={len(empty)} matra-only={len(matra_only)}")
    cm = json.loads(Path("layouts/cat_map.json").read_text(encoding="utf-8"))
    known = {"t9-9", "t9-12", "t9-16"}
    if cm.get("default") not in known:
        errs.append(f"cat_map default {cm.get('default')!r} not in {sorted(known)}")
    for cat, lid in cm.get("cats", {}).items():
        if lid not in known:
            errs.append(f"cat_map[{cat!r}] -> unknown layout {lid!r}")
    print(f"  cat_map: default={cm.get('default')} pinned={len(cm.get('cats', {}))} (empty pins = all resolve via global default)")
    sch = json.loads(Path("kbpack.schema.json").read_text(encoding="utf-8"))
    for k in ("id", "version", "kind", "name"):
        if k not in sch.get("required", []):
            errs.append(f"schema: required missing {k!r}")
    re.compile(sch["properties"]["version"]["pattern"])
    print("  schema: required keys ok, semver pattern compiles")
    stale = []
    pat = re.compile(r"9-key predictive keyboard|9-button keyboard", re.I)
    for f in glob.glob("README.md") + sorted(glob.glob("docs/*.md")) + ["core-rust/README.md", "android/README.md"]:
        try:
            text = Path(f).read_text(encoding="utf-8")
        except FileNotFoundError:
            continue
        for i, line in enumerate(text.splitlines(), 1):
            if pat.search(line):
                stale.append(f"{f}:{i}")
    if stale:
        errs.append(f"stale board claims: {stale[:5]}")
    print(f"  wording: {'STALE ' + str(stale[:5]) if stale else 'zero stale 9-button claims'}")
    if errs:
        print("SWEEP FAIL:")
        [print("  (!) " + e) for e in errs]
        sys.exit(1)
    print("OK sweep-packs done")

# Android debug build via wrapper (mise gradle is fallback); needs JDK 17 on PATH.
android-assemble:
    @echo "-> assembleDebug (requires JDK 17)"
    @if [ -z "${JAVA_HOME:-}" ]; then echo "(!) JAVA_HOME unset - run: mise install, then re-exec shell with mise activated"; fi
    cd android && ./gradlew assembleDebug

# Cross-compile the Rust engine for Android (needs ANDROID_NDK_HOME + cargo-ndk).
# Rebuilds android/ime/src/main/jniLibs/<abi>/libkbcore.so (release).
# Re-run after ANY #[uniffi::export] change, then re-regen the Kotlin bindings
# (uniffi-bindgen 0.32.1, see docs/BUILD.md) — .so checksums must match them.
ndk-libs:
    @echo "-> cargo ndk release libs (arm64-v8a + x86_64)"
    @if [ -z "${ANDROID_NDK_HOME:-}" ]; then echo "(!) ANDROID_NDK_HOME unset - point it at NDK 28 (see docs/BUILD.md)"; exit 1; fi
    cd core-rust && cargo ndk -t arm64-v8a -t x86_64 -o ../android/ime/src/main/jniLibs build --release
    @ls -la android/ime/src/main/jniLibs/arm64-v8a/libkbcore.so android/ime/src/main/jniLibs/x86_64/libkbcore.so

# Read-only snapshot; cross-module reconcile is owned by another agent.
sync-report:
    @echo "-> sync-report (read-only snapshot, no edits)"
    @git status --short || true
    @echo "OK status above; this recipe makes no changes"

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

# Android debug build via wrapper (mise gradle is fallback); needs JDK 17 on PATH.
android-assemble:
    @echo "-> assembleDebug (requires JDK 17)"
    @if [ -z "${JAVA_HOME:-}" ]; then echo "(!) JAVA_HOME unset - run: mise install, then re-exec shell with mise activated"; fi
    cd android && ./gradlew assembleDebug

# Read-only snapshot; cross-module reconcile is owned by another agent.
sync-report:
    @echo "-> sync-report (read-only snapshot, no edits)"
    @git status --short || true
    @echo "OK status above; this recipe makes no changes"

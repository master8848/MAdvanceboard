#!/usr/bin/env python3
"""Sync built packs into Android category assets.

Reads `packs/*.json` (validated first: loader-compat + production
minimums, failing loudly on any near-empty pack) and writes
`android/ime/src/main/assets/categories/<id>.json` manifests with the
built `words` embedded, so the on-device engine serves the same vocab
the Rust core loads. Existing manifest fields (displayName, priority,
learn, langs, description) are preserved; `version` tracks the pack.

  python3 scripts/sync_android_assets.py

Mapping: words_en.json->words.json, nepali.json->ne.json,
code_js.json->js.json, code_rust.json->rust.json,
code_html.json->html.json, emoji.json->emoji.json, math.json->math.json,
medical.json->medical.json (new extension manifest), numbers.json stays
manifest+digits.
"""

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
from build_pack import load_pack, validate_pack  # noqa: E402 (same T9 map)

PACK_TO_ASSET = {
    "words_en": "words",
    "nepali": "ne",
    "numbers": "numbers",
    "code_js": "js",
    "code_rust": "rust",
    "code_html": "html",
    "emoji": "emoji",
    "math": "math",
    "medical": "medical",
}

# Fresh manifest for packs with no existing Android asset (extension
# packs, not tabs: SPEC §0 tabs stay numbers/words/ne/js/rust/html/emoji/
# math/★personal; medical loads via CategoryRegistry.listIds).
NEW_MANIFEST = {
    "medical": {
        "id": "medical",
        "displayName": "medical",
        "title": "medical",
        "source": "bundled",
        "priority": 40,
        "langs": ["en"],
        "description": "Medical terms (symptoms, diseases, anatomy, drugs, procedures)",
        "privacy": {"learn": True},
    },
}


def main() -> None:
    assets = ROOT / "android" / "ime" / "src" / "main" / "assets" / "categories"
    if not assets.is_dir():
        sys.exit(f"missing assets dir: {assets}")
    for pack_name, asset_id in PACK_TO_ASSET.items():
        pack = load_pack(ROOT / "packs" / f"{pack_name}.json")
        errs = validate_pack(pack, pack_name)
        if errs:
            print(f"REFUSING sync for {pack_name}:", file=sys.stderr)
            for e in errs[:10]:
                print(f"  {e}", file=sys.stderr)
            sys.exit(1)
        out = assets / f"{asset_id}.json"
        if out.exists():
            manifest = json.loads(out.read_text(encoding="utf-8"))
        else:
            manifest = dict(NEW_MANIFEST[asset_id])
            print(f"new asset manifest for pack '{pack['id']}'")
        manifest["id"] = pack["id"]
        manifest["version"] = pack["version"]
        manifest["source"] = "bundled"
        manifest["words"] = pack["words"]
        out.write_text(json.dumps(manifest, ensure_ascii=False) + "\n", encoding="utf-8")
        print(f"synced {pack_name}.json ({len(pack['words'])} words, "
              f"v{pack['version']}) -> assets/categories/{asset_id}.json")


if __name__ == "__main__":
    main()

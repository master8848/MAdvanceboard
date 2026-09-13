# Friend-Ready Checklist (plan/24 §Minimal checklist)

Check before inviting anyone. Each item is checkable: status + where the
evidence lives. Items 2–9 that depend on unbuilt Tier-1/2 work are marked
`BLOCKED (owner file)` — they ride with QWERTY/parity, not after
(plan/10 Tier-3 order).

- [x] 1. Signed distributable + listing. Play internal track at minimum;
  EN (+NE) listing + screenshots per `docs/PLAY_LISTING.md`. *Shipped by
  release process; text ready in-repo.*
- [x] 2. Enable→switch <2 min. Warning pre-frame + auto-advance +
  switch-back instructions (`OnboardingActivity`, 5 steps, skippable).
  *Code done; timing to confirm on-device (<2 min, stopwatch).*
- [x] 3. QWERTY default; 9-key opt-in. `FriendDefaults.qwertyDefault`
  (default ON) + onboarding step 2 + Settings → Typing toggle; service
  starts on QWERTY, ⌨ switches anytime. *Code done; needs `22`
  parity for Gboard-grade corrections (separate track).*
- [ ] 4. EN+NE messaging without tab dance (`23` steps 1+4); visible tabs
  `EN/ने/😀/123` only. *Strip filter done (`FriendDefaults.filterTabs`,
  code tabs hidden by default); Roman-commit + no-dance needs `23`.*
- [x] 5. Emoji grid + recents. `EmojiPickerSheet` (grid + recents +
  search) + `EmojiRecents` store; 🙂 opens it. *Code done.*
- [ ] 6. System voice, mic hidden in password/numbers (`15`). *Not built
  — ship system-engine path first; zero audio retention story required.*
- [ ] 7. Autocap, double-space period, backspace-restore, general undo
  (`22`). *Toolbar undo + fling undo exist; caps/double-space/restore
  need `22`.*
- [ ] 8. Zero crashes across WhatsApp/Viber/Messenger field test + no
  battery regression. *Checklist ready (`docs/FIELD_TEST.md`); run it
  before invites. Tier-2 power trace (`06`) still owed.*
- [x] 9. Clipboard OFF/24h; incognito visibly indicated; one-sentence
  retention story. `FriendDefaults.clipboardEnabled` (default OFF gates
  both capture paths + panel) + 24h ★ preset + dark strip + 🎭 badge +
  entry banner. *Code done. Story: "Clipboard is off until you turn it
  on; when on, unpinned items vanish after a day."*
- [x] 10. One-tap "report problem / switch back". Onboarding graduation
  carries switch-back instructions + "Report a problem" → Settings;
  ⚙ quick-sheet → Details → Dictionaries. *Code done.*

## Quick verification (no device lab)

- `just docs-check` — fence balance + link surface.
- `just test-core` — engine green (T9 metrics untouched by this track).
- `just validate-packs` — all 9 packs + schema load.
- `:ime` JVM tests (`FriendDefaultsTest`, `SettingsSearchTest`,
  `EmojiRecentsTest`, `CategoryLabelsTest` incl. medical round-trip) —
  run with JDK 17 (`cd android && ./gradlew :ime:testDebugUnitTest`).
- 8→4 routes: every legacy id in `resolveSection` maps to a live
  section; search filters `SEARCH_INDEX`; quick-sheet writes go through
  the same stores as Dictionaries (no fork).

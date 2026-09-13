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
  `EN/ने/😀/123` only. Status: **BLOCKED (`23`)**. Checkable: (a) CODE-DONE —
  strip filter (`FriendDefaults.filterTabs`, code tabs hidden by default,
  `FriendDefaultsTest` green on JVM); (b) BLOCKED — Roman-commit +
  no-dance needs `23` steps 1+4; (c) NOT-RUN — no device run claimed.
- [x] 5. Emoji grid + recents. `EmojiPickerSheet` (grid + recents +
  search) + `EmojiRecents` store; 🙂 opens it. *Code done.*
- [ ] 6. System voice, mic hidden in password/numbers (`15`). Status:
  **BLOCKED (service wiring)**. Checkable: (a) CODE-DONE — policy
  contract (`VoiceInputPolicy`: mic visible in TEXT only, hidden in
  PASSWORD/NUMBER/PHONE/DATETIME; learn gate `!password && !incognito`;
  network failure → `RetryOffline` reusing kept text, no re-speak;
  `close()` wipes to zero residue; `VoiceInputPolicyTest` covers all five
  rules on JVM); (b) BLOCKED — `SpeechRecognizer` service wiring not
  built (create/listener/stop-cancel/destroy, `isRecognitionAvailable`
  gate, `EXTRA_LANGUAGE`/`PREFER_OFFLINE`, `RECORD_AUDIO` via settings
  Activity only); (c) NOT-RUN — airplane-mode retry + residue sweep
  need a device run, none claimed.
- [ ] 7. Autocap, double-space period, backspace-restore, general undo
  (`22`). Status: **BLOCKED (`22`)**. Checkable: (a) CODE-DONE —
  toolbar undo + fling undo exist; (b) BLOCKED — caps/double-space/restore
  + general strip undo need `22`; (c) NOT-RUN — no device timing claimed.
- [ ] 8. Zero crashes across WhatsApp/Viber/Messenger field test + no
  battery regression. Status: **NOT-RUN**. Checkable: checklist ready
  (`docs/FIELD_TEST.md` per-app rows + stability rows); zero device runs
  claimed; Tier-2 power trace (`06`) still owed.
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
  `EmojiRecentsTest`, `VoiceInputPolicyTest`, `CategoryLabelsTest` incl.
  medical round-trip) —
  run with JDK 17 (`cd android && ./gradlew :ime:testDebugUnitTest`).
- 8→4 routes: every legacy id in `resolveSection` maps to a live
  section; search filters `SEARCH_INDEX`; quick-sheet writes go through
  the same stores as Dictionaries (no fork).

//! Voice typing: system engine first, API engine second (plan `15`).
//!
//! The mic is an action (like plan `01`), not typing — same privacy as
//! keys. No bundled model, no new engine: the OS/API does the work, so
//! keyboard CPU/battery stays at the `06` baseline.
//!
//! - **Default = in-IME system recognizer, zero config** (AOSP/Gboard
//!   pattern). The OS owns the mic: **no audio is kept** — only text
//!   partials are retained ([`VoiceSession::partials`]).
//! - **Optional = user API STT engine** (endpoint/key in encrypted prefs).
//!   This path owns the mic via `AudioRecord` + VAD, so it CAN
//!   buffer/replay: RAM ring ~10 s + cache-file spill (`cacheDir/stt_*`).
//! - **No re-speak on failure differs per engine** (plan `15:13-18`):
//!   system path reuses kept text (user only re-speaks the missing tail);
//!   API path re-POSTs the same buffered PCM.
//! - **Lifecycle = keyboard session**: [`VoiceSession::close`] frees the
//!   buffer + reports swept files; nothing survives IME close (zero
//!   residue), never SQLite / session log / sync / learn.
//!
//! This module is the pure, deterministic core (gates, retry policy,
//! residue accounting). The Android service owns the recognizer lifecycle
//! (`stopListening`/`cancel` before restart, `destroy()` in
//! `onFinishInputView`/`onDestroy`) and calls into here for policy.

/// Which engine serves the utterance (Settings → Voice → Engine).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum EngineKind {
    /// OS recognizer (default). Zero audio retention by construction.
    #[default]
    System,
    /// User-configured HTTP/WS STT endpoint (owns its mic buffer).
    Api,
}

/// Classified recognizer failure (plan `15:15-17` error mapping).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceError {
    /// `ERROR_NETWORK` / `SERVER`: offline retry keeps text (system) or
    /// re-POSTs buffered PCM (API) — no re-speak.
    Network,
    /// `ERROR_SERVER` persistent / disconnected: recreate before restart.
    ServerGone,
    /// `BUSY` / `TOO_MANY`: `destroy()` + recreate before restart.
    Busy,
    /// `NO_MATCH` / `SPEECH_TIMEOUT`: keep last partial visible.
    NoMatch,
    /// `INSUFFICIENT_PERMISSIONS`: rationale, mic disabled until granted.
    NoPermission,
    /// `LANGUAGE_NOT_SUPPORTED`: model-download prompt.
    BadLanguage,
}

/// True when a retry can proceed WITHOUT re-speaking: the kept text
/// (system) or buffered PCM (API) is reused. `false` only when there is
/// nothing to retry with (permission / language — user action needed).
pub fn retry_keeps_transcript(err: VoiceError, has_kept_input: bool) -> bool {
    match err {
        VoiceError::Network | VoiceError::ServerGone | VoiceError::Busy | VoiceError::NoMatch => {
            has_kept_input
        }
        VoiceError::NoPermission | VoiceError::BadLanguage => false,
    }
}

/// Cache-file spill prefix for the API path (`cacheDir/stt_*`, plan `15:30`).
pub const STT_SPILL_PREFIX: &str = "stt_";

/// True when `name` is a voice-spill file swept on keyboard close.
pub fn is_spill_file(name: &str) -> bool {
    name.starts_with(STT_SPILL_PREFIX)
}

/// Pure sweep: which cache-dir names to delete on close. Never touches
/// anything outside the `stt_*` namespace (audio must never take other
/// files with it).
pub fn sweep_spill_files(names: &[String]) -> Vec<String> {
    names
        .iter()
        .filter(|n| is_spill_file(n))
        .cloned()
        .collect()
}

/// One voice utterance's retained state. System path: `audio_retained` is
/// ALWAYS false (OS streams; nothing to spill). API path: true while the
/// RAM ring / spill file is alive. `partials` holds the last
/// `onPartialResults` / `UNSTABLE_TEXT` + `RESULTS_RECOGNITION` text on
/// BOTH paths (retry-offline reuses text; user re-speaks only the tail).
#[derive(Clone, Debug, Default)]
pub struct VoiceSession {
    pub engine: EngineKind,
    pub partials: Vec<String>,
    pub audio_retained: bool,
    pub spill_file: Option<String>,
}

impl VoiceSession {
    pub fn start(engine: EngineKind) -> Self {
        Self {
            engine,
            partials: Vec::new(),
            audio_retained: false,
            spill_file: None,
        }
    }

    /// System path invariant: starting (or running) the OS recognizer
    /// retains zero audio. Asserts in debug; returns the check for tests.
    pub fn system_holds_no_audio(&self) -> bool {
        self.engine != EngineKind::System || !self.audio_retained
    }

    /// Keep the latest interim/final text (both engines).
    pub fn keep_text(&mut self, text: &str) {
        let t = text.trim();
        if t.is_empty() {
            return;
        }
        self.partials.push(t.to_string());
    }

    /// API path only: retain the mic buffer (RAM ring + optional spill
    /// file) so `[Retry same]` re-POSTs without re-speaking. Panics in
    /// debug on the system path (OS owns that mic — there is nothing to
    /// retain, and pretending otherwise would be a residue lie).
    pub fn retain_audio(&mut self, spill_file: Option<String>) {
        debug_assert_eq!(
            self.engine,
            EngineKind::Api,
            "retain_audio is API-path only (system holds no audio)"
        );
        self.audio_retained = true;
        self.spill_file = spill_file;
    }

    /// Latest kept text (what retry reuses / the commit inserts).
    pub fn kept_text(&self) -> String {
        self.partials.join(" ")
    }

    pub fn has_kept_input(&self) -> bool {
        !self.partials.is_empty() || self.audio_retained
    }

    /// Close the keyboard session: free the buffer, drop text, report the
    /// spill files to delete. After `close`, [`Self::residue`] is zero —
    /// the acceptance bar (plan `15:50c`).
    pub fn close(&mut self) -> Vec<String> {
        let files = self.spill_file.take().into_iter().collect::<Vec<_>>();
        self.partials.clear();
        self.audio_retained = false;
        files
    }

    /// (retained text chunks, audio held, spill files pending). All zeros
    /// after [`Self::close`] — zero residue.
    pub fn residue(&self) -> (usize, bool, usize) {
        (
            self.partials.len(),
            self.audio_retained,
            self.spill_file.iter().len(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gates::{mic_visible, GateInput};

    #[test]
    fn mic_hidden_in_password_and_numbers() {
        // Plan 15:37-38 + friend-ready DoD #6: mic hidden in password
        // fields and on the numbers tab; everywhere else visible.
        assert!(!mic_visible(true, false));
        assert!(!mic_visible(false, true));
        assert!(mic_visible(false, false));
        // …and the service gate agrees through the shared GateInput shape:
        // incognito keeps the mic (insert without learning).
        let g = GateInput::normal("words", true);
        assert!(crate::gates::learn_now(&g));
        let inc = GateInput {
            incognito: true,
            ..GateInput::normal("words", true)
        };
        assert!(!crate::gates::learn_now(&inc));
        assert!(mic_visible(false, false), "incognito keeps mic, drops learn");
    }

    #[test]
    fn system_engine_zero_audio_retention() {
        let mut s = VoiceSession::start(EngineKind::System);
        s.keep_text("hello world");
        assert!(s.system_holds_no_audio());
        assert!(!s.audio_retained, "OS streams: nothing to spill");
        assert_eq!(s.kept_text(), "hello world");
    }

    #[test]
    fn airplane_mode_retry_without_respeak() {
        // (a) acceptance: API failure in airplane mode → one-tap retry
        // succeeds without re-speaking (buffered PCM / kept text reused).
        let mut sys = VoiceSession::start(EngineKind::System);
        sys.keep_text("good morning");
        assert!(retry_keeps_transcript(
            VoiceError::Network,
            sys.has_kept_input()
        ));
        let mut api = VoiceSession::start(EngineKind::Api);
        api.keep_text("good morning");
        api.retain_audio(Some("stt_0001.ogg".to_string()));
        assert!(retry_keeps_transcript(
            VoiceError::Network,
            api.has_kept_input()
        ));
        // Nothing kept → nothing to retry with (user must speak again).
        let empty = VoiceSession::start(EngineKind::System);
        assert!(!retry_keeps_transcript(
            VoiceError::Network,
            empty.has_kept_input()
        ));
        // Permission / language need user action, never silent retry.
        assert!(!retry_keeps_transcript(
            VoiceError::NoPermission,
            true
        ));
        assert!(!retry_keeps_transcript(
            VoiceError::BadLanguage,
            true
        ));
        // Busy/NoMatch keep text when there is text.
        assert!(retry_keeps_transcript(VoiceError::Busy, true));
        assert!(retry_keeps_transcript(VoiceError::NoMatch, true));
        assert!(!retry_keeps_transcript(VoiceError::Busy, false));
    }

    #[test]
    fn zero_residue_after_close() {
        // (b)/(c) acceptance: buffer + file freed on keyboard close; audit
        // finds zero audio/text on disk after close.
        let mut api = VoiceSession::start(EngineKind::Api);
        api.keep_text("transcript");
        api.retain_audio(Some("stt_0001.ogg".to_string()));
        assert_ne!(api.residue(), (0, false, 0));
        let files = api.close();
        assert_eq!(files, vec!["stt_0001.ogg".to_string()]);
        assert_eq!(api.residue(), (0, false, 0));
        assert!(api.kept_text().is_empty());

        let mut sys = VoiceSession::start(EngineKind::System);
        sys.keep_text("partial");
        assert!(sys.close().is_empty(), "system holds no files");
        assert_eq!(sys.residue(), (0, false, 0));
    }

    #[test]
    fn spill_sweep_only_touches_stt_namespace() {
        let names = vec![
            "stt_0001.ogg".to_string(),
            "stt_retry.pcm".to_string(),
            "prefs.xml".to_string(),
            "personal.jsonl".to_string(),
            "stt".to_string(),
        ];
        let swept = sweep_spill_files(&names);
        assert_eq!(
            swept,
            vec!["stt_0001.ogg".to_string(), "stt_retry.pcm".to_string()]
        );
    }

    #[test]
    fn voice_commits_respect_learn_gate() {
        // Plan 15:39: voice commits never enter personal/bigrams unless
        // learnNow passes (password/incognito/numbers/email suppressed).
        let cases = [
            (GateInput::normal("words", true), true),
            (
                GateInput {
                    password: true,
                    ..GateInput::normal("words", true)
                },
                false,
            ),
            (
                GateInput {
                    incognito: true,
                    ..GateInput::normal("words", true)
                },
                false,
            ),
            (GateInput::normal("numbers", false), false),
            (GateInput::normal("math", false), false),
        ];
        for (g, want) in cases {
            assert_eq!(
                crate::gates::learn_now(&g),
                want,
                "voice learn follows learnNow at {g:?}"
            );
        }
    }
}

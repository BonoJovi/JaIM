/// IBus Engine D-Bus interface implementation.
///
/// Implements org.freedesktop.IBus.Engine via zbus #[interface].
/// Bridges IBus key events to JaIM's ConversionEngine and sends
/// preedit/commit/candidates back via D-Bus signals.
use std::sync::Mutex;

use log::{debug, info};
use zbus::object_server::SignalEmitter;
use zbus::{interface, zvariant};

use crate::engine::ConversionEngine;

use super::keymap::*;

/// IBus Engine state
pub struct JaimEngine {
    engine: Mutex<ConversionEngine>,
    /// Whether the engine is active (enabled by IBus)
    enabled: Mutex<bool>,
    /// Whether we are in conversion mode (showing candidates)
    converting: Mutex<bool>,
    /// Current candidate list
    candidates: Mutex<Vec<String>>,
    /// Selected candidate index
    selected: Mutex<usize>,
}

impl JaimEngine {
    pub fn new() -> Self {
        Self {
            engine: Mutex::new(ConversionEngine::new()),
            enabled: Mutex::new(false),
            converting: Mutex::new(false),
            candidates: Mutex::new(Vec::new()),
            selected: Mutex::new(0),
        }
    }
}

/// Helper to build an IBus text variant (a{sv} dict with "s" key).
/// IBus expects text as a GVariant struct: (sa{sv}sv)
/// Simplified: we send a plain string wrapped in IBusText structure.
fn ibus_text(text: &str) -> zvariant::Value<'static> {
    // IBusText is serialized as: ("IBusText", {}, text, {})
    // But for CommitText/UpdatePreeditText, the daemon accepts
    // a simpler tuple format depending on the IBus version.
    // Using the struct format: (name, attachments, text, attributes)
    zvariant::Value::new(zvariant::Structure::from((
        "IBusText",                                          // type name
        std::collections::HashMap::<String, String>::new(),  // attachments
        text.to_string(),                                    // the actual text
    )))
}

#[interface(name = "org.freedesktop.IBus.Engine")]
impl JaimEngine {
    /// Process a key event. Returns true if handled.
    async fn process_key_event(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        keyval: u32,
        _keycode: u32,
        state: u32,
    ) -> zbus::fdo::Result<bool> {
        // Ignore key releases
        if is_release(state) {
            return Ok(false);
        }

        // Pass through modifier combos (Ctrl+C, Alt+Tab, etc.)
        if has_modifier(state) {
            return Ok(false);
        }

        let enabled = *self.enabled.lock().unwrap();
        if !enabled {
            return Ok(false);
        }

        let converting = *self.converting.lock().unwrap();

        // Handle special keys during conversion mode
        if converting {
            return self.handle_conversion_key(&emitter, keyval).await;
        }

        // Space → trigger conversion
        if keyval == IBUS_KEY_SPACE {
            return self.start_conversion(&emitter).await;
        }

        // Enter → commit current preedit as-is (hiragana)
        if keyval == IBUS_KEY_RETURN {
            return self.commit_preedit(&emitter).await;
        }

        // Escape → cancel input
        if keyval == IBUS_KEY_ESCAPE {
            return self.cancel_input(&emitter).await;
        }

        // Backspace → delete last character from buffer
        if keyval == IBUS_KEY_BACKSPACE {
            return self.handle_backspace(&emitter).await;
        }

        // Printable ASCII → feed to romaji converter
        if let Some(ch) = keyval_to_char(keyval) {
            if ch.is_ascii_alphabetic() || ch == '-' || ch == '\'' {
                let preedit = {
                    let mut engine = self.engine.lock().unwrap();
                    engine.process_key(ch.to_ascii_lowercase());
                    engine.preedit().to_string()
                };

                self.send_preedit(&emitter, &preedit).await?;
                return Ok(true);
            }
        }

        // Unhandled key
        Ok(false)
    }

    /// Called when the engine gains focus.
    async fn focus_in(&self) {
        info!("JaIM: FocusIn");
        *self.enabled.lock().unwrap() = true;
    }

    /// Called when the engine loses focus.
    async fn focus_out(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) {
        info!("JaIM: FocusOut");
        let _ = self.cancel_input(&emitter).await;
        *self.enabled.lock().unwrap() = false;
    }

    /// Reset engine state.
    async fn reset(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) {
        debug!("JaIM: Reset");
        let _ = self.cancel_input(&emitter).await;
    }

    /// Enable the engine.
    async fn enable(&self) {
        info!("JaIM: Enable");
        *self.enabled.lock().unwrap() = true;
    }

    /// Disable the engine.
    async fn disable(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) {
        info!("JaIM: Disable");
        let _ = self.cancel_input(&emitter).await;
        *self.enabled.lock().unwrap() = false;
    }

    /// Set cursor location (unused but required by interface).
    async fn set_cursor_location(&self, _x: i32, _y: i32, _w: i32, _h: i32) {}

    /// Set capabilities (unused but required by interface).
    async fn set_capabilities(&self, _cap: u32) {}

    // ---- IBus Signals ----

    /// Commit composed text to the application.
    #[zbus(signal)]
    async fn commit_text(emitter: &SignalEmitter<'_>, text: zvariant::Value<'_>)
        -> zbus::Result<()>;

    /// Update preedit text displayed in the input area.
    #[zbus(signal)]
    async fn update_preedit_text(
        emitter: &SignalEmitter<'_>,
        text: zvariant::Value<'_>,
        cursor_pos: u32,
        visible: bool,
    ) -> zbus::Result<()>;

    /// Hide preedit text.
    #[zbus(signal)]
    async fn hide_preedit_text(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    /// Update the lookup table (candidate list).
    #[zbus(signal)]
    async fn update_lookup_table(
        emitter: &SignalEmitter<'_>,
        table: zvariant::Value<'_>,
        visible: bool,
    ) -> zbus::Result<()>;

    /// Hide the lookup table.
    #[zbus(signal)]
    async fn hide_lookup_table(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

// Private helper methods (not exposed via D-Bus)
impl JaimEngine {
    async fn start_conversion(
        &self,
        emitter: &SignalEmitter<'_>,
    ) -> zbus::fdo::Result<bool> {
        let conversion_candidates = {
            let mut engine = self.engine.lock().unwrap();
            engine.convert()
        };

        if conversion_candidates.is_empty() {
            return Ok(false);
        }

        let candidate_texts: Vec<String> =
            conversion_candidates.iter().map(|c| c.text.clone()).collect();

        // Show the top candidate as preedit
        let top = &candidate_texts[0];
        self.send_preedit(emitter, top).await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;

        *self.candidates.lock().unwrap() = candidate_texts;
        *self.selected.lock().unwrap() = 0;
        *self.converting.lock().unwrap() = true;

        Ok(true)
    }

    async fn handle_conversion_key(
        &self,
        emitter: &SignalEmitter<'_>,
        keyval: u32,
    ) -> zbus::fdo::Result<bool> {
        match keyval {
            // Space / Down → next candidate
            IBUS_KEY_SPACE | IBUS_KEY_DOWN => {
                let text = {
                    let candidates = self.candidates.lock().unwrap();
                    let mut selected = self.selected.lock().unwrap();
                    if candidates.is_empty() {
                        None
                    } else {
                        *selected = (*selected + 1) % candidates.len();
                        Some(candidates[*selected].clone())
                    }
                };
                if let Some(text) = text {
                    self.send_preedit(emitter, &text).await
                        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
                }
                Ok(true)
            }
            // Up → previous candidate
            IBUS_KEY_UP => {
                let text = {
                    let candidates = self.candidates.lock().unwrap();
                    let mut selected = self.selected.lock().unwrap();
                    if candidates.is_empty() {
                        None
                    } else {
                        *selected = if *selected == 0 {
                            candidates.len() - 1
                        } else {
                            *selected - 1
                        };
                        Some(candidates[*selected].clone())
                    }
                };
                if let Some(text) = text {
                    self.send_preedit(emitter, &text).await
                        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
                }
                Ok(true)
            }
            // Enter → commit selected candidate
            IBUS_KEY_RETURN => {
                let text = {
                    let candidates = self.candidates.lock().unwrap();
                    let selected = *self.selected.lock().unwrap();
                    candidates.get(selected).cloned()
                };
                if let Some(text) = text {
                    {
                        let mut engine = self.engine.lock().unwrap();
                        engine.commit(&text);
                    }

                    Self::commit_text(emitter, ibus_text(&text)).await
                        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
                    Self::hide_preedit_text(emitter).await
                        .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;

                    *self.converting.lock().unwrap() = false;
                    *self.candidates.lock().unwrap() = Vec::new();
                }
                Ok(true)
            }
            // Escape → cancel conversion, return to preedit
            IBUS_KEY_ESCAPE => {
                self.cancel_conversion(emitter).await?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    async fn commit_preedit(
        &self,
        emitter: &SignalEmitter<'_>,
    ) -> zbus::fdo::Result<bool> {
        let preedit = {
            let mut engine = self.engine.lock().unwrap();
            engine.convert();
            let p = engine.preedit().to_string();
            if p.is_empty() {
                return Ok(false);
            }
            engine.commit(&p);
            p
        };

        Self::commit_text(emitter, ibus_text(&preedit)).await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Self::hide_preedit_text(emitter).await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;

        Ok(true)
    }

    async fn cancel_input(
        &self,
        emitter: &SignalEmitter<'_>,
    ) -> zbus::fdo::Result<bool> {
        self.engine.lock().unwrap().reset();
        *self.converting.lock().unwrap() = false;
        *self.candidates.lock().unwrap() = Vec::new();
        Self::hide_preedit_text(emitter).await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(true)
    }

    async fn cancel_conversion(
        &self,
        emitter: &SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        *self.converting.lock().unwrap() = false;
        *self.candidates.lock().unwrap() = Vec::new();
        let preedit = {
            let engine = self.engine.lock().unwrap();
            engine.preedit().to_string()
        };
        self.send_preedit(emitter, &preedit).await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }

    async fn send_preedit(
        &self,
        emitter: &SignalEmitter<'_>,
        text: &str,
    ) -> zbus::fdo::Result<()> {
        let cursor = text.chars().count() as u32;
        let visible = !text.is_empty();
        Self::update_preedit_text(emitter, ibus_text(text), cursor, visible).await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(())
    }

    async fn handle_backspace(
        &self,
        emitter: &SignalEmitter<'_>,
    ) -> zbus::fdo::Result<bool> {
        let new_preedit = {
            let mut engine = self.engine.lock().unwrap();
            let preedit = engine.preedit().to_string();
            if preedit.is_empty() {
                return Ok(false);
            }
            // Reset and re-type all but the last character
            let chars: Vec<char> = preedit.chars().collect();
            engine.reset();
            for &ch in &chars[..chars.len() - 1] {
                engine.process_key(ch);
            }
            engine.preedit().to_string()
        };

        if new_preedit.is_empty() {
            Self::hide_preedit_text(emitter).await
                .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        } else {
            self.send_preedit(emitter, &new_preedit).await?;
        }
        Ok(true)
    }
}

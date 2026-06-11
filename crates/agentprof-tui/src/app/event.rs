//! Input event abstraction.
//!
//! Wraps `crossterm::event::Event` into a minimal enum the `dispatch`
//! function consumes. Keeps `app::state` free of any direct crossterm
//! dependency, which makes the state machine unit-testable without a
//! terminal.

use crossterm::event::{Event as CtEvent, KeyCode, KeyEvent, KeyModifiers};

/// Normalized input event.
///
/// `Tick` is a placeholder for future periodic refresh (M1.5 only fires
/// on keystrokes). `Resize` carries the new (columns, rows) from
/// `crossterm::event::Event::Resize`.
///
/// **`Refresh` is reserved but currently has no producer** (TUI #1).
/// The original M1.6.3 design had [`crate::watch::WatchRunner::run`]
/// emit `Event::Refresh` after a refresh-channel hit, but the
/// implementation took a shortcut and handles the mpsc drain inline
/// via a `got_refresh` bool + direct `do_reload()` call (see
/// `watch.rs::run` near line 490). Kept in the public enum so a future
/// refactor that DOES produce it (e.g. exposing the refresh as a
/// user-visible event for animations / status footer) is non-breaking;
/// remove if a 1.0 cleanup decides the variant pays no rent.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Event {
    /// Key pressed (any modifier set).
    Key(KeyEvent),
    /// Terminal resized to (columns, rows).
    Resize(u16, u16),
    /// Periodic tick. Reserved for M2.x; no producer in the static event loop.
    #[allow(dead_code)]
    Tick,
    /// Watched session file changed (M1.6.3). Currently **has no producer
    /// in the codebase** (TUI #1): `WatchRunner::run` handles refreshes
    /// inline rather than via this variant. Reserved for a future
    /// refactor — see the enum-level doc for the full history.
    #[allow(dead_code)]
    Refresh,
}

impl Event {
    /// Convert a `crossterm` event into our [`Event`]. Mouse / focus / paste
    /// events are dropped (M1.5 is keyboard-only).
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_tui::app::event::Event;
    /// use crossterm::event::Event as CtEvent;
    /// assert_eq!(Event::from_crossterm(CtEvent::Resize(80, 24)), Some(Event::Resize(80, 24)));
    /// ```
    #[must_use]
    #[allow(clippy::needless_pass_by_value)]
    pub fn from_crossterm(ev: CtEvent) -> Option<Self> {
        match ev {
            // Polish #3: filter Release/Repeat key events — on Windows kitty
            // protocol / enhanced input mode, key release fires a second
            // KeyEvent that would double-toggle help_open etc. Press is
            // the only kind we want to act on for M1.5/M1.6.
            CtEvent::Key(k) if k.kind == crossterm::event::KeyEventKind::Press => {
                Some(Self::Key(k))
            }
            CtEvent::Key(_) => None,
            CtEvent::Resize(c, r) => Some(Self::Resize(c, r)),
            CtEvent::Mouse(_) | CtEvent::FocusGained | CtEvent::FocusLost | CtEvent::Paste(_) => {
                None
            }
        }
    }

    /// `true` if this event is Ctrl-C.
    ///
    /// # Examples
    ///
    /// ```
    /// use agentprof_tui::app::event::Event;
    /// use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    /// let e = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    /// assert!(e.is_ctrl_c());
    /// ```
    #[must_use]
    pub const fn is_ctrl_c(&self) -> bool {
        matches!(self, Self::Key(KeyEvent { code: KeyCode::Char('c'), modifiers, .. })
                 if modifiers.contains(KeyModifiers::CONTROL))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn key(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn ctrl_c_recognized() {
        let e = Event::Key(key(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(e.is_ctrl_c());
    }

    #[test]
    fn plain_c_is_not_ctrl_c() {
        let e = Event::Key(key(KeyCode::Char('c'), KeyModifiers::NONE));
        assert!(!e.is_ctrl_c());
    }

    #[test]
    fn from_crossterm_drops_mouse() {
        use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
        let me = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        assert!(Event::from_crossterm(CtEvent::Mouse(me)).is_none());
    }

    #[test]
    fn from_crossterm_preserves_resize() {
        assert_eq!(
            Event::from_crossterm(CtEvent::Resize(120, 40)),
            Some(Event::Resize(120, 40))
        );
    }

    #[test]
    fn from_crossterm_drops_key_release() {
        use crossterm::event::KeyEventKind;
        let k = KeyEvent::new_with_kind(
            KeyCode::Char('a'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        );
        assert!(Event::from_crossterm(CtEvent::Key(k)).is_none());
    }

    #[test]
    fn from_crossterm_drops_key_repeat() {
        use crossterm::event::KeyEventKind;
        let k =
            KeyEvent::new_with_kind(KeyCode::Char('a'), KeyModifiers::NONE, KeyEventKind::Repeat);
        assert!(Event::from_crossterm(CtEvent::Key(k)).is_none());
    }

    #[test]
    fn refresh_variant_distinct_from_tick() {
        let r1 = Event::Refresh;
        let r2 = Event::Refresh;
        assert_eq!(r1, r2);
        assert_ne!(Event::Refresh, Event::Tick);
    }

    #[test]
    fn from_crossterm_never_produces_refresh() {
        use crossterm::event::KeyEventKind;
        let k =
            KeyEvent::new_with_kind(KeyCode::Char('x'), KeyModifiers::NONE, KeyEventKind::Press);
        let got = Event::from_crossterm(CtEvent::Key(k));
        assert!(matches!(got, Some(Event::Key(_))));
        assert_ne!(got, Some(Event::Refresh));
    }
}

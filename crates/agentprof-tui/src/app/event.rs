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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Key pressed (any modifier set).
    Key(KeyEvent),
    /// Terminal resized to (columns, rows).
    Resize(u16, u16),
    /// Periodic tick. Reserved; unused in M1.5.
    Tick,
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
            CtEvent::Key(k) => Some(Self::Key(k)),
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
}

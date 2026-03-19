use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::App;

pub(crate) fn handle_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }
        KeyCode::Tab => app.next_panel(),
        KeyCode::Up => app.scroll_up(),
        KeyCode::Down => app.scroll_down(),
        _ => {}
    }
}

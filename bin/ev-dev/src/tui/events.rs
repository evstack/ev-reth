use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::{App, Panel};

pub(crate) fn handle_key(app: &mut App, key: KeyEvent) {
    // If block detail overlay is open, handle it separately
    if app.block_detail.is_some() {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => app.close_block_detail(),
            KeyCode::Char('q') => app.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.should_quit = true;
            }
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }
        KeyCode::Tab => app.next_panel(),
        KeyCode::Up => app.scroll_up(),
        KeyCode::Down => app.scroll_down(),
        KeyCode::Char('a') if app.active_panel == Panel::Accounts => {
            app.copy_account_address();
        }
        KeyCode::Char('k') if app.active_panel == Panel::Accounts => {
            app.copy_account_key();
        }
        KeyCode::Enter if app.active_panel == Panel::Blocks => {
            app.fetch_block_detail();
        }
        _ => {}
    }
}

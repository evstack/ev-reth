pub(crate) mod app;
mod events;
mod tracing_layer;
mod ui;

pub(crate) use app::{spawn_balance_poller, App};
pub(crate) use tracing_layer::TuiTracingLayer;

use std::io::{self, stdout};

use crossterm::{
    event::{Event, EventStream},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use futures::StreamExt;
use ratatui::prelude::CrosstermBackend;

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = stdout().execute(LeaveAlternateScreen);
    }
}

pub(crate) async fn run(mut app: App) -> eyre::Result<()> {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = stdout().execute(LeaveAlternateScreen);
        original_hook(info);
    }));

    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let _guard = TerminalGuard;

    let backend = CrosstermBackend::new(stdout());
    let mut terminal = ratatui::Terminal::new(backend)?;

    let mut event_stream = EventStream::new();
    let mut tick = tokio::time::interval(std::time::Duration::from_millis(100));

    loop {
        if app.should_quit {
            break;
        }

        tokio::select! {
            _ = tick.tick() => {
                app.drain_logs();
                app.drain_balances();
                app.drain_block_detail();
                terminal.draw(|frame| ui::draw(frame, &app))?;
            }
            maybe_event = event_stream.next() => {
                if let Some(Ok(Event::Key(key))) = maybe_event {
                    events::handle_key(&mut app, key);
                }
            }
        }
    }

    // Terminal restored by TerminalGuard drop
    Ok(())
}

pub(crate) fn restore_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

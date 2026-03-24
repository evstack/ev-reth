use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table},
    Frame,
};

use super::app::{App, BlockDetail, Panel};

fn border_style(app: &App, panel: Panel) -> Style {
    if app.active_panel == panel {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

const fn level_color(level: &tracing::Level) -> Color {
    match *level {
        tracing::Level::ERROR => Color::Red,
        tracing::Level::WARN => Color::Yellow,
        tracing::Level::INFO => Color::Green,
        tracing::Level::DEBUG | tracing::Level::TRACE => Color::DarkGray,
    }
}

fn format_uptime(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h}h{m:02}m{s:02}s")
    } else if m > 0 {
        format!("{m}m{s:02}s")
    } else {
        format!("{s}s")
    }
}

fn format_gas(gas: u64) -> String {
    if gas >= 1_000_000 {
        format!("{:.1}M", gas as f64 / 1_000_000.0)
    } else if gas >= 1_000 {
        format!("{:.1}k", gas as f64 / 1_000.0)
    } else {
        gas.to_string()
    }
}

pub(crate) fn draw(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();

    let outer = Layout::vertical([
        Constraint::Length(3), // header
        Constraint::Min(6),    // main content
        Constraint::Length(3), // footer
    ])
    .split(area);

    draw_header(frame, app, outer[0]);
    draw_main(frame, app, outer[1]);
    draw_footer(frame, app, outer[2]);

    if let Some(ref detail) = app.block_detail {
        draw_block_detail(frame, detail, area);
    }
}

fn draw_header(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let block_time_str = if app.block_time == 0 {
        "auto".to_string()
    } else {
        format!("{}s", app.block_time)
    };

    let text = Line::from(vec![
        Span::styled(" Chain: ", Style::default().fg(Color::DarkGray)),
        Span::styled(app.chain_id.to_string(), Style::default().fg(Color::White)),
        Span::styled("  RPC: ", Style::default().fg(Color::DarkGray)),
        Span::styled(&app.rpc_url, Style::default().fg(Color::Cyan)),
        Span::styled("  Block: ", Style::default().fg(Color::DarkGray)),
        Span::styled(block_time_str, Style::default().fg(Color::White)),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" ev-dev ")
        .title_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .border_style(Style::default().fg(Color::Cyan));

    let paragraph = Paragraph::new(text).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_main(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let main_split = Layout::vertical([
        Constraint::Percentage(45), // top row (blocks + accounts)
        Constraint::Percentage(55), // logs
    ])
    .split(area);

    let top_split = Layout::horizontal([
        Constraint::Percentage(55), // blocks
        Constraint::Percentage(45), // accounts
    ])
    .split(main_split[0]);

    draw_blocks(frame, app, top_split[0]);
    draw_accounts(frame, app, top_split[1]);
    draw_logs(frame, app, main_split[1]);
}

fn draw_blocks(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let is_focused = app.active_panel == Panel::Blocks;

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Blocks ")
        .border_style(border_style(app, Panel::Blocks));

    let header = Row::new(vec![
        Cell::from("Block").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Hash").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Txs").style(Style::default().add_modifier(Modifier::BOLD)),
        Cell::from("Gas").style(Style::default().add_modifier(Modifier::BOLD)),
    ])
    .style(Style::default().fg(Color::DarkGray));

    // Auto-scroll to keep selected block visible
    let inner_height = area.height.saturating_sub(4) as usize; // borders + header + header separator
    let scroll = if inner_height > 0 && app.block_selected >= inner_height {
        app.block_selected - inner_height + 1
    } else {
        0
    };

    let rows: Vec<Row<'_>> = app
        .blocks
        .iter()
        .enumerate()
        .skip(scroll)
        .take(inner_height.max(1))
        .map(|(i, b)| {
            let selected = is_focused && i == app.block_selected;
            let marker = if selected { "▸" } else { " " };
            let style = if selected {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(format!("{marker}#{}", b.number)),
                Cell::from(b.hash.clone()).style(Style::default().fg(Color::DarkGray)),
                Cell::from(format!("{}", b.tx_count)),
                Cell::from(format_gas(b.gas_used)),
            ])
            .style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(10),
        Constraint::Length(12),
        Constraint::Length(5),
        Constraint::Min(6),
    ];

    let table = Table::new(rows, widths).header(header).block(block);

    frame.render_widget(table, area);
}

fn draw_accounts(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let is_focused = app.active_panel == Panel::Accounts;
    let mut items: Vec<ListItem<'_>> = app
        .accounts
        .iter()
        .enumerate()
        .map(|(i, (addr, _key))| {
            let truncated = if addr.len() > 10 {
                format!("{}..{}", &addr[..6], &addr[addr.len() - 4..])
            } else {
                addr.clone()
            };
            let balance = app
                .balances
                .get(i)
                .cloned()
                .unwrap_or_else(|| "? ETH".to_string());

            let selected = is_focused && i == app.account_selected;
            let marker = if selected { "▸ " } else { "  " };
            let addr_color = if selected { Color::Cyan } else { Color::White };

            ListItem::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(Color::Cyan)),
                Span::styled(format!("({i}) "), Style::default().fg(Color::DarkGray)),
                Span::styled(truncated, Style::default().fg(addr_color)),
                Span::styled(format!(" {balance}"), Style::default().fg(Color::Green)),
            ]))
        })
        .collect();

    if let Some(ref contracts) = app.deploy_contracts {
        items.push(ListItem::new(Line::from("")));
        items.push(ListItem::new(Line::from(Span::styled(
            "Genesis Contracts",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ))));
        for (name, addr) in contracts {
            let truncated = if addr.len() > 10 {
                format!("{}..{}", &addr[..6], &addr[addr.len() - 4..])
            } else {
                addr.clone()
            };
            items.push(ListItem::new(Line::from(vec![
                Span::styled(format!("{name:18} "), Style::default().fg(Color::DarkGray)),
                Span::styled(truncated, Style::default().fg(Color::White)),
            ])));
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Accounts ")
        .border_style(border_style(app, Panel::Accounts));

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn draw_logs(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Logs ")
        .border_style(border_style(app, Panel::Logs));

    let inner_height = area.height.saturating_sub(2) as usize;
    let total = app.logs.len();

    let end = total.saturating_sub(app.log_scroll);
    let start = end.saturating_sub(inner_height);

    let items: Vec<ListItem<'_>> = app
        .logs
        .iter()
        .skip(start)
        .take(end.saturating_sub(start))
        .map(|entry| {
            let color = level_color(&entry.level);
            let level_str = format!("{:5}", entry.level);
            let elapsed = entry.timestamp.elapsed().as_secs();
            let ts = format!("{elapsed:>4}s");

            let target_short = entry.target.rsplit("::").next().unwrap_or(&entry.target);

            let mut spans = vec![
                Span::styled(ts, Style::default().fg(Color::DarkGray)),
                Span::raw(" "),
                Span::styled(level_str, Style::default().fg(color)),
                Span::raw(" "),
                Span::styled(
                    format!("{target_short:>16} "),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(entry.message.clone(), Style::default().fg(Color::White)),
            ];

            for (k, v) in &entry.fields {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    format!("{k}="),
                    Style::default().fg(Color::DarkGray),
                ));
                spans.push(Span::styled(v.clone(), Style::default().fg(Color::Gray)));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn draw_block_detail(frame: &mut Frame<'_>, detail: &BlockDetail, area: Rect) {
    let popup = centered_rect(80, 60, area);
    frame.render_widget(Clear, popup);

    let title = format!(
        " Block #{} ({} txs) ",
        detail.number,
        detail.txs.len()
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .border_style(Style::default().fg(Color::Cyan));

    if detail.txs.is_empty() {
        let text = Paragraph::new(Line::from(vec![
            Span::styled(
                "  No transactions in this block",
                Style::default().fg(Color::DarkGray),
            ),
        ]))
        .block(block);
        frame.render_widget(text, popup);
    } else {
        let header = Row::new(vec![
            Cell::from("Hash").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("From").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("To").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Value").style(Style::default().add_modifier(Modifier::BOLD)),
        ])
        .style(Style::default().fg(Color::DarkGray));

        let rows: Vec<Row<'_>> = detail
            .txs
            .iter()
            .map(|tx| {
                Row::new(vec![
                    Cell::from(tx.hash.clone()).style(Style::default().fg(Color::DarkGray)),
                    Cell::from(tx.from.clone()),
                    Cell::from(tx.to.clone()),
                    Cell::from(tx.value.clone()).style(Style::default().fg(Color::Green)),
                ])
            })
            .collect();

        let widths = [
            Constraint::Length(14),
            Constraint::Length(14),
            Constraint::Length(18),
            Constraint::Min(10),
        ];

        let table = Table::new(rows, widths).header(header).block(block);
        frame.render_widget(table, popup);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(r);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(popup_layout[1])[1]
}

fn draw_footer(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let uptime = app.start_time.elapsed().as_secs();

    // Check for clipboard flash message (show for 2 seconds)
    let clipboard_flash = app.clipboard_msg.as_ref().and_then(|(msg, when)| {
        if when.elapsed().as_secs() < 2 {
            Some(msg.clone())
        } else {
            None
        }
    });

    let mut spans = vec![
        Span::styled(" Up: ", Style::default().fg(Color::DarkGray)),
        Span::styled(format_uptime(uptime), Style::default().fg(Color::White)),
        Span::styled("  Block: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("#{}", app.current_block),
            Style::default().fg(Color::Cyan),
        ),
        Span::styled("  |  ", Style::default().fg(Color::DarkGray)),
        Span::styled("[q]", Style::default().fg(Color::Yellow)),
        Span::styled("uit  ", Style::default().fg(Color::DarkGray)),
        Span::styled("[Tab]", Style::default().fg(Color::Yellow)),
        Span::styled("focus  ", Style::default().fg(Color::DarkGray)),
    ];

    if app.block_detail.is_some() {
        spans.extend([
            Span::styled("[Esc]", Style::default().fg(Color::Yellow)),
            Span::styled("close", Style::default().fg(Color::DarkGray)),
        ]);
    } else {
        match app.active_panel {
            Panel::Accounts => {
                spans.extend([
                    Span::styled("[↑↓]", Style::default().fg(Color::Yellow)),
                    Span::styled("select  ", Style::default().fg(Color::DarkGray)),
                    Span::styled("[a]", Style::default().fg(Color::Yellow)),
                    Span::styled("ddress  ", Style::default().fg(Color::DarkGray)),
                    Span::styled("[k]", Style::default().fg(Color::Yellow)),
                    Span::styled("ey", Style::default().fg(Color::DarkGray)),
                ]);
            }
            Panel::Blocks => {
                spans.extend([
                    Span::styled("[↑↓]", Style::default().fg(Color::Yellow)),
                    Span::styled("select  ", Style::default().fg(Color::DarkGray)),
                    Span::styled("[Enter]", Style::default().fg(Color::Yellow)),
                    Span::styled("txs", Style::default().fg(Color::DarkGray)),
                ]);
            }
            Panel::Logs => {
                spans.extend([
                    Span::styled("[↑↓]", Style::default().fg(Color::Yellow)),
                    Span::styled("scroll", Style::default().fg(Color::DarkGray)),
                ]);
            }
        }
    }

    if let Some(msg) = clipboard_flash {
        spans.extend([
            Span::styled("  ", Style::default()),
            Span::styled(
                format!("✓ {msg}"),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
    }

    let text = Line::from(spans);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let paragraph = Paragraph::new(text).block(block);
    frame.render_widget(paragraph, area);
}

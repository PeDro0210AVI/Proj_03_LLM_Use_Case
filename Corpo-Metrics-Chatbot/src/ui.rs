use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, Role};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(frame, chunks[0], app);
    draw_messages(frame, chunks[1], app);
    draw_status(frame, chunks[2], app);
    draw_input(frame, chunks[3], app);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().borders(Borders::ALL).title("session");
    let text = Paragraph::new(app.header.as_str())
        .style(Style::default().add_modifier(Modifier::BOLD))
        .block(block);
    frame.render_widget(text, area);
}

fn role_style(role: Role) -> (Style, &'static str) {
    match role {
        Role::User => (Style::default().fg(Color::Cyan), "you"),
        Role::Assistant => (Style::default().fg(Color::Green), "claude"),
        Role::Tool => (Style::default().fg(Color::Yellow), "mcp"),
        Role::System => (Style::default().fg(Color::DarkGray), "system"),
    }
}

fn draw_messages(frame: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default().borders(Borders::ALL).title("conversation");
    let inner_width = area.width.saturating_sub(2).max(1) as usize;

    let mut all_lines: Vec<Line> = Vec::new();
    for msg in &app.display {
        let (style, label) = role_style(msg.role);
        let prefix = format!("{label}: ");
        let wrap_width = inner_width.saturating_sub(prefix.len()).max(10);

        let wrapped = textwrap::wrap(&msg.text, wrap_width);
        if wrapped.is_empty() {
            all_lines.push(Line::from(Span::styled(prefix.clone(), style)));
            continue;
        }
        for (i, line) in wrapped.iter().enumerate() {
            if i == 0 {
                all_lines.push(Line::from(vec![
                    Span::styled(prefix.clone(), style),
                    Span::raw(line.to_string()),
                ]));
            } else {
                let indent = " ".repeat(prefix.len());
                all_lines.push(Line::from(vec![
                    Span::raw(indent),
                    Span::raw(line.to_string()),
                ]));
            }
        }
    }

    let total_lines = all_lines.len();
    let viewport_height = area.height.saturating_sub(2) as usize;
    let max_scroll = total_lines.saturating_sub(viewport_height);
    app.max_scroll = max_scroll;

    let effective_offset = app.scroll_offset.min(max_scroll);
    let top = max_scroll.saturating_sub(effective_offset);
    let bottom = (top + viewport_height).min(total_lines);
    let visible: Vec<Line> = all_lines[top..bottom].to_vec();

    let paragraph = Paragraph::new(visible).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_status(frame: &mut Frame, area: Rect, app: &App) {
    let text = if let Some(status) = &app.status {
        format!(" ⏳ {status}")
    } else if app.is_streaming {
        " ⏳ waiting for claude...".to_string()
    } else if !app.follow {
        " ↑ scrolled up — press End to jump to latest".to_string()
    } else {
        " ready".to_string()
    };
    let style = if app.is_streaming || app.status.is_some() {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    frame.render_widget(Paragraph::new(text).style(style), area);
}

fn draw_input(frame: &mut Frame, area: Rect, app: &App) {
    let title = if app.is_streaming {
        "message (waiting for reply...)"
    } else {
        "message (Enter to send, Esc to quit)"
    };
    let block = Block::default().borders(Borders::ALL).title(title);
    let paragraph = Paragraph::new(app.input.as_str()).block(block);
    frame.render_widget(paragraph, area);

    if !app.is_streaming {
        let cursor_x = area.x + 1 + app.input.len() as u16;
        let cursor_y = area.y + 1;
        frame.set_cursor_position((cursor_x.min(area.x + area.width.saturating_sub(2)), cursor_y));
    }
}

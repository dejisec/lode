use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use super::App;
use super::markdown::MarkdownRenderer;

#[derive(Clone)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

#[derive(Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
}

pub fn render_ui(frame: &mut Frame, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .split(frame.area());

    render_chat_area(frame, app, chunks[0]);
    render_input(frame, app, chunks[1]);
    render_status_bar(frame, app, chunks[2]);
}

fn render_chat_area(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Lode Research ")
        .title_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.messages.is_empty() {
        let welcome = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "Welcome to Lode",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Enter a research query below to get started.",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("Press ", Style::default().fg(Color::DarkGray)),
                Span::styled("Enter", Style::default().fg(Color::Yellow)),
                Span::styled(" to submit, ", Style::default().fg(Color::DarkGray)),
                Span::styled("Esc", Style::default().fg(Color::Yellow)),
                Span::styled(" to quit", Style::default().fg(Color::DarkGray)),
            ]),
        ])
        .centered();
        frame.render_widget(welcome, inner);
        return;
    }

    let md_renderer = MarkdownRenderer::new();
    let mut lines: Vec<Line> = Vec::new();

    for msg in &app.messages {
        match msg.role {
            MessageRole::User => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "You: ",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(&msg.content, Style::default().fg(Color::White)),
                ]));
            }
            MessageRole::System => {
                lines.push(Line::from(vec![
                    Span::styled("→ ", Style::default().fg(Color::Yellow)),
                    Span::styled(&msg.content, Style::default().fg(Color::DarkGray)),
                ]));
            }
            MessageRole::Assistant => {
                lines.push(Line::from(Span::styled(
                    "Lode:",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )));
                let rendered = md_renderer.render(&msg.content);
                for line in rendered.lines {
                    lines.push(line);
                }
            }
        }
        lines.push(Line::from(""));
    }

    let visible_height = inner.height as usize;
    let total_lines = lines.len();
    let max_scroll = total_lines.saturating_sub(visible_height);

    // scroll_offset is "lines from bottom": 0 = at bottom, max = at top
    let scroll = max_scroll.saturating_sub(app.scroll_offset);

    let paragraph = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((scroll as u16, 0));

    frame.render_widget(paragraph, inner);
}

pub fn calculate_total_lines(app: &App) -> usize {
    let md_renderer = MarkdownRenderer::new();
    let mut count = 0;
    for msg in &app.messages {
        match msg.role {
            MessageRole::User | MessageRole::System => {
                count += 1; // single line for user/system messages
            }
            MessageRole::Assistant => {
                count += 1; // "Lode:" prefix line
                let rendered = md_renderer.render(&msg.content);
                count += rendered.lines.len();
            }
        }
        count += 1; // blank line after each message
    }
    count
}

fn render_input(frame: &mut Frame, app: &App, area: Rect) {
    let (border_color, title) = if app.is_processing {
        (Color::Yellow, " Processing... ")
    } else {
        (Color::Cyan, " Query ")
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title)
        .title_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let display_text = if app.is_processing {
        app.status.clone().unwrap_or_default()
    } else {
        format!("{}▏", app.input)
    };

    let text_style = if app.is_processing {
        Style::default().fg(Color::DarkGray).italic()
    } else {
        Style::default().fg(Color::White)
    };

    let paragraph = Paragraph::new(Span::styled(display_text, text_style));
    frame.render_widget(paragraph, inner);
}

fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let status_text = if let Some(ref status) = app.status {
        vec![
            Span::styled("● ", Style::default().fg(Color::Yellow)),
            Span::styled(status.as_str(), Style::default().fg(Color::DarkGray)),
        ]
    } else if app.is_processing {
        vec![
            Span::styled("● ", Style::default().fg(Color::Yellow)),
            Span::styled("Working...", Style::default().fg(Color::DarkGray)),
        ]
    } else {
        vec![
            Span::styled("↑↓", Style::default().fg(Color::DarkGray)),
            Span::styled(" scroll  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Enter", Style::default().fg(Color::DarkGray)),
            Span::styled(" submit  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::DarkGray)),
            Span::styled(" quit", Style::default().fg(Color::DarkGray)),
        ]
    };

    let paragraph = Paragraph::new(Line::from(status_text));
    frame.render_widget(paragraph, area);
}

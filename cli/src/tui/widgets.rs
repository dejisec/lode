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
    let available_width = inner.width.saturating_sub(0) as usize; // no extra padding needed

    // Calculate wrapped line count for proper scrolling
    let total_lines: usize = lines
        .iter()
        .map(|line| wrapped_line_count(line, available_width))
        .sum();

    let max_scroll = total_lines.saturating_sub(visible_height);

    // scroll_offset is "lines from bottom": 0 = at bottom, max = at top
    let scroll = max_scroll.saturating_sub(app.scroll_offset);

    let paragraph = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((scroll as u16, 0));

    frame.render_widget(paragraph, inner);
}

fn wrapped_line_count(line: &Line, width: usize) -> usize {
    if width == 0 {
        return 1;
    }
    let char_count: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
    if char_count == 0 {
        return 1;
    }
    char_count.div_ceil(width)
}

pub fn calculate_total_lines(app: &App, width: u16) -> usize {
    let md_renderer = MarkdownRenderer::new();
    let available_width = width.saturating_sub(2) as usize; // account for borders
    let mut count = 0;

    for msg in &app.messages {
        match msg.role {
            MessageRole::User => {
                let prefix_len = 5; // "You: "
                let content_len = msg.content.chars().count();
                let total_len = prefix_len + content_len;
                count += if available_width > 0 {
                    total_len.div_ceil(available_width)
                } else {
                    1
                };
            }
            MessageRole::System => {
                let prefix_len = 2; // "→ "
                let content_len = msg.content.chars().count();
                let total_len = prefix_len + content_len;
                count += if available_width > 0 {
                    total_len.div_ceil(available_width)
                } else {
                    1
                };
            }
            MessageRole::Assistant => {
                count += 1; // "Lode:" prefix line
                let rendered = md_renderer.render(&msg.content);
                for line in &rendered.lines {
                    count += wrapped_line_count(line, available_width);
                }
            }
        }
        count += 1; // blank line after each message
    }
    count
}

fn render_input(frame: &mut Frame, app: &App, area: Rect) {
    let (border_color, title) = if app.is_clarifying() {
        if let Some(q) = app.current_question() {
            (Color::Magenta, format!(" {} ", q.label))
        } else {
            (Color::Magenta, " Answer ".to_string())
        }
    } else if app.is_confirming() || app.awaiting_confirmation() {
        (Color::Magenta, " Confirm ".to_string())
    } else if app.is_processing {
        (Color::Yellow, " Processing... ".to_string())
    } else {
        (Color::Cyan, " Query ".to_string())
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title)
        .title_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let display_text = if app.is_clarifying() {
        format!("{}▏", app.input)
    } else if app.awaiting_confirmation() {
        format!("{}▏", app.input)
    } else if app.is_processing {
        app.status.clone().unwrap_or_default()
    } else {
        format!("{}▏", app.input)
    };

    let text_style = if app.is_processing && !app.is_clarifying() && !app.awaiting_confirmation() {
        Style::default().fg(Color::DarkGray).italic()
    } else {
        Style::default().fg(Color::White)
    };

    let paragraph = Paragraph::new(Span::styled(display_text, text_style));
    frame.render_widget(paragraph, inner);
}

fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let message = if let Some(ref status) = app.status {
        status.clone()
    } else {
        app.default_status()
    };

    let spans = vec![
        Span::styled(
            format!(" {} ", app.spinner_frame()),
            Style::default().fg(Color::Yellow),
        ),
        Span::styled(
            format!("[{}] ", app.phase_label()),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(message, Style::default().fg(Color::DarkGray)),
    ];

    let paragraph = Paragraph::new(Line::from(spans));
    frame.render_widget(paragraph, area);
}

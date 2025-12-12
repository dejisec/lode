mod markdown;
mod widgets;

use std::io::{self, Stdout};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::prelude::CrosstermBackend;
use tokio::sync::mpsc;

use crate::protocol::Response;
use widgets::{ChatMessage, MessageRole, calculate_total_lines, render_ui};

pub struct App {
    pub messages: Vec<ChatMessage>,
    pub input: String,
    pub status: Option<String>,
    pub is_processing: bool,
    pub scroll_offset: usize,
    pub should_quit: bool,
    event_rx: Option<mpsc::UnboundedReceiver<AppEvent>>,
    event_tx: mpsc::UnboundedSender<AppEvent>,
}

pub enum AppEvent {
    BackendResponse(Response),
    RunComplete { success: bool, run_id: String },
    Error(String),
}

impl App {
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        Self {
            messages: Vec::new(),
            input: String::new(),
            status: None,
            is_processing: false,
            scroll_offset: 0,
            should_quit: false,
            event_rx: Some(event_rx),
            event_tx,
        }
    }

    pub fn event_sender(&self) -> mpsc::UnboundedSender<AppEvent> {
        self.event_tx.clone()
    }

    pub fn add_user_message(&mut self, content: String) {
        self.messages.push(ChatMessage {
            role: MessageRole::User,
            content,
        });
        self.scroll_to_bottom();
    }

    pub fn add_assistant_message(&mut self, content: String) {
        self.messages.push(ChatMessage {
            role: MessageRole::Assistant,
            content,
        });
        self.scroll_to_bottom();
    }

    pub fn add_system_message(&mut self, content: String) {
        self.messages.push(ChatMessage {
            role: MessageRole::System,
            content,
        });
        self.scroll_to_bottom();
    }

    pub fn set_status(&mut self, status: Option<String>) {
        self.status = status;
    }

    fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0; // 0 = at bottom
    }

    fn handle_backend_event(&mut self, response: Response) {
        match response {
            Response::Status { message } => {
                self.set_status(Some(message));
            }
            Response::Trace { trace_url, .. } => {
                self.add_system_message(format!("Trace: {}", trace_url));
            }
            Response::Prompt {
                agent, sequence, ..
            } => {
                self.set_status(Some(format!("Running {} (step {})", agent, sequence)));
            }
            Response::AgentOutput {
                agent, sequence, ..
            } => {
                self.set_status(Some(format!(
                    "Received {} response (step {})",
                    agent, sequence
                )));
            }
            Response::Report {
                short_summary,
                markdown_report,
                ..
            } => {
                self.set_status(None);
                let content = format!("**{}**\n\n{}", short_summary, markdown_report);
                self.add_assistant_message(content);
            }
            Response::Error { message, code } => {
                let msg = if let Some(c) = code {
                    format!("Error [{}]: {}", c, message)
                } else {
                    format!("Error: {}", message)
                };
                self.add_system_message(msg);
            }
            Response::Done { .. } | Response::Metadata { .. } => {}
        }
    }

    fn process_events(&mut self) {
        let events: Vec<AppEvent> = if let Some(ref mut rx) = self.event_rx {
            let mut collected = Vec::new();
            while let Ok(event) = rx.try_recv() {
                collected.push(event);
            }
            collected
        } else {
            Vec::new()
        };

        for event in events {
            match event {
                AppEvent::BackendResponse(response) => {
                    self.handle_backend_event(response);
                }
                AppEvent::RunComplete { success, run_id } => {
                    self.is_processing = false;
                    self.set_status(None);
                    if success {
                        self.add_system_message(format!("Research complete ({})", &run_id[..8]));
                    } else {
                        self.add_system_message("Research failed".to_string());
                    }
                }
                AppEvent::Error(msg) => {
                    self.is_processing = false;
                    self.set_status(None);
                    self.add_system_message(format!("Error: {}", msg));
                }
            }
        }
    }
}

pub struct Tui {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl Tui {
    pub fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }

    pub fn restore(&mut self) -> io::Result<()> {
        disable_raw_mode()?;
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)?;
        Ok(())
    }

    pub async fn run<F>(&mut self, app: &mut App, mut on_submit: F) -> io::Result<()>
    where
        F: FnMut(&str) + Send,
    {
        loop {
            app.process_events();

            self.terminal.draw(|frame| {
                render_ui(frame, app);
            })?;

            if app.should_quit {
                break;
            }

            if event::poll(Duration::from_millis(50))?
                && let Event::Key(key) = event::read()?
            {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                match key.code {
                    KeyCode::Esc => {
                        app.should_quit = true;
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.should_quit = true;
                    }
                    KeyCode::Enter => {
                        if !app.input.is_empty() && !app.is_processing {
                            let query = app.input.clone();
                            app.input.clear();
                            app.add_user_message(query.clone());
                            app.is_processing = true;
                            app.set_status(Some("Starting research...".to_string()));
                            on_submit(&query);
                        }
                    }
                    KeyCode::Backspace => {
                        app.input.pop();
                    }
                    KeyCode::Char(c) => {
                        if !app.is_processing {
                            app.input.push(c);
                        }
                    }
                    KeyCode::Up => {
                        let total_lines = calculate_total_lines(app);
                        let max_scroll = total_lines.saturating_sub(1);
                        if app.scroll_offset < max_scroll {
                            app.scroll_offset += 3;
                            app.scroll_offset = app.scroll_offset.min(max_scroll);
                        }
                    }
                    KeyCode::Down => {
                        if app.scroll_offset > 0 {
                            app.scroll_offset = app.scroll_offset.saturating_sub(3);
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

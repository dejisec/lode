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

use crate::protocol::{ClarifyingQuestion, Response};
use widgets::{ChatMessage, MessageRole, calculate_total_lines, render_ui};

const SPINNER_FRAMES: [&str; 4] = ["-", "\\", "|", "/"];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AppPhase {
    Idle,
    AwaitingClarification,
    Clarifying,
    Confirming,
    Researching,
    Completed,
    Error,
}

#[derive(Clone)]
pub struct ClarifyingState {
    pub questions: Vec<ClarifyingQuestion>,
    pub current_index: usize,
    pub answers: Vec<String>,
}

pub struct App {
    pub messages: Vec<ChatMessage>,
    pub input: String,
    pub status: Option<String>,
    pub is_processing: bool,
    pub scroll_offset: usize,
    pub should_quit: bool,
    pub clarifying: Option<ClarifyingState>,
    pub terminal_width: u16,
    pub require_confirmation: bool,
    pub pending_answers: Option<Vec<String>>,
    pub phase: AppPhase,
    spinner_index: usize,
    event_rx: Option<mpsc::UnboundedReceiver<AppEvent>>,
    event_tx: mpsc::UnboundedSender<AppEvent>,
}

pub enum AppEvent {
    BackendResponse(Response),
    RunComplete { success: bool, run_id: String },
    Error(String),
}

impl App {
    pub fn new(require_confirmation: bool) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        Self {
            messages: Vec::new(),
            input: String::new(),
            status: None,
            is_processing: false,
            scroll_offset: 0,
            should_quit: false,
            clarifying: None,
            terminal_width: 80, // default, updated on each draw
            require_confirmation,
            pending_answers: None,
            phase: AppPhase::Idle,
            spinner_index: 0,
            event_rx: Some(event_rx),
            event_tx,
        }
    }

    pub fn is_clarifying(&self) -> bool {
        self.clarifying.is_some()
    }

    pub fn awaiting_confirmation(&self) -> bool {
        self.pending_answers.is_some()
    }

    pub fn current_question(&self) -> Option<&ClarifyingQuestion> {
        self.clarifying
            .as_ref()
            .and_then(|state| state.questions.get(state.current_index))
    }

    pub fn is_confirming(&self) -> bool {
        matches!(self.phase, AppPhase::Confirming)
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

    fn spinner_frame(&self) -> &str {
        if self.is_processing {
            SPINNER_FRAMES[self.spinner_index % SPINNER_FRAMES.len()]
        } else {
            " "
        }
    }

    fn advance_spinner(&mut self) {
        if self.is_processing {
            self.spinner_index = self.spinner_index.wrapping_add(1);
        } else {
            self.spinner_index = 0;
        }
    }

    fn phase_label(&self) -> &'static str {
        match self.phase {
            AppPhase::Idle => "Idle",
            AppPhase::AwaitingClarification => "Clarifying",
            AppPhase::Clarifying => "Clarifying",
            AppPhase::Confirming => "Confirm",
            AppPhase::Researching => "Researching",
            AppPhase::Completed => "Complete",
            AppPhase::Error => "Error",
        }
    }

    fn default_status(&self) -> String {
        match self.phase {
            AppPhase::Idle => "Type a query and press Enter. Esc to quit.".to_string(),
            AppPhase::AwaitingClarification => "Generating clarifying questions...".to_string(),
            AppPhase::Clarifying => {
                if let Some(state) = &self.clarifying {
                    format!(
                        "Answer question {} of {}",
                        state.current_index + 1,
                        state.questions.len()
                    )
                } else {
                    "Answer the clarifying questions.".to_string()
                }
            }
            AppPhase::Confirming => "Type 'confirm' to continue or 'cancel' to abort.".to_string(),
            AppPhase::Researching => "Working... Press Esc to stop.".to_string(),
            AppPhase::Completed => {
                "Research complete. Enter another query or Esc to quit.".to_string()
            }
            AppPhase::Error => "Run failed. Enter another query or Esc to quit.".to_string(),
        }
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
            Response::ClarifyingQuestions { questions } => {
                self.add_system_message("Please answer these clarifying questions:".to_string());
                for (i, q) in questions.iter().enumerate() {
                    self.add_system_message(format!("{}. [{}] {}", i + 1, q.label, q.question));
                }
                self.clarifying = Some(ClarifyingState {
                    questions: questions.clone(),
                    current_index: 0,
                    answers: Vec::new(),
                });
                self.set_status(None);
                self.phase = AppPhase::Clarifying;
            }
            Response::Prompt {
                agent, sequence, ..
            } => {
                self.set_status(Some(format!("Running {} (step {})", agent, sequence)));
                self.phase = AppPhase::Researching;
            }
            Response::AgentOutput {
                agent, sequence, ..
            } => {
                self.set_status(Some(format!(
                    "Received {} response (step {})",
                    agent, sequence
                )));
                self.phase = AppPhase::Researching;
            }
            Response::Decision {
                action,
                remaining_searches,
                remaining_iterations,
                ..
            } => {
                self.set_status(Some(format!(
                    "Decision: {} (budget: {} searches, {} iterations)",
                    action, remaining_searches, remaining_iterations
                )));
                self.phase = AppPhase::Researching;
            }
            Response::Report {
                short_summary,
                markdown_report,
                ..
            } => {
                self.set_status(None);
                self.phase = AppPhase::Completed;
                let content = format!("**{}**\n\n{}", short_summary, markdown_report);
                self.add_assistant_message(content);
            }
            Response::Error { message, code } => {
                let msg = if let Some(c) = code {
                    format!("Error [{}]: {}", c, message)
                } else {
                    format!("Error: {}", message)
                };
                self.phase = AppPhase::Error;
                self.add_system_message(msg);
            }
            Response::Done { .. } | Response::Metadata { .. } => {}
        }
    }

    fn process_events(&mut self) {
        self.advance_spinner();
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
                    self.phase = if success {
                        AppPhase::Completed
                    } else {
                        AppPhase::Error
                    };
                    if success {
                        self.add_system_message(format!("Research complete ({})", &run_id[..8]));
                    } else {
                        self.add_system_message("Research failed".to_string());
                    }
                }
                AppEvent::Error(msg) => {
                    self.is_processing = false;
                    self.set_status(None);
                    self.phase = AppPhase::Error;
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

    pub async fn run<F, G, H>(
        &mut self,
        app: &mut App,
        mut on_submit: F,
        mut on_answers: G,
        mut on_interrupt: H,
    ) -> io::Result<()>
    where
        F: FnMut(&str) + Send,
        G: FnMut(Vec<String>, bool) + Send,
        H: FnMut() + Send,
    {
        loop {
            app.process_events();

            let size = self.terminal.size()?;
            app.terminal_width = size.width;

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
                        if app.is_processing {
                            on_interrupt();
                            app.add_system_message("Stopping research...".to_string());
                            app.phase = AppPhase::Researching;
                        } else {
                            app.should_quit = true;
                        }
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if app.is_processing {
                            on_interrupt();
                            app.add_system_message("Stopping research...".to_string());
                            app.phase = AppPhase::Researching;
                        }
                        app.should_quit = true;
                    }
                    KeyCode::Enter => {
                        if app.awaiting_confirmation() {
                            let user_input = app.input.trim().to_string();
                            let lowered = user_input.to_lowercase();
                            app.input.clear();

                            if !user_input.is_empty() {
                                app.add_user_message(user_input.clone());
                            }

                            let confirmed = matches!(
                                lowered.as_str(),
                                "" | "y" | "yes" | "confirm" | "continue" | "proceed"
                            );
                            let cancelled =
                                matches!(lowered.as_str(), "n" | "no" | "cancel" | "stop" | "quit");

                            if confirmed {
                                if let Some(answers) = app.pending_answers.take() {
                                    app.set_status(Some("Continuing research...".to_string()));
                                    app.phase = AppPhase::Researching;
                                    on_answers(answers, true);
                                }
                            } else if cancelled {
                                if let Some(answers) = app.pending_answers.take() {
                                    app.set_status(Some("Cancelling research...".to_string()));
                                    app.add_system_message(
                                        "Research cancelled before execution.".to_string(),
                                    );
                                    app.phase = AppPhase::Completed;
                                    on_answers(answers, false);
                                }
                            } else {
                                app.add_system_message(
                                    "Type 'confirm' to continue or 'cancel' to abort.".to_string(),
                                );
                            }
                            continue;
                        }

                        if app.is_clarifying() {
                            let answer = app.input.clone();
                            app.input.clear();
                            app.add_user_message(answer.clone());

                            let (is_complete, answers, next_index, total) = {
                                let state = app.clarifying.as_mut().unwrap();
                                state.answers.push(answer);
                                state.current_index += 1;
                                let complete = state.current_index >= state.questions.len();
                                let answers = if complete {
                                    Some(state.answers.clone())
                                } else {
                                    None
                                };
                                (
                                    complete,
                                    answers,
                                    state.current_index,
                                    state.questions.len(),
                                )
                            };

                            if is_complete {
                                app.clarifying = None;
                                if app.require_confirmation {
                                    if let Some(answers) = answers {
                                        app.pending_answers = Some(answers);
                                        app.add_system_message(
                                            "Type 'confirm' to continue or 'cancel' to abort."
                                                .to_string(),
                                        );
                                        app.set_status(Some(
                                            "Awaiting confirmation...".to_string(),
                                        ));
                                        app.phase = AppPhase::Confirming;
                                    }
                                } else {
                                    app.set_status(Some("Continuing research...".to_string()));
                                    app.phase = AppPhase::Researching;
                                    if let Some(answers) = answers {
                                        on_answers(answers, true);
                                    }
                                }
                            } else {
                                app.set_status(Some(format!(
                                    "Answer question {} of {}",
                                    next_index + 1,
                                    total
                                )));
                                app.phase = AppPhase::Clarifying;
                            }
                        } else if !app.input.is_empty() && !app.is_processing {
                            let query = app.input.clone();
                            app.input.clear();
                            app.add_user_message(query.clone());
                            app.is_processing = true;
                            app.set_status(Some("Starting research...".to_string()));
                            app.phase = AppPhase::AwaitingClarification;
                            on_submit(&query);
                        }
                    }
                    KeyCode::Backspace => {
                        if app.is_clarifying() || app.awaiting_confirmation() || !app.is_processing
                        {
                            app.input.pop();
                        }
                    }
                    KeyCode::Char(c) => {
                        if app.is_clarifying() || app.awaiting_confirmation() || !app.is_processing
                        {
                            app.input.push(c);
                        }
                    }
                    KeyCode::Up => {
                        let total_lines = calculate_total_lines(app, app.terminal_width);
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

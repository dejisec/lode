use std::path::Path;

use serde::Serialize;

use crate::cli::RequestConfig;

#[derive(Clone, Copy)]
pub enum OutputMode {
    Human,
    Quiet,
    Json,
}

pub struct Output {
    mode: OutputMode,
}

impl Output {
    pub fn new(json: bool, quiet: bool) -> Self {
        let mode = if json {
            OutputMode::Json
        } else if quiet {
            OutputMode::Quiet
        } else {
            OutputMode::Human
        };
        Self { mode }
    }

    pub fn start(&self, run_id: &str, artifacts_dir: &Path, config: &RequestConfig) {
        match self.mode {
            OutputMode::Human => {
                eprintln!("Starting research run: {}", run_id);
                eprintln!(
                    "Model: {}, Searches: {} (max: {}), Iterations: {}",
                    config.model, config.search_count, config.max_searches, config.max_iterations
                );
                eprintln!("Artifacts: {}", artifacts_dir.display());
            }
            OutputMode::Quiet => {}
            OutputMode::Json => {
                #[derive(Serialize)]
                struct Start<'a> {
                    r#type: &'static str,
                    version: &'static str,
                    run_id: &'a str,
                    artifacts_dir: &'a str,
                    model: &'a str,
                    search_count: u32,
                    max_iterations: u32,
                    max_searches: u32,
                    auto_decide: bool,
                }
                let msg = Start {
                    r#type: "start",
                    version: "v1",
                    run_id,
                    artifacts_dir: &artifacts_dir.display().to_string(),
                    model: &config.model,
                    search_count: config.search_count,
                    max_iterations: config.max_iterations,
                    max_searches: config.max_searches,
                    auto_decide: config.auto_decide,
                };
                println!("{}", serde_json::to_string(&msg).unwrap());
            }
        }
    }

    pub fn status(&self, message: &str) {
        match self.mode {
            OutputMode::Human => eprintln!("→ {}", message),
            OutputMode::Quiet => {}
            OutputMode::Json => {
                #[derive(Serialize)]
                struct Status<'a> {
                    r#type: &'static str,
                    message: &'a str,
                }
                let msg = Status {
                    r#type: "status",
                    message,
                };
                println!("{}", serde_json::to_string(&msg).unwrap());
            }
        }
    }

    pub fn trace(&self, trace_id: &str, trace_url: &str) {
        match self.mode {
            OutputMode::Human => {
                eprintln!(
                    "📊 Trace [{}]: {}",
                    &trace_id[..8.min(trace_id.len())],
                    trace_url
                );
            }
            OutputMode::Quiet => {}
            OutputMode::Json => {
                #[derive(Serialize)]
                struct Trace<'a> {
                    r#type: &'static str,
                    trace_id: &'a str,
                    trace_url: &'a str,
                }
                let msg = Trace {
                    r#type: "trace",
                    trace_id,
                    trace_url,
                };
                println!("{}", serde_json::to_string(&msg).unwrap());
            }
        }
    }

    pub fn prompt(&self, agent: &str, sequence: u32) {
        match self.mode {
            OutputMode::Human => eprintln!("📝 Prompt: {} ({})", agent, sequence),
            OutputMode::Quiet => {}
            OutputMode::Json => {
                #[derive(Serialize)]
                struct Prompt<'a> {
                    r#type: &'static str,
                    agent: &'a str,
                    sequence: u32,
                }
                let msg = Prompt {
                    r#type: "prompt",
                    agent,
                    sequence,
                };
                println!("{}", serde_json::to_string(&msg).unwrap());
            }
        }
    }

    pub fn response(&self, agent: &str, sequence: u32) {
        match self.mode {
            OutputMode::Human => eprintln!("📥 Response: {} ({})", agent, sequence),
            OutputMode::Quiet => {}
            OutputMode::Json => {
                #[derive(Serialize)]
                struct Response<'a> {
                    r#type: &'static str,
                    agent: &'a str,
                    sequence: u32,
                }
                let msg = Response {
                    r#type: "response",
                    agent,
                    sequence,
                };
                println!("{}", serde_json::to_string(&msg).unwrap());
            }
        }
    }

    pub fn decision(
        &self,
        action: &str,
        reason: &str,
        remaining_searches: u32,
        remaining_iterations: u32,
    ) {
        match self.mode {
            OutputMode::Human => {
                eprintln!(
                    "🤔 Decision: {} (searches: {}, iterations: {})",
                    action, remaining_searches, remaining_iterations
                );
                eprintln!("   Reason: {}", reason);
            }
            OutputMode::Quiet => {}
            OutputMode::Json => {
                #[derive(Serialize)]
                struct Decision<'a> {
                    r#type: &'static str,
                    action: &'a str,
                    reason: &'a str,
                    remaining_searches: u32,
                    remaining_iterations: u32,
                }
                let msg = Decision {
                    r#type: "decision",
                    action,
                    reason,
                    remaining_searches,
                    remaining_iterations,
                };
                println!("{}", serde_json::to_string(&msg).unwrap());
            }
        }
    }

    pub fn report(
        &self,
        short_summary: &str,
        markdown_report: &str,
        follow_up_questions: &[String],
    ) {
        match self.mode {
            OutputMode::Human | OutputMode::Quiet => {
                println!("\n{}\n", "=".repeat(60));
                println!("SUMMARY: {}\n", short_summary);
                println!("{}", markdown_report);
                if !follow_up_questions.is_empty() {
                    println!("\nFollow-up questions:");
                    for q in follow_up_questions {
                        println!("  - {}", q);
                    }
                }
            }
            OutputMode::Json => {
                #[derive(Serialize)]
                struct Report<'a> {
                    r#type: &'static str,
                    short_summary: &'a str,
                    markdown_report: &'a str,
                    follow_up_questions: &'a [String],
                }
                let msg = Report {
                    r#type: "report",
                    short_summary,
                    markdown_report,
                    follow_up_questions,
                };
                println!("{}", serde_json::to_string(&msg).unwrap());
            }
        }
    }

    pub fn error(&self, code: Option<&str>, message: &str) {
        match self.mode {
            OutputMode::Human | OutputMode::Quiet => {
                if let Some(c) = code {
                    eprintln!("Error [{}]: {}", c, message);
                } else {
                    eprintln!("Error: {}", message);
                }
            }
            OutputMode::Json => {
                #[derive(Serialize)]
                struct Error<'a> {
                    r#type: &'static str,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    code: Option<&'a str>,
                    message: &'a str,
                }
                let msg = Error {
                    r#type: "error",
                    code,
                    message,
                };
                println!("{}", serde_json::to_string(&msg).unwrap());
            }
        }
    }

    pub fn warning(&self, message: &str) {
        match self.mode {
            OutputMode::Human => eprintln!("Warning: {}", message),
            OutputMode::Quiet => {}
            OutputMode::Json => {
                #[derive(Serialize)]
                struct Warning<'a> {
                    r#type: &'static str,
                    message: &'a str,
                }
                let msg = Warning {
                    r#type: "warning",
                    message,
                };
                println!("{}", serde_json::to_string(&msg).unwrap());
            }
        }
    }

    pub fn complete(&self, success: bool, run_id: &str, artifacts_dir: &Path) {
        match self.mode {
            OutputMode::Human => {
                eprintln!(
                    "Run complete. Artifacts saved to: {}",
                    artifacts_dir.display()
                );
            }
            OutputMode::Quiet => {}
            OutputMode::Json => {
                #[derive(Serialize)]
                struct Complete<'a> {
                    r#type: &'static str,
                    success: bool,
                    run_id: &'a str,
                    artifacts_dir: &'a str,
                }
                let msg = Complete {
                    r#type: "complete",
                    success,
                    run_id,
                    artifacts_dir: &artifacts_dir.display().to_string(),
                };
                println!("{}", serde_json::to_string(&msg).unwrap());
            }
        }
    }
}


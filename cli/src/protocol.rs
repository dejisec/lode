use serde::{Deserialize, Serialize};

use crate::cli::RequestConfig;

#[derive(Serialize)]
pub struct Request {
    pub version: &'static str,
    pub run_id: String,
    pub query: String,
    pub config: RequestConfig,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct ClarifyingQuestion {
    pub label: String,
    pub question: String,
}

#[derive(Serialize)]
pub struct ClarifyingAnswers {
    pub answers: Vec<String>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InterruptCommand {
    Stop,
    #[allow(dead_code)]
    Pause,
    #[allow(dead_code)]
    ForceWrite,
}

#[derive(Serialize)]
pub struct Interrupt {
    #[serde(rename = "type")]
    pub msg_type: &'static str,
    pub command: InterruptCommand,
}

impl Interrupt {
    pub fn new(command: InterruptCommand) -> Self {
        Self {
            msg_type: "interrupt",
            command,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

#[derive(Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Status {
        message: String,
    },
    Trace {
        trace_id: String,
        trace_url: String,
    },
    ClarifyingQuestions {
        questions: Vec<ClarifyingQuestion>,
    },
    Prompt {
        agent: String,
        sequence: u32,
        #[allow(dead_code)]
        content: String,
    },
    #[serde(rename = "raw_response")]
    AgentOutput {
        agent: String,
        sequence: u32,
        content: String,
        token_usage: Option<TokenUsage>,
    },
    Decision {
        action: String,
        reason: String,
        remaining_searches: u32,
        remaining_iterations: u32,
    },
    Report {
        short_summary: String,
        markdown_report: String,
        follow_up_questions: Vec<String>,
    },
    Metadata {
        model: String,
        total_tokens: Option<u32>,
        #[allow(dead_code)]
        duration_ms: u64,
    },
    Error {
        message: String,
        code: Option<String>,
    },
    Done {
        success: bool,
    },
}

#[derive(Serialize)]
pub struct RunMetadata {
    pub run_id: String,
    pub model: Option<String>,
    pub total_tokens: Option<u32>,
    pub duration_ms: u64,
    pub trace_id: Option<String>,
    pub trace_url: Option<String>,
}

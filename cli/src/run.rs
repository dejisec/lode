use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Serialize;
use tokio::fs;

use crate::protocol::{Request, RunMetadata, TokenUsage};

pub struct RunContext {
    pub run_dir: PathBuf,
    start_time: Instant,
    pub trace_id: Option<String>,
    pub trace_url: Option<String>,
    pub model: Option<String>,
    pub total_tokens: Option<u32>,
    pub markdown_report: Option<String>,
}

impl RunContext {
    pub fn new(run_dir: PathBuf) -> Self {
        Self {
            run_dir,
            start_time: Instant::now(),
            trace_id: None,
            trace_url: None,
            model: None,
            total_tokens: None,
            markdown_report: None,
        }
    }

    pub fn prompts_dir(&self) -> PathBuf {
        self.run_dir.join("prompts")
    }

    pub fn responses_dir(&self) -> PathBuf {
        self.run_dir.join("raw_responses")
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    pub fn to_metadata(&self, run_id: String) -> RunMetadata {
        RunMetadata {
            run_id,
            model: self.model.clone(),
            total_tokens: self.total_tokens,
            duration_ms: self.elapsed_ms(),
            trace_id: self.trace_id.clone(),
            trace_url: self.trace_url.clone(),
        }
    }
}

pub async fn setup_run_directory(run_id: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let run_dir = PathBuf::from("runs").join(run_id);
    fs::create_dir_all(&run_dir).await?;
    fs::create_dir_all(run_dir.join("prompts")).await?;
    fs::create_dir_all(run_dir.join("raw_responses")).await?;
    Ok(run_dir)
}

pub async fn write_request(
    run_dir: &Path,
    request: &Request,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = run_dir.join("request.json");
    let content = serde_json::to_string_pretty(request)?;
    fs::write(path, content).await?;
    Ok(())
}

pub async fn write_prompt(
    ctx: &RunContext,
    agent: &str,
    sequence: u32,
    content: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let filename = format!("{:03}-{}.txt", sequence, agent.to_lowercase());
    let path = ctx.prompts_dir().join(filename);
    fs::write(path, content).await?;
    Ok(())
}

pub async fn write_raw_response(
    ctx: &RunContext,
    agent: &str,
    sequence: u32,
    content: &str,
    token_usage: Option<&TokenUsage>,
) -> Result<(), Box<dyn std::error::Error>> {
    let filename = format!("{:03}-{}.json", sequence, agent.to_lowercase());
    let path = ctx.responses_dir().join(filename);

    #[derive(Serialize)]
    struct RawResponseFile<'a> {
        agent: &'a str,
        sequence: u32,
        content: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        token_usage: Option<&'a TokenUsage>,
    }

    let data = RawResponseFile {
        agent,
        sequence,
        content,
        token_usage,
    };
    let json = serde_json::to_string_pretty(&data)?;
    fs::write(path, json).await?;
    Ok(())
}

pub async fn write_output(
    run_dir: &Path,
    markdown: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = run_dir.join("output.md");
    fs::write(path, markdown).await?;
    Ok(())
}

pub async fn write_metadata(
    run_dir: &Path,
    metadata: &RunMetadata,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = run_dir.join("metadata.json");
    let content = serde_json::to_string_pretty(metadata)?;
    fs::write(path, content).await?;
    Ok(())
}


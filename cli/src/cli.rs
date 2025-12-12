use std::env;

use clap::Parser;
use serde::Serialize;

pub const DEFAULT_MODEL: &str = "gpt-4o";
pub const DEFAULT_SEARCH_COUNT: u32 = 5;

#[derive(Parser)]
#[command(name = "lode")]
#[command(about = "Multi-agent research system", long_about = None)]
pub struct Cli {
    /// The research query to investigate
    pub query: Vec<String>,

    /// OpenAI model to use (overrides LODE_MODEL env var)
    #[arg(long)]
    pub model: Option<String>,

    /// Number of web searches to perform (overrides LODE_SEARCH_COUNT env var)
    #[arg(long)]
    pub search_count: Option<u32>,

    /// Output in JSON format (one JSON object per line)
    #[arg(long)]
    pub json: bool,

    /// Suppress progress output, only emit final result and errors
    #[arg(long, short)]
    pub quiet: bool,
}

#[derive(Serialize, Clone)]
pub struct RequestConfig {
    pub model: String,
    pub search_count: u32,
}

pub fn load_config(cli: &Cli) -> RequestConfig {
    let model = cli
        .model
        .clone()
        .or_else(|| env::var("LODE_MODEL").ok())
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());

    let search_count = cli
        .search_count
        .or_else(|| {
            env::var("LODE_SEARCH_COUNT")
                .ok()
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(DEFAULT_SEARCH_COUNT);

    RequestConfig {
        model,
        search_count,
    }
}

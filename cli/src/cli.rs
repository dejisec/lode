use std::env;

use clap::Parser;
use serde::Serialize;

pub const DEFAULT_MODEL: &str = "gpt-4o";
pub const DEFAULT_SEARCH_COUNT: u32 = 5;
pub const DEFAULT_MAX_ITERATIONS: u32 = 10;
pub const DEFAULT_MAX_SEARCHES: u32 = 15;

#[derive(Parser)]
#[command(name = "lode")]
#[command(about = "Multi-agent research system", long_about = None)]
pub struct Cli {
    /// The research query to investigate
    pub query: Vec<String>,

    /// OpenAI model to use (overrides LODE_MODEL env var)
    #[arg(long)]
    pub model: Option<String>,

    /// Initial number of web searches to plan (overrides LODE_SEARCH_COUNT env var)
    #[arg(long)]
    pub search_count: Option<u32>,

    /// Maximum orchestrator reasoning loops
    #[arg(long)]
    pub max_iterations: Option<u32>,

    /// Maximum total searches allowed
    #[arg(long)]
    pub max_searches: Option<u32>,

    /// Disable automatic decision-making (require confirmation for major actions)
    #[arg(long)]
    pub no_auto: bool,

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
    pub max_iterations: u32,
    pub max_searches: u32,
    pub auto_decide: bool,
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

    let max_iterations = cli
        .max_iterations
        .or_else(|| {
            env::var("LODE_MAX_ITERATIONS")
                .ok()
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(DEFAULT_MAX_ITERATIONS);

    let max_searches = cli
        .max_searches
        .or_else(|| {
            env::var("LODE_MAX_SEARCHES")
                .ok()
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(DEFAULT_MAX_SEARCHES);

    let auto_decide = !cli.no_auto;

    RequestConfig {
        model,
        search_count,
        max_iterations,
        max_searches,
        auto_decide,
    }
}

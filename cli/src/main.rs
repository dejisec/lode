mod cli;
mod output;
mod protocol;
mod run;
mod tui;

use std::process::Stdio;

use clap::Parser;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use uuid::Uuid;

use cli::{Cli, RequestConfig, load_config};
use output::Output;
use protocol::{Request, Response};
use run::{
    RunContext, setup_run_directory, write_metadata, write_output, write_prompt,
    write_raw_response, write_request,
};
use tui::{App, AppEvent, Tui};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();

    if cli.json || cli.quiet || !cli.query.is_empty() {
        run_single_query(&cli).await
    } else {
        run_tui(&cli).await
    }
}

async fn run_tui(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    let config = load_config(cli);
    let mut app = App::new();
    let mut tui_instance = Tui::new()?;

    let event_tx = app.event_sender();

    tui_instance
        .run(&mut app, |query| {
            let query = query.to_string();
            let config = config.clone();
            let tx = event_tx.clone();

            tokio::spawn(async move {
                if let Err(e) = run_research_query(&query, &config, tx.clone()).await {
                    let _ = tx.send(AppEvent::Error(e));
                }
            });
        })
        .await?;

    Ok(())
}

async fn run_research_query(
    query: &str,
    config: &RequestConfig,
    event_tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<(), String> {
    let run_id = Uuid::new_v4().to_string();
    let request = Request {
        version: "v1",
        run_id: run_id.clone(),
        query: query.to_string(),
        config: config.clone(),
    };

    let run_dir = setup_run_directory(&run_id)
        .await
        .map_err(|e| e.to_string())?;
    write_request(&run_dir, &request)
        .await
        .map_err(|e| e.to_string())?;

    let mut ctx = RunContext::new(run_dir.clone());

    let mut child = Command::new("uv")
        .args(["run", "python", "-m", "lode.runner"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    let request_json = serde_json::to_string(&request).map_err(|e| e.to_string())?;
    stdin
        .write_all(request_json.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    stdin.write_all(b"\n").await.map_err(|e| e.to_string())?;
    drop(stdin);

    let stdout = child.stdout.take().expect("Failed to open stdout");
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();

    let mut success = false;

    while let Some(line) = lines.next_line().await.map_err(|e| e.to_string())? {
        if let Ok(response) = serde_json::from_str::<Response>(&line) {
            match &response {
                Response::Trace {
                    trace_id,
                    trace_url,
                } => {
                    ctx.trace_id = Some(trace_id.clone());
                    ctx.trace_url = Some(trace_url.clone());
                }
                Response::Prompt {
                    agent,
                    sequence,
                    content,
                } => {
                    let _ = write_prompt(&ctx, agent, *sequence, content).await;
                }
                Response::AgentOutput {
                    agent,
                    sequence,
                    content,
                    token_usage,
                } => {
                    let _ = write_raw_response(
                        &ctx,
                        agent,
                        *sequence,
                        content,
                        token_usage.as_ref(),
                    )
                    .await;
                }
                Response::Report {
                    markdown_report, ..
                } => {
                    ctx.markdown_report = Some(markdown_report.clone());
                }
                Response::Metadata {
                    model,
                    total_tokens,
                    ..
                } => {
                    ctx.model = Some(model.clone());
                    ctx.total_tokens = *total_tokens;
                }
                Response::Done { success: s } => {
                    success = *s;
                }
                _ => {}
            }

            let _ = event_tx.send(AppEvent::BackendResponse(response));
        }
    }

    let status = child.wait().await.map_err(|e| e.to_string())?;

    if let Some(ref markdown) = ctx.markdown_report {
        let _ = write_output(&run_dir, markdown).await;
    }

    let metadata = ctx.to_metadata(run_id.clone());
    let _ = write_metadata(&run_dir, &metadata).await;

    let _ = event_tx.send(AppEvent::RunComplete {
        success: success && status.success(),
        run_id,
    });

    Ok(())
}

async fn run_single_query(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    let output = Output::new(cli.json, cli.quiet);
    let query = cli.query.join(" ");

    if query.is_empty() {
        output.error(Some("MISSING_QUERY"), "query is required");
        eprintln!("Usage: lode <query>");
        std::process::exit(1);
    }

    let config = load_config(cli);
    let run_id = Uuid::new_v4().to_string();
    let request = Request {
        version: "v1",
        run_id: run_id.clone(),
        query: query.clone(),
        config: config.clone(),
    };

    let run_dir = setup_run_directory(&run_id).await?;
    write_request(&run_dir, &request).await?;

    output.start(&run_id, &run_dir, &config);

    let mut ctx = RunContext::new(run_dir.clone());

    let mut child = Command::new("uv")
        .args(["run", "python", "-m", "lode.runner"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    let request_json = serde_json::to_string(&request)?;
    stdin.write_all(request_json.as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    drop(stdin);

    let stdout = child.stdout.take().expect("Failed to open stdout");
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();

    let mut success = false;

    while let Some(line) = lines.next_line().await? {
        match serde_json::from_str::<Response>(&line) {
            Ok(response) => match response {
                Response::Status { message } => {
                    output.status(&message);
                }
                Response::Trace {
                    trace_id,
                    trace_url,
                } => {
                    output.trace(&trace_id, &trace_url);
                    ctx.trace_id = Some(trace_id);
                    ctx.trace_url = Some(trace_url);
                }
                Response::Prompt {
                    agent,
                    sequence,
                    content,
                } => {
                    output.prompt(&agent, sequence);
                    if let Err(e) = write_prompt(&ctx, &agent, sequence, &content).await {
                        output.warning(&format!("failed to write prompt: {}", e));
                    }
                }
                Response::AgentOutput {
                    agent,
                    sequence,
                    content,
                    token_usage,
                } => {
                    output.response(&agent, sequence);
                    if let Err(e) =
                        write_raw_response(&ctx, &agent, sequence, &content, token_usage.as_ref())
                            .await
                    {
                        output.warning(&format!("failed to write response: {}", e));
                    }
                }
                Response::Report {
                    short_summary,
                    markdown_report,
                    follow_up_questions,
                } => {
                    ctx.markdown_report = Some(markdown_report.clone());
                    output.report(&short_summary, &markdown_report, &follow_up_questions);
                }
                Response::Metadata {
                    model,
                    total_tokens,
                    ..
                } => {
                    ctx.model = Some(model);
                    ctx.total_tokens = total_tokens;
                }
                Response::Error { message, code } => {
                    output.error(code.as_deref(), &message);
                }
                Response::Done { success: s } => {
                    success = s;
                }
            },
            Err(e) => {
                output.warning(&format!("failed to parse response: {} (line: {})", e, line));
            }
        }
    }

    let status = child.wait().await?;

    if let Some(ref markdown) = ctx.markdown_report
        && let Err(e) = write_output(&run_dir, markdown).await
    {
        output.warning(&format!("failed to write output.md: {}", e));
    }

    let metadata = ctx.to_metadata(run_id.clone());
    if let Err(e) = write_metadata(&run_dir, &metadata).await {
        output.warning(&format!("failed to write metadata.json: {}", e));
    }

    output.complete(success && status.success(), &run_id, &run_dir);

    if !status.success() || !success {
        std::process::exit(1);
    }

    Ok(())
}

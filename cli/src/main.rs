mod cli;
mod output;
mod protocol;
mod run;
mod tui;

use std::process::Stdio;
use std::sync::Arc;

use clap::Parser;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{Mutex, mpsc, oneshot};
use uuid::Uuid;

use cli::{Cli, RequestConfig, load_config};
use output::Output;
use protocol::{ClarifyingAnswers, InterruptCommand, Request, Response};
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
    let answer_slot: Arc<Mutex<Option<oneshot::Sender<Vec<String>>>>> = Arc::new(Mutex::new(None));
    let interrupt_tx: Arc<Mutex<Option<mpsc::UnboundedSender<InterruptCommand>>>> =
        Arc::new(Mutex::new(None));

    let answer_slot_submit = answer_slot.clone();
    let answer_slot_answers = answer_slot.clone();
    let interrupt_tx_submit = interrupt_tx.clone();
    let interrupt_tx_tui = interrupt_tx.clone();

    tui_instance
        .run(
            &mut app,
            move |query| {
                let query = query.to_string();
                let config = config.clone();
                let tx = event_tx.clone();
                let answer_slot = answer_slot_submit.clone();
                let interrupt_slot = interrupt_tx_submit.clone();

                tokio::spawn(async move {
                    let (int_tx, int_rx) = mpsc::unbounded_channel();
                    {
                        let mut guard = interrupt_slot.lock().await;
                        *guard = Some(int_tx);
                    }

                    if let Err(e) =
                        run_research_query(&query, &config, tx.clone(), answer_slot, Some(int_rx))
                            .await
                    {
                        let _ = tx.send(AppEvent::Error(e));
                    }

                    // Clear interrupt sender after query completes
                    {
                        let mut guard = interrupt_slot.lock().await;
                        *guard = None;
                    }
                });
            },
            move |answers| {
                let slot = answer_slot_answers.clone();
                tokio::spawn(async move {
                    let mut guard = slot.lock().await;
                    if let Some(tx) = guard.take() {
                        let _ = tx.send(answers);
                    }
                });
            },
            move || {
                let slot = interrupt_tx_tui.clone();
                tokio::spawn(async move {
                    let guard = slot.lock().await;
                    if let Some(ref tx) = *guard {
                        let _ = tx.send(InterruptCommand::Stop);
                    }
                });
            },
        )
        .await?;

    Ok(())
}

async fn send_interrupt(
    stdin: &Arc<Mutex<tokio::process::ChildStdin>>,
    command: InterruptCommand,
) -> Result<(), String> {
    use protocol::Interrupt;
    let interrupt = Interrupt::new(command);
    let json = interrupt.to_json();
    let mut guard = stdin.lock().await;
    guard
        .write_all(json.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    guard.write_all(b"\n").await.map_err(|e| e.to_string())?;
    Ok(())
}

async fn run_research_query(
    query: &str,
    config: &RequestConfig,
    event_tx: mpsc::UnboundedSender<AppEvent>,
    answer_slot: Arc<Mutex<Option<oneshot::Sender<Vec<String>>>>>,
    interrupt_rx: Option<mpsc::UnboundedReceiver<InterruptCommand>>,
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

    let stdin = Arc::new(Mutex::new(child.stdin.take().expect("Failed to open stdin")));
    let request_json = serde_json::to_string(&request).map_err(|e| e.to_string())?;
    {
        let mut guard = stdin.lock().await;
        guard
            .write_all(request_json.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        guard.write_all(b"\n").await.map_err(|e| e.to_string())?;
    }

    // Spawn interrupt handler if receiver provided
    let stdin_for_interrupt = stdin.clone();
    let interrupt_handle = interrupt_rx.map(|mut rx| {
        tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                let _ = send_interrupt(&stdin_for_interrupt, cmd).await;
            }
        })
    });

    let stdout = child.stdout.take().expect("Failed to open stdout");
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();

    let mut success = false;
    let mut answers_sent = false;

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
                Response::ClarifyingQuestions { .. } if !answers_sent => {
                    let _ = event_tx.send(AppEvent::BackendResponse(response.clone()));

                    let (tx, rx) = oneshot::channel();
                    {
                        let mut slot = answer_slot.lock().await;
                        *slot = Some(tx);
                    }

                    match rx.await {
                        Ok(answers) => {
                            let answers_msg = ClarifyingAnswers { answers };
                            let answers_json =
                                serde_json::to_string(&answers_msg).map_err(|e| e.to_string())?;
                            let mut guard = stdin.lock().await;
                            guard
                                .write_all(answers_json.as_bytes())
                                .await
                                .map_err(|e| e.to_string())?;
                            guard.write_all(b"\n").await.map_err(|e| e.to_string())?;
                            answers_sent = true;
                        }
                        Err(_) => {
                            return Err("Failed to receive clarifying answers".to_string());
                        }
                    }
                    continue;
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
                Response::Decision { .. } => {
                    // Decision events are displayed via TUI status updates
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

    // Clean up interrupt handler
    if let Some(handle) = interrupt_handle {
        handle.abort();
    }

    drop(stdin);
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
    use std::io::{BufRead, Write};

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

    let stdout = child.stdout.take().expect("Failed to open stdout");
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();

    let mut success = false;
    let mut answers_sent = false;

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
                Response::ClarifyingQuestions { questions } if !answers_sent => {
                    eprintln!("\nPlease answer these clarifying questions:");
                    let mut answers = Vec::new();
                    let term_stdin = std::io::stdin();

                    for (i, q) in questions.iter().enumerate() {
                        eprintln!("\n{}. [{}] {}", i + 1, q.label, q.question);
                        eprint!("> ");
                        std::io::stderr().flush().ok();

                        let mut answer = String::new();
                        term_stdin.lock().read_line(&mut answer).ok();
                        answers.push(answer.trim().to_string());
                    }

                    eprintln!();
                    let answers_msg = ClarifyingAnswers { answers };
                    let answers_json = serde_json::to_string(&answers_msg)?;
                    stdin.write_all(answers_json.as_bytes()).await?;
                    stdin.write_all(b"\n").await?;
                    answers_sent = true;
                }
                Response::ClarifyingQuestions { .. } => {
                    // Already sent answers, ignore
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
                Response::Decision {
                    action,
                    reason,
                    remaining_searches,
                    remaining_iterations,
                } => {
                    output.decision(&action, &reason, remaining_searches, remaining_iterations);
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

    drop(stdin);

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

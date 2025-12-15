"""JSON stdin/stdout runner for Rust CLI integration."""

import asyncio
import json
import sys
import time
from dataclasses import dataclass
from typing import Any

from .config import Config
from .manager import (
    ResearchManager,
    PromptEvent,
    ResponseEvent,
    ClarifyingQuestionsEvent,
    DecisionEvent,
)


@dataclass
class Request:
    version: str
    run_id: str
    query: str
    config: Config


def emit(msg: dict[str, Any]) -> None:
    """Emit a JSON message to stdout."""
    print(json.dumps(msg), flush=True)


def emit_status(message: str) -> None:
    emit({"type": "status", "message": message})


def emit_trace(trace_id: str, trace_url: str) -> None:
    emit({"type": "trace", "trace_id": trace_id, "trace_url": trace_url})


def emit_prompt(agent: str, sequence: int, content: str) -> None:
    emit({
        "type": "prompt",
        "agent": agent,
        "sequence": sequence,
        "content": content,
    })


def emit_raw_response(
    agent: str,
    sequence: int,
    content: str,
    token_usage: dict[str, int] | None = None,
) -> None:
    msg: dict[str, Any] = {
        "type": "raw_response",
        "agent": agent,
        "sequence": sequence,
        "content": content,
    }
    if token_usage:
        msg["token_usage"] = token_usage
    emit(msg)


def emit_clarifying_questions(questions: list[dict[str, str]]) -> None:
    emit({
        "type": "clarifying_questions",
        "questions": questions,
    })


def emit_decision(
    action: str,
    reason: str,
    remaining_searches: int,
    remaining_iterations: int,
) -> None:
    emit({
        "type": "decision",
        "action": action,
        "reason": reason,
        "remaining_searches": remaining_searches,
        "remaining_iterations": remaining_iterations,
    })


def emit_report(short_summary: str, markdown_report: str, follow_up_questions: list[str]) -> None:
    emit({
        "type": "report",
        "short_summary": short_summary,
        "markdown_report": markdown_report,
        "follow_up_questions": follow_up_questions,
    })


def emit_metadata(model: str, duration_ms: int, total_tokens: int | None = None) -> None:
    msg: dict[str, Any] = {
        "type": "metadata",
        "model": model,
        "duration_ms": duration_ms,
    }
    if total_tokens is not None:
        msg["total_tokens"] = total_tokens
    emit(msg)


def emit_error(message: str, code: str | None = None) -> None:
    msg: dict[str, Any] = {"type": "error", "message": message}
    if code:
        msg["code"] = code
    emit(msg)


def emit_done(success: bool) -> None:
    emit({"type": "done", "success": success})


def parse_request(line: str) -> Request:
    data = json.loads(line)
    if data.get("version") != "v1":
        raise ValueError(f"Unsupported version: {data.get('version')}")

    config_data = data.get("config", {})
    config = Config(
        model=config_data.get("model", "gpt-4o"),
        search_count=config_data.get("search_count", 5),
        max_iterations=config_data.get("max_iterations", 10),
        max_searches=config_data.get("max_searches", 15),
        auto_decide=config_data.get("auto_decide", True),
    )

    return Request(
        version=data["version"],
        run_id=data["run_id"],
        query=data["query"],
        config=config,
    )


async def read_stdin_line_with_timeout() -> str | None:
    """Read a line from stdin with timeout using select."""
    import select
    loop = asyncio.get_event_loop()
    
    def _read_if_available() -> str | None:
        # Check if stdin has data available (non-blocking)
        if select.select([sys.stdin], [], [], 0)[0]:
            line = sys.stdin.readline()
            return line.strip() if line else None
        return None
    
    try:
        return await loop.run_in_executor(None, _read_if_available)
    except Exception:
        return None


async def interrupt_listener(manager: ResearchManager, stop_event: asyncio.Event) -> None:
    """Listen for interrupt commands from stdin."""
    while not stop_event.is_set():
        line = await read_stdin_line_with_timeout()
        if line:
            try:
                data = json.loads(line)
                if data.get("type") == "interrupt":
                    command = data.get("command", "stop")
                    await manager.interrupt_controller.signal_interrupt(command)
            except json.JSONDecodeError:
                pass
        await asyncio.sleep(0.05)


async def run_clarify(request: Request, manager: ResearchManager) -> None:
    """Run the clarification phase and emit questions."""
    async for item in manager.clarify(request.query):
        if isinstance(item, PromptEvent):
            emit_prompt(item.agent, item.sequence, item.content)
        elif isinstance(item, ResponseEvent):
            emit_raw_response(
                item.agent,
                item.sequence,
                item.content,
                item.token_usage,
            )
        elif isinstance(item, ClarifyingQuestionsEvent):
            emit_clarifying_questions(item.questions)
        elif isinstance(item, str):
            emit_status(item)


async def run_research(
    request: Request,
    manager: ResearchManager,
    clarifying_answers: list[str] | None,
    start_time: float,
) -> None:
    """Execute the agentic research workflow and emit JSON responses."""
    from .agents import ReportData

    async for item in manager.run(request.query, clarifying_answers):
        if isinstance(item, PromptEvent):
            emit_prompt(item.agent, item.sequence, item.content)
        elif isinstance(item, ResponseEvent):
            emit_raw_response(
                item.agent,
                item.sequence,
                item.content,
                item.token_usage,
            )
        elif isinstance(item, DecisionEvent):
            emit_decision(
                item.action,
                item.reason,
                item.remaining_searches,
                item.remaining_iterations,
            )
        elif isinstance(item, ReportData):
            emit_report(
                item.short_summary,
                item.markdown_report,
                item.follow_up_questions,
            )
        elif isinstance(item, str):
            if item.startswith("View trace:"):
                trace_url = item.split(": ", 1)[1]
                trace_id = trace_url.split("trace_id=")[1] if "trace_id=" in trace_url else ""
                emit_trace(trace_id, trace_url)
            else:
                emit_status(item)

    duration_ms = int((time.time() - start_time) * 1000)
    emit_metadata(request.config.model, duration_ms)


def read_clarifying_answers() -> list[str] | None:
    """Read clarifying answers from stdin (second JSON line)."""
    line = sys.stdin.readline().strip()
    if not line:
        return None
    try:
        data = json.loads(line)
        return data.get("answers", [])
    except json.JSONDecodeError:
        return None


async def async_main() -> None:
    line = sys.stdin.readline().strip()
    if not line:
        emit_error("No input received", "NO_INPUT")
        emit_done(False)
        return

    try:
        request = parse_request(line)
    except (json.JSONDecodeError, KeyError, ValueError) as e:
        emit_error(f"Invalid request: {e}", "INVALID_REQUEST")
        emit_done(False)
        return

    try:
        start_time = time.time()
        manager = ResearchManager(request.config)

        # Phase 1: Generate and emit clarifying questions
        await run_clarify(request, manager)

        # Phase 2: Wait for answers from stdin
        clarifying_answers = read_clarifying_answers()

        # Phase 3: Run agentic research with interrupt listener
        stop_event = asyncio.Event()
        interrupt_task = asyncio.create_task(interrupt_listener(manager, stop_event))
        try:
            await run_research(request, manager, clarifying_answers, start_time)
        finally:
            stop_event.set()
            interrupt_task.cancel()
            try:
                await interrupt_task
            except asyncio.CancelledError:
                pass

        emit_done(True)
    except Exception as e:
        emit_error(str(e), "RUNTIME_ERROR")
        emit_done(False)


def main() -> None:
    asyncio.run(async_main())


if __name__ == "__main__":
    main()

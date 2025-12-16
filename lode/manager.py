"""Research manager orchestrating the agentic workflow."""

import asyncio
import json
from dataclasses import dataclass, field
from typing import AsyncIterator, Any

from agents import trace, gen_trace_id

from .agents import (
    create_clarifier_agent,
    ClarifyingQuestions,
    create_orchestrator_agent,
    ReportData,
)
from .config import Config
from .openai_client import agent_runner


@dataclass
class PromptEvent:
    """Event emitted before an agent call with the prompt content."""
    agent: str
    sequence: int
    content: str


@dataclass
class ResponseEvent:
    """Event emitted after an agent call with the response content."""
    agent: str
    sequence: int
    content: str
    token_usage: dict[str, int] | None = None


@dataclass
class ClarifyingQuestionsEvent:
    """Event emitted when clarifying questions are ready for the user."""
    questions: list[dict[str, str]] = field(default_factory=list)


@dataclass
class DecisionEvent:
    """Event emitted when the orchestrator makes a decision."""
    action: str
    reason: str
    remaining_searches: int
    remaining_iterations: int


@dataclass
class MetadataEvent:
    """Event emitted with run metadata."""
    model: str
    total_tokens: int | None = None


class InterruptController:
    """Handles interrupt signals from Rust CLI."""

    def __init__(self):
        self._interrupted = False
        self._command: str | None = None
        self._lock = asyncio.Lock()

    async def check_interrupt(self) -> tuple[bool, str | None]:
        """Check if an interrupt has been requested."""
        async with self._lock:
            return self._interrupted, self._command

    async def signal_interrupt(self, command: str) -> None:
        """Signal an interrupt with a specific command."""
        async with self._lock:
            self._interrupted = True
            self._command = command

    async def clear(self) -> None:
        """Clear the interrupt state."""
        async with self._lock:
            self._interrupted = False
            self._command = None


class ResearchManager:
    """Orchestrates the deep research process using an agentic approach."""

    def __init__(self, config: Config):
        self._config = config
        self._sequence = 0
        self._interrupt_controller = InterruptController()
        self._clarifier_agent = create_clarifier_agent(config.model)
        self._orchestrator_agent = create_orchestrator_agent(
            model=config.model,
            search_count=config.search_count,
            max_iterations=config.max_iterations,
            max_searches=config.max_searches,
        )
        self._searches_used = 0
        self._iterations_used = 0

    def _next_sequence(self) -> int:
        self._sequence += 1
        return self._sequence

    @property
    def interrupt_controller(self) -> InterruptController:
        return self._interrupt_controller

    async def clarify(self, query: str) -> AsyncIterator[Any]:
        """Generate clarifying questions for the query."""
        yield "Generating clarifying questions..."
        clarifier_prompt = f"Query: {query}"
        seq = self._next_sequence()
        yield PromptEvent(agent="clarifier", sequence=seq, content=clarifier_prompt)

        result = await agent_runner.run(self._clarifier_agent, clarifier_prompt)
        questions = result.output.final_output_as(ClarifyingQuestions)

        token_usage = result.token_usage.to_dict() if result.token_usage else None
        yield ResponseEvent(
            agent="clarifier",
            sequence=seq,
            content=json.dumps([q.model_dump() for q in questions.questions], indent=2),
            token_usage=token_usage,
        )

        yield ClarifyingQuestionsEvent(
            questions=[q.model_dump() for q in questions.questions]
        )

    async def run(
        self,
        query: str,
        clarifying_answers: list[str] | None = None,
    ) -> AsyncIterator[Any]:
        """Run the agentic research process."""
        trace_id = gen_trace_id()
        with trace("Research trace", trace_id=trace_id):
            yield f"View trace: https://platform.openai.com/traces/trace?trace_id={trace_id}"

            context = ""
            if clarifying_answers:
                context = "\n\nUser provided additional context:\n"
                for answer in clarifying_answers:
                    if answer.strip():
                        context += f"- {answer}\n"

            orchestrator_input = self._build_orchestrator_input(query, context)

            yield "Starting agentic research loop..."
            yield DecisionEvent(
                action="start",
                reason="Beginning research with orchestrator agent",
                remaining_searches=self._config.max_searches,
                remaining_iterations=self._config.max_iterations,
            )

            seq = self._next_sequence()
            yield PromptEvent(agent="orchestrator", sequence=seq, content=orchestrator_input)

            async for event in self._run_orchestrator(orchestrator_input, seq):
                yield event

    def _build_orchestrator_input(self, query: str, context: str) -> str:
        """Build the input for the orchestrator agent."""
        return f"""Research Query: {query}{context}

Budget Constraints:
- Maximum searches: {self._config.max_searches}
- Maximum iterations: {self._config.max_iterations}
- Initial search plan size: {self._config.search_count}

Begin your research process. Use your tools strategically to gather comprehensive 
information, then hand off to the writer when ready."""

    async def _run_orchestrator(
        self,
        input_text: str,
        seq: int,
    ) -> AsyncIterator[Any]:
        """Run the orchestrator agent with event streaming."""
        orchestrator_task = asyncio.create_task(
            agent_runner.run(self._orchestrator_agent, input_text)
        )
        try:
            while True:
                done, _ = await asyncio.wait(
                    {orchestrator_task},
                    timeout=0.1,
                    return_when=asyncio.FIRST_COMPLETED,
                    )

                interrupted, command = await self._interrupt_controller.check_interrupt()
                if interrupted:
                    orchestrator_task.cancel()
                    try:
                        await orchestrator_task
                    except asyncio.CancelledError:
                        pass

                    reason = "Interrupted by user command"
                    if command:
                        reason = f"Interrupted by command: {command}"
                    yield DecisionEvent(
                        action="interrupt",
                        reason=reason,
                        remaining_searches=self._config.max_searches - self._searches_used,
                        remaining_iterations=self._config.max_iterations - self._iterations_used,
                    )
                    yield "Research interrupted"
                    return

                if orchestrator_task in done:
                    break
            result = await orchestrator_task

            token_usage = result.token_usage.to_dict() if result.token_usage else None

            final_output = result.output.final_output
            if hasattr(final_output, 'model_dump'):
                output_content = json.dumps(final_output.model_dump(), indent=2)
            else:
                output_content = str(final_output)

            self._update_budget_usage(final_output)

            yield ResponseEvent(
                agent="orchestrator",
                sequence=seq,
                content=output_content,
                token_usage=token_usage,
            )

            report_data = self._extract_report(final_output)
            if report_data:
                yield "Research complete"
                yield report_data
            else:
                yield DecisionEvent(
                    action="complete",
                    reason="Orchestrator finished without explicit handoff to writer",
                    remaining_searches=self._config.max_searches - self._searches_used,
                    remaining_iterations=self._config.max_iterations - self._iterations_used,
                )
                yield "Research complete (orchestrator loop ended)"

        except Exception as e:
            yield ResponseEvent(
                agent="orchestrator",
                sequence=seq,
                content=f"[error: {e}]",
            )
            raise

    def _update_budget_usage(self, output: Any) -> None:
        """Update budget counters from the orchestrator's output if available."""

        def _extract(obj: Any, key: str) -> int | None:
            if hasattr(obj, key):
                value = getattr(obj, key)
                if isinstance(value, int):
                    return value
                if hasattr(value, "model_dump"):
                    try:
                        data = value.model_dump()
                    except Exception:
                        data = None
                    if isinstance(data, dict) and isinstance(data.get(key), int):
                        return data[key]

            if hasattr(obj, "model_dump"):
                try:
                    data = obj.model_dump()
                except Exception:
                    data = None
                if isinstance(data, dict) and isinstance(data.get(key), int):
                    return data[key]

            if isinstance(obj, dict) and isinstance(obj.get(key), int):
                return obj[key]

            return None

        searches = _extract(output, "searches_performed")
        iterations = _extract(output, "iterations")

        if searches is not None:
            self._searches_used = searches
        if iterations is not None:
            self._iterations_used = iterations

    def _extract_report(self, output: Any) -> ReportData | None:
        """Try to extract ReportData from various output formats."""
        if isinstance(output, ReportData):
            return output
        
        if hasattr(output, 'final_output'):
            return self._extract_report(output.final_output)
        
        if hasattr(output, 'model_dump'):
            try:
                data = output.model_dump()
                if all(k in data for k in ['short_summary', 'markdown_report', 'follow_up_questions']):
                    return ReportData(**data)
            except Exception:
                pass
        
        if isinstance(output, dict):
            if all(k in output for k in ['short_summary', 'markdown_report', 'follow_up_questions']):
                try:
                    return ReportData(**output)
                except Exception:
                    pass
        
        return None

"""Research manager orchestrating the multi-agent workflow."""

import asyncio
import json
from dataclasses import dataclass

from agents import trace, gen_trace_id

from .agents import (
    create_planner_agent,
    WebSearchItem,
    WebSearchPlan,
    create_search_agent,
    create_writer_agent,
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
class MetadataEvent:
    """Event emitted with run metadata."""
    model: str
    total_tokens: int | None = None


class ResearchManager:
    """Orchestrates the deep research process across multiple agents."""

    def __init__(self, config: Config):
        self._config = config
        self._sequence = 0
        self._planner_agent = create_planner_agent(config.model, config.search_count)
        self._search_agent = create_search_agent(config.model)
        self._writer_agent = create_writer_agent(config.model)

    def _next_sequence(self) -> int:
        self._sequence += 1
        return self._sequence

    async def run(self, query: str):
        """Run the deep research process, yielding status updates, events, and the final report."""
        trace_id = gen_trace_id()
        with trace("Research trace", trace_id=trace_id):
            yield f"View trace: https://platform.openai.com/traces/trace?trace_id={trace_id}"

            # Plan searches
            yield "Planning searches..."
            planner_prompt = f"Query: {query}"
            seq = self._next_sequence()
            yield PromptEvent(agent="planner", sequence=seq, content=planner_prompt)

            result = await agent_runner.run(self._planner_agent, planner_prompt)
            search_plan = result.output.final_output_as(WebSearchPlan)

            yield ResponseEvent(
                agent="planner",
                sequence=seq,
                content=json.dumps([s.model_dump() for s in search_plan.searches], indent=2),
                token_usage=result.token_usage.to_dict() if result.token_usage else None,
            )
            yield f"Will perform {len(search_plan.searches)} searches"

            # Perform searches
            yield "Searching..."
            search_results = []
            search_tasks = []

            for item in search_plan.searches:
                seq = self._next_sequence()
                search_prompt = f"Search term: {item.query}\nReason for searching: {item.reason}"
                yield PromptEvent(agent="search", sequence=seq, content=search_prompt)
                search_tasks.append((seq, item, asyncio.create_task(self._search(item))))

            for seq, item, task in search_tasks:
                try:
                    search_result = await task
                    if search_result is not None:
                        result_text, token_usage = search_result
                        search_results.append(result_text)
                        yield ResponseEvent(
                            agent="search",
                            sequence=seq,
                            content=result_text,
                            token_usage=token_usage,
                        )
                    else:
                        yield ResponseEvent(agent="search", sequence=seq, content="[search failed]")
                except Exception as e:
                    yield ResponseEvent(agent="search", sequence=seq, content=f"[error: {e}]")

            yield f"Completed {len(search_results)} searches"

            # Write report
            yield "Writing report..."
            writer_prompt = f"Original query: {query}\nSummarized search results: {search_results}"
            seq = self._next_sequence()
            yield PromptEvent(agent="writer", sequence=seq, content=writer_prompt)

            result = await agent_runner.run(self._writer_agent, writer_prompt)
            report = result.output.final_output_as(ReportData)

            yield ResponseEvent(
                agent="writer",
                sequence=seq,
                content=json.dumps(report.model_dump(), indent=2),
                token_usage=result.token_usage.to_dict() if result.token_usage else None,
            )

            yield "Research complete"
            yield report

    async def _search(self, item: WebSearchItem) -> tuple[str, dict[str, int] | None] | None:
        """Perform a single search. Returns (result_text, token_usage) or None on failure."""
        input_text = f"Search term: {item.query}\nReason for searching: {item.reason}"
        try:
            result = await agent_runner.run(self._search_agent, input_text)
            token_usage = result.token_usage.to_dict() if result.token_usage else None
            return str(result.output.final_output), token_usage
        except Exception:
            return None

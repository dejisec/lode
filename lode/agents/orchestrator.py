"""Orchestrator agent that autonomously manages the research workflow."""

from pydantic import BaseModel, Field
from agents import Agent, handoff

from .clarifier import create_clarifier_agent
from .planner import create_planner_agent
from .search import create_search_agent
from .evaluator import create_evaluator_agent
from .writer import create_writer_agent


class OrchestratorState(BaseModel):
    """Tracks orchestrator progress and budget."""
    searches_performed: int = 0
    iterations: int = 0
    research_summaries: list[str] = Field(default_factory=list)


ORCHESTRATOR_INSTRUCTIONS = """You are a research orchestrator responsible for conducting thorough 
research on a user's query. You have access to specialized agents as tools and must decide how to 
use them effectively within your budget constraints.

## Your Tools

1. **clarify_query**: Generate clarifying questions to better understand user intent. 
   Use early if the query is ambiguous.

2. **plan_searches**: Given a query and context, generate a list of search queries.
   Returns a structured plan with reasons for each search.

3. **execute_search**: Execute a single web search and get summarized results.
   Call this for each search query you want to execute.

4. **evaluate_research**: Assess current research quality and identify gaps.
   Use this to decide if you need more searches or can proceed to writing.

## Your Process

1. If the query is ambiguous, use clarify_query first
2. Use plan_searches to create an initial research plan
3. Execute searches from the plan using execute_search
4. Use evaluate_research to assess coverage
5. If gaps exist and budget allows, plan and execute additional searches
6. When research is sufficient, hand off to the writer agent

## Budget Constraints

You have a limited number of searches and iterations. Check your remaining budget 
before each action. If budget is exhausted, proceed to writing with available research.

## Decision Making

After each evaluation:
- If coverage_score >= 7 and is_sufficient=true: Proceed to writing
- If coverage_score < 7 and budget remains: Execute gap-filling searches
- If budget exhausted: Proceed to writing with current research

Always explain your reasoning before taking actions."""


def create_orchestrator_agent(
    model: str,
    search_count: int,
    max_iterations: int,
    max_searches: int,
) -> Agent:
    """Create the orchestrator agent with all sub-agents as tools."""
    clarifier = create_clarifier_agent(model)
    planner = create_planner_agent(model, search_count)
    searcher = create_search_agent(model)
    evaluator = create_evaluator_agent(model)
    writer = create_writer_agent(model)

    clarifier_tool = clarifier.as_tool(
        tool_name="clarify_query",
        tool_description="Generate clarifying questions to better understand the user's query.",
    )

    planner_tool = planner.as_tool(
        tool_name="plan_searches",
        tool_description="Generate a research plan with search queries based on the query and context.",
    )

    search_tool = searcher.as_tool(
        tool_name="execute_search",
        tool_description="Execute a web search and return summarized results. Input should include the search term and reason.",
    )

    evaluator_tool = evaluator.as_tool(
        tool_name="evaluate_research",
        tool_description="Evaluate research quality and identify gaps. Input should include the original query and all research summaries collected so far.",
    )

    dynamic_instructions = f"""{ORCHESTRATOR_INSTRUCTIONS}

## Current Budget
- Initial search plan size: {search_count}
- Maximum total searches: {max_searches}
- Maximum iterations: {max_iterations}

Track your usage and remaining budget in your reasoning."""

    return Agent(
        name="OrchestratorAgent",
        instructions=dynamic_instructions,
        model=model,
        tools=[clarifier_tool, planner_tool, search_tool, evaluator_tool],
        handoffs=[handoff(writer)],
    )


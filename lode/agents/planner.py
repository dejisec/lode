"""Planner agent for generating search strategies."""

from pydantic import BaseModel, Field
from agents import Agent


class WebSearchItem(BaseModel):
    reason: str = Field(description="Your reasoning for why this search is important to the query.")
    query: str = Field(description="The search term to use for the web search.")


class WebSearchPlan(BaseModel):
    searches: list[WebSearchItem] = Field(
        description="A list of web searches to perform to best answer the query."
    )


def create_planner_agent(model: str, search_count: int) -> Agent:
    """Create a planner agent with the specified model and search count."""
    instructions = (
        f"You are a helpful research assistant. Given a query, come up with a set of web searches "
        f"to perform to best answer the query. Output {search_count} terms to query for."
    )
    return Agent(
        name="PlannerAgent",
        instructions=instructions,
        model=model,
        output_type=WebSearchPlan,
    )

"""Evaluator agent for assessing research quality and identifying gaps."""

from pydantic import BaseModel, Field
from agents import Agent


class ResearchGap(BaseModel):
    topic: str = Field(description="The topic or area that needs more research.")
    reason: str = Field(description="Why this gap matters for the query.")
    suggested_query: str = Field(description="A search query to fill this gap.")


class EvaluationResult(BaseModel):
    coverage_score: int = Field(
        description="Score from 1-10 indicating how well the research covers the query.",
        ge=1,
        le=10,
    )
    is_sufficient: bool = Field(
        description="Whether the research is sufficient to write a comprehensive report."
    )
    key_findings: list[str] = Field(
        description="The main findings from the research so far."
    )
    gaps: list[ResearchGap] = Field(
        description="Identified gaps that could be filled with additional searches."
    )
    reasoning: str = Field(
        description="Explanation of the evaluation and recommendation."
    )


INSTRUCTIONS = """You are a research quality evaluator. Given a research query and the summaries 
collected so far, assess whether the research is sufficient to write a comprehensive report.

Your evaluation should consider:
- Coverage: Does the research address all aspects of the query?
- Depth: Is there enough detail on key topics?
- Recency: Is the information current and relevant?
- Diversity: Are multiple perspectives or sources represented?

Be critical but fair. Research is sufficient when:
- Core aspects of the query are addressed with concrete information
- There's enough material for a substantive 1000+ word report
- Major gaps would require significantly more searches (5+) to fill

If research is insufficient, identify the most important gaps and suggest specific 
search queries that would help fill them. Limit suggestions to 1-3 high-impact gaps."""


def create_evaluator_agent(model: str) -> Agent:
    """Create an evaluator agent with the specified model."""
    return Agent(
        name="EvaluatorAgent",
        instructions=INSTRUCTIONS,
        model=model,
        output_type=EvaluationResult,
    )


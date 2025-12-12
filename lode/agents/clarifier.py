"""Clarifier agent for generating clarifying questions."""

from pydantic import BaseModel, Field
from agents import Agent


class ClarifyingQuestion(BaseModel):
    label: str = Field(description="A brief 2-4 word label for this question.")
    question: str = Field(description="The full clarifying question to ask the user.")


class ClarifyingQuestions(BaseModel):
    questions: list[ClarifyingQuestion] = Field(
        description="Exactly 3 clarifying questions to better understand the user's intent."
    )


INSTRUCTIONS = (
    "You are a research assistant helping to clarify a user's query before conducting research. "
    "Given a query, generate exactly 3 clarifying questions that would help you better understand "
    "what the user is looking for.\n\n"
    "Your questions should:\n"
    "- Help narrow down the scope or focus of the research\n"
    "- Clarify any ambiguous terms or concepts\n"
    "- Understand the user's goals, constraints, or preferences\n\n"
    "Each question should have a brief label (2-4 words) and the full question text."
)


def create_clarifier_agent(model: str) -> Agent:
    """Create a clarifier agent with the specified model."""
    return Agent(
        name="ClarifierAgent",
        instructions=INSTRUCTIONS,
        model=model,
        output_type=ClarifyingQuestions,
    )


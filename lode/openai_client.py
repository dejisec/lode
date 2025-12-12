"""Centralized OpenAI abstraction layer with retry and logging."""

import asyncio
import random
import sys
from dataclasses import dataclass

from agents import Agent, Runner
from agents.result import RunResult


@dataclass
class TokenUsage:
    """Token usage from an API call."""
    prompt_tokens: int
    completion_tokens: int
    total_tokens: int

    def to_dict(self) -> dict[str, int]:
        return {
            "prompt_tokens": self.prompt_tokens,
            "completion_tokens": self.completion_tokens,
            "total_tokens": self.total_tokens,
        }


@dataclass
class AgentResult:
    """Result from an agent run with metadata."""
    output: RunResult
    token_usage: TokenUsage | None


def _log(message: str) -> None:
    """Log to stderr for debugging."""
    print(f"[openai_client] {message}", file=sys.stderr)


def _should_retry(exception: Exception) -> bool:
    """Determine if an exception is retryable."""
    error_str = str(exception).lower()

    # Rate limits
    if "rate" in error_str and "limit" in error_str:
        return True
    if "429" in error_str:
        return True

    # Server errors
    if "500" in error_str or "502" in error_str or "503" in error_str or "504" in error_str:
        return True
    if "server" in error_str and "error" in error_str:
        return True

    # Connection errors
    if "connection" in error_str or "timeout" in error_str:
        return True

    return False


def _extract_token_usage(result: RunResult) -> TokenUsage | None:
    """Extract token usage from a RunResult if available."""
    try:
        if hasattr(result, "raw_responses") and result.raw_responses:
            total_prompt = 0
            total_completion = 0
            for response in result.raw_responses:
                if hasattr(response, "usage") and response.usage:
                    total_prompt += getattr(response.usage, "prompt_tokens", 0) or 0
                    total_completion += getattr(response.usage, "completion_tokens", 0) or 0
            if total_prompt > 0 or total_completion > 0:
                return TokenUsage(
                    prompt_tokens=total_prompt,
                    completion_tokens=total_completion,
                    total_tokens=total_prompt + total_completion,
                )
    except Exception:
        pass
    return None


class AgentRunner:
    """Centralized wrapper for all OpenAI agent calls with retry logic."""

    def __init__(self, default_max_retries: int = 3):
        self.default_max_retries = default_max_retries

    async def run(
        self,
        agent: Agent,
        input_text: str,
        *,
        max_retries: int | None = None,
    ) -> AgentResult:
        """
        Run an agent with retry logic.

        Args:
            agent: The agent to run
            input_text: Input text for the agent
            max_retries: Max retry attempts (uses instance default if None)

        Returns:
            AgentResult with output and token usage
        """
        retries = max_retries if max_retries is not None else self.default_max_retries
        last_exception: Exception | None = None

        for attempt in range(retries + 1):
            try:
                _log(f"Running {agent.name} (attempt {attempt + 1}/{retries + 1})")

                result = await Runner.run(agent, input_text)

                token_usage = _extract_token_usage(result)
                if token_usage:
                    _log(f"{agent.name} used {token_usage.total_tokens} tokens")

                return AgentResult(
                    output=result,
                    token_usage=token_usage,
                )

            except Exception as e:
                last_exception = e

                if attempt < retries and _should_retry(e):
                    base_delay = 2 ** attempt
                    jitter = random.uniform(0, 0.5)
                    delay = base_delay + jitter
                    _log(f"{agent.name} failed: {e}. Retrying in {delay:.1f}s...")
                    await asyncio.sleep(delay)
                else:
                    _log(f"{agent.name} failed permanently: {e}")
                    raise

        if last_exception:
            raise last_exception
        raise RuntimeError("Unexpected retry loop exit")


agent_runner = AgentRunner()

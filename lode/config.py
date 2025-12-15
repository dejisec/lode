"""Runtime configuration received from Rust CLI."""

from dataclasses import dataclass


@dataclass
class Config:
    """Configuration passed from Rust CLI via request JSON."""
    model: str
    search_count: int
    max_iterations: int = 10
    max_searches: int = 15
    auto_decide: bool = True

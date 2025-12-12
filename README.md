# Lode

Multi-agent research system that orchestrates AI agents to perform deep web research and synthesize comprehensive reports.

## Architecture

Lode uses a hybrid Rust/Python architecture:

```text
┌─────────────────────────────────────────────────────────────┐
│                        Rust CLI                             │
│  • Argument parsing, config loading                         │
│  • Process orchestration                                    │
│  • Artifact storage (runs/<run_id>/)                        │
│  • Output formatting (human/json/quiet)                     │
└─────────────────────┬───────────────────────────────────────┘
                      │ JSON stdin/stdout
┌─────────────────────▼───────────────────────────────────────┐
│                     Python Runner                           │
│  • OpenAI API interaction                                   │
│  • Research logic (plan → search → synthesize)              │
│  • Multi-agent orchestration                                │
└─────────────────────────────────────────────────────────────┘
```

### Agents

| Agent | Role |
|-------|------|
| **Planner** | Generates search strategy from query |
| **Search** | Executes web searches, summarizes results |
| **Writer** | Synthesizes research into comprehensive report |

## Installation

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Python 3.12](https://www.python.org/)
- [uv](https://github.com/astral-sh/uv) (Python package manager)

### Setup

```bash
# Clone the repository
git clone <repo-url>
cd lode

# Install Python dependencies
uv sync

# Build the Rust CLI
cd cli && cargo build --release
```

### Configuration

```bash
# Copy example environment file
cp env.example .env

# Edit .env with your OpenAI API key
OPENAI_API_KEY=sk-...
```

## Usage

```bash
# Basic usage
./cli/target/release/lode-cli "What are the latest developments in quantum computing?"

# With custom model
./cli/target/release/lode-cli --model gpt-4o-mini "Explain CRISPR gene editing"

# Adjust search depth
./cli/target/release/lode-cli --search-count 10 "Climate change mitigation strategies"

# JSON output (for programmatic use)
./cli/target/release/lode-cli --json "Your query"

# Quiet mode (suppress progress, show only report)
./cli/target/release/lode-cli --quiet "Your query"
```

### CLI Options

```
Usage: lode-cli [OPTIONS] [QUERY]...

Arguments:
  [QUERY]...  The research query to investigate

Options:
      --model <MODEL>                OpenAI model (default: gpt-4o, env: LODE_MODEL)
      --search-count <SEARCH_COUNT>  Number of searches (default: 5, env: LODE_SEARCH_COUNT)
      --json                         Output JSON lines instead of human-readable
  -q, --quiet                        Suppress progress, show only report and errors
  -h, --help                         Print help
```

## Artifacts

Each research run produces artifacts in `runs/<run_id>/`:

```text
runs/<run_id>/
  request.json       # Input parameters
  metadata.json      # Model, timing, trace info
  output.md          # Final markdown report
  prompts/
    001-planner.txt  # Planner input
    002-search.txt   # Search inputs
    ...
    007-writer.txt   # Writer input
  raw_responses/
    001-planner.json # Planner output (search plan)
    002-search.json  # Search outputs
    ...
    007-writer.json  # Writer output (report)
```

## Configuration Priority

1. CLI flags (`--model`, `--search-count`)
2. Environment variables (`LODE_MODEL`, `LODE_SEARCH_COUNT`)
3. Defaults (`gpt-4o`, `5`)

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `OPENAI_API_KEY` | OpenAI API key (required) | - |
| `LODE_MODEL` | Model to use | `gpt-4o` |
| `LODE_SEARCH_COUNT` | Number of web searches | `5` |


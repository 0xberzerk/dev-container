# TODO — Secure Dev Container for Solidity Auditing

## Vision

Transform this repo (cloned from the dev-environment template) into a **secure, containerized environment for auditing untrusted Solidity code**. Target audience: security auditors who need sandboxed, reproducible tooling.

The agents and tools act as an **extension of the auditor's brain** — optimizing the process, making audits faster and more accurate. The auditor works manually in parallel; agent output is documentation that augments their own findings.

## Decisions Made

- **Primary user:** Auditors reviewing untrusted code
- **Audit tools:** Slither, Aderyn, Chimera (Recon — supports Echidna, Medusa, Halmos, Foundry)
- **Scope:** Standalone repo (not part of the template). Reuse relevant pieces, remove what doesn't apply
- **Network:** Restricted by default — whitelist only package registries and RPC endpoints
- **Workflow:** Auditors review code, write tests/POCs, and run analysis tools — they don't deploy, push, or maintain CI
- **No style enforcement on target code:** Third-party code won't follow our conventions. Linting/formatting is noise. Review must focus on architectural risks, state inconsistencies, and edge conditions that can trigger exploits — not style
- **No BTT/bulloak:** Auditors write targeted POCs for specific scenarios, not exhaustive branch coverage. Tree-driven scaffolding adds overhead without value
- **Fork-first testing:** Auditors almost always work against forked chains. Fork testing is the default, not the exception
- **No Node.js:** All node deps (solhint, commitlint, husky, lint-staged) are dropped. No reason to keep the Node.js toolchain
- **Dynamic specialist agents:** The orchestrator profiles the codebase and decides which specialist agents to launch — no fixed roster. Specialists are scoped by domain and fed relevant Solodit bugs via MCP
- **Double-pass architecture:** Architect Pass 1 maps the territory, specialists audit in parallel, Architect Pass 2 hunts cross-domain interactions informed by all specialist findings
- **Fuzz testing runs in parallel with analysis:** A dedicated Fuzz Agent generates and runs fuzz tests (Foundry + Chimera) concurrently with specialist analysis. Reads its own corpus to fix unwanted reverts. Always produces bounded + unbounded entry points
- **Static analysis as pre-processing:** Slither and Aderyn run before the agent pipeline. Their outputs feed into the Architect as structured input alongside @audit tags — not as a separate specialist
- **POC writer is a skill, not an agent:** POCs are only written on human-auditor invocation for specific findings. Not automated, not part of the pipeline
- **Auditor reviews findings via inline tags:** After the pipeline, the auditor discusses findings with the agent using `@audit-response` tags in the code, creating a local dialogue
- **Severity scope:** Only critical, high, and medium. Low, informational, and gas findings are excluded from KB ingestion and agent output — they are noise for security auditing
- **Local build only:** No container image publishing. Auditors build the dev container locally

## Open Questions

- Chimera exact installation steps (need to deep-dive into Recon docs)
- VS Code extensions to pre-install (Solidity, Slither, Recon Extension?)
- How to handle fork RPC URLs securely inside the container
- Solodit API: confirm query capabilities (category filtering, severity filtering, sub-categorization)
- Specialist agent taxonomy: define the known domains and how to map Solodit categories to them
- KB JSON schema: define fields per entry (Solodit ID, category, severity, curation status, relevance score, auditor notes, etc.)
- Fuzz Agent: how to parse Foundry/Chimera corpus output into actionable revert diagnostics
- Static analysis normalization: define the unified JSON schema for `analysis/static-analysis.json` (field mapping from Slither + Aderyn native formats)

---

## Agent Architecture

### Human-First Annotation (`@audit` Tags)

The auditor performs the initial codebase read and annotates source files with `@audit` tags before launching the agent pipeline. This gives the Architect grounded starting signals instead of cold-reading the codebase, improving accuracy and reducing hallucinated integration detection.

Tags are an **accelerator, not a requirement** — the Architect still works autonomously with zero tags, just with lower confidence.

#### Tag Taxonomy

**Pre-pipeline tags** (auditor annotates before launching agents):

| Tag | Purpose | Example |
|---|---|---|
| `@audit-integration` | External protocol or standard used | `@audit-integration CCIP` |
| `@audit-trust-boundary` | Who is trusted vs untrusted | `@audit-trust-boundary relayer is untrusted` |
| `@audit-attention` | Specific concern, smell, or suspicious pattern | `@audit-attention rounding in withdraw()` |
| `@audit-entry` | Key entry point / attack surface | `@audit-entry depositAndBridge()` |

**Post-pipeline tags** (auditor reviews findings and responds inline):

| Tag | Purpose | Example |
|---|---|---|
| `@audit-confirmed` | Auditor agrees with agent finding | `@audit-confirmed valid — needs POC` |
| `@audit-false-positive` | Auditor rejects agent finding with reason | `@audit-false-positive fee is capped by admin` |
| `@audit-discuss` | Auditor wants to discuss further with agent | `@audit-discuss could this interact with the bridge timeout?` |
| `@audit-escalate` | Auditor upgrades severity or flags for deeper analysis | `@audit-escalate this is worse than medium — attacker controls both params` |

#### Placement Rules

- Tags go in inline comments (`// @audit-*`) next to the relevant code
- One tag per comment line for parsability
- Free-text after the tag type — no rigid schema, just be descriptive
- Multiple tags on the same line are allowed but discouraged (harder to parse)
- Post-pipeline tags reference findings by placing them next to the code the finding targets

#### Example — Pre-Pipeline

```solidity
contract Bridge {
    // @audit-integration CCIP — uses Router for cross-chain messaging
    // @audit-trust-boundary relayer is untrusted, can call executeMessage()

    function depositAndBridge(uint256 amount) external { // @audit-entry
        // @audit-attention no slippage check before swap
        uint256 received = uniswapRouter.exactInputSingle(...);
        // @audit-attention rounding down on fee calculation — dust accumulation?
        uint256 fee = received * feeRate / BASIS_POINTS;
    }
}
```

#### Example — Post-Pipeline (Auditor Reviewing Findings)

```solidity
function withdraw(uint256 shares) external {
    // @audit-confirmed valid — rounding favors attacker on small amounts
    // @audit-discuss could this be chained with the fee bypass in depositAndBridge()?
    uint256 assets = shares * totalAssets() / totalSupply();

    // @audit-false-positive admin has a 5% cap on fee, dust extraction not profitable
    uint256 fee = assets * feeRate / BASIS_POINTS;
}
```

### Auditor Feedback Loop (Post-Pipeline)

After the pipeline produces findings, the auditor reviews them **in the code** using post-pipeline `@audit` tags. This creates a local dialogue:

1. Agent pipeline produces findings, each referencing specific code locations
2. Auditor reads findings, goes to each location, and annotates with `@audit-confirmed`, `@audit-false-positive`, `@audit-discuss`, or `@audit-escalate`
3. Auditor invokes the agent to process feedback:
   - `@audit-discuss` tags trigger a focused conversation about that specific concern
   - `@audit-false-positive` tags with reasons feed back into the KB (deprioritize similar patterns)
   - `@audit-escalate` tags can trigger deeper specialist re-analysis on that area
   - `@audit-confirmed` tags can be passed to the POC writer skill when the auditor is ready
4. Feedback persists in the codebase as documentation of the audit reasoning

### Pipeline

```
0. AUDITOR (human)
   │  Reads codebase, annotates with @audit tags:
   │    @audit-integration, @audit-trust-boundary,
   │    @audit-attention, @audit-entry
   │
0.5 STATIC ANALYSIS (pre-processing)
   │  Runs Slither + Aderyn on the codebase
   │  Output normalized into structured format
   │  Fed to Architect P1 as additional input (not a specialist — pre-processing)
   │  Detectors: known vulnerability patterns, data flow, control flow
   │
1. ARCHITECT (Pass 1 — Validator + Gap Finder)
   │  Inputs: full codebase + @audit tags + static analysis output
   │  Tasks:
   │    a. Parse all @audit tags as ground truth / high-confidence signals
   │    b. Ingest static analysis output — correlate with @audit-attention tags
   │    c. Validate: do tagged integrations actually exist? Are boundaries correct?
   │    d. Extend: what did the auditor miss? (new integrations, implicit trust, hidden entry points)
   │    e. Flag disagreements explicitly:
   │         "Auditor tagged X, but I also see Y"
   │         "Auditor flagged Z — confirmed, and it interacts with W"
   │         "Slither flagged reentrancy in foo() — auditor didn't tag this"
   │    f. Produce structured Codebase Profile:
   │         - Confirmed integrations (from tags + own analysis)
   │         - Architecture pattern (e.g., cross-chain vault with fee distribution)
   │         - Trust boundaries (confirmed + discovered)
   │         - Entry points and privilege levels
   │         - Auditor attention points (carried forward from @audit-attention tags)
   │         - Static analysis highlights (carried forward, deduplicated)
   │    g. Decide which specialists to launch + Solodit query parameters for each
   │    h. Define fuzz targets for the Fuzz Agent (contracts, entry points, invariants)
   │
   │  Fallback: if zero @audit tags are found, Architect runs autonomously
   │  (same analysis, lower confidence, flags uncertainty explicitly)
   │
   │  ┌── PARALLEL EXECUTION ──────────────────────────────────────────────┐
   │  │                                                                     │
   │  ├──→ 2a. Specialist: CCIP         (Solodit: CCIP bugs, sub-filtered) │
   │  ├──→ 2b. Specialist: UniswapV3    (Solodit: UniV3 bugs, sub-filtered)│
   │  ├──→ 2c. Specialist: ERC4626      (Solodit: vault bugs, sub-filtered)│
   │  ├──→ 2d. Specialist: Fee Dist.    (Solodit: Synthetix/reward bugs)   │
   │  ├──→ 2e. Specialist: Heuristics   (logic bugs, economic issues —     │
   │  │         focuses on what static analysis CAN'T catch:               │
   │  │         cross-function state, economic invariants, logic flaws)    │
   │  │    ... (dynamic — orchestrator decides which to launch)            │
   │  │                                                                     │
   │  ├──→ 2f. FUZZ AGENT (coverage-focused — does NOT analyze results)     │
   │  │    │  Goal: achieve maximum relevant coverage, nothing else         │
   │  │    │  Generates fuzz test suites (Foundry + Chimera)               │
   │  │    │  Targets: state-changing + state-reading functions only        │
   │  │    │    (pure functions are excluded — no relevant state to fuzz)   │
   │  │    │  Always produces BOTH bounded and unbounded entry points      │
   │  │    │  Reads corpus to identify unwanted reverts blocking coverage  │
   │  │    │  Iterates: fix revert → re-run → read corpus → repeat        │
   │  │    │  Outputs: coverage report + raw results (no interpretation)   │
   │  │    │  Analysis of results is done by the relevant specialist agent │
   │  │    │                                                                │
   │  └────────────────────────────────────────────────────────────────────┘
   │
   │  All specialists + Fuzz Agent run in PARALLEL
   │  Specialists receive: scoped codebase + @audit-attention tags + curated KB entries
   │  Fuzz Agent receives: full codebase + Architect fuzz targets + @audit-entry tags
   │  Specialists output structured findings in a common format
   │  Fuzz Agent outputs coverage report + raw results (findings come from specialists)
   │
   ├──→ 3. CONSOLIDATOR
   │    │  Receives: specialist findings + fuzz coverage/raw results + static analysis
   │    │  Routes fuzz results to relevant specialists for interpretation
   │    │  Deduplicates findings (specialists + static analysis)
   │    │  Normalizes severity ratings
   │    │  Cross-references overlapping reports
   │    │  Maps findings back to @audit-attention tags (confirmed / new / missed)
   │    │  Produces unified findings list
   │
   ├──→ 4. ARCHITECT (Pass 2 — Intersection Auditor)
   │       Receives: Codebase Profile + Consolidated Findings + fuzz coverage + @audit tags
   │       Focus ONLY on cross-domain interactions:
   │         "Does the ERC4626 accounting break when triggered via a CCIP callback?"
   │         "Can the fee distribution be manipulated through a Uniswap flash swap?"
   │       Cross-references fuzz coverage with specialist findings:
   │         "Fuzz couldn't reach withdraw() path — specialist flagged rounding there, needs manual review"
   │         "Fuzz invariant violation in deposit() — correlates with specialist's state inconsistency finding"
   │       Validates whether auditor's @audit-attention concerns were confirmed or not
   │       Adds intersection findings to the final report
   │
   └──→ 5. AUDITOR REVIEW (human — post-pipeline)
          Receives: Final Report (see format below)
          Reviews findings in the code using post-pipeline @audit tags:
            @audit-confirmed, @audit-false-positive, @audit-discuss, @audit-escalate
          Discusses @audit-discuss items with agent for focused analysis
          Invokes /poc skill for confirmed findings that need exploit tests
          Feedback flows back to KB (false positives → noise, confirmed → useful)
```

### Static Analysis Integration

Slither and Aderyn run as **pre-processing**, not as agents. Their output is written to well-known file paths in a fixed format so the Architect knows exactly where to look — no discovery needed.

#### Output Convention

| Tool | Format | Output Path |
|---|---|---|
| Slither | JSON | `analysis/slither.json` |
| Aderyn | JSON | `analysis/aderyn.json` |
| Normalized (merged) | JSON | `analysis/static-analysis.json` |

- Tools are invoked with explicit format flags (e.g., `slither . --json analysis/slither.json`, `aderyn . -o analysis/aderyn.json`)
- A normalization step merges both outputs into `analysis/static-analysis.json` using a unified schema
- The Architect reads **only** `analysis/static-analysis.json` — never raw tool output directly
- The `analysis/` directory is gitignored (generated artifacts, not source)

#### Role Separation: Static Analysis vs Heuristics Specialist

| Concern | Static Analysis (Slither/Aderyn) | Heuristics Specialist (Agent) |
|---|---|---|
| Reentrancy | Detects known patterns (CEI violations) | Cross-function, cross-contract reentrancy through state |
| Access control | Missing modifiers, unprotected functions | Privilege escalation paths, implicit trust assumptions |
| Overflow | Unchecked arithmetic | Economic overflow (value extraction at boundaries) |
| Data flow | Tainted inputs, unused returns | State machine inconsistencies, temporal dependencies |
| Scope | Syntactic / structural patterns | Semantic / logical / economic reasoning |

The Heuristics specialist explicitly **excludes** what static analysis already covers. It focuses on what requires reasoning: logic bugs, economic invariant violations, cross-function state corruption, temporal dependencies.

### Fuzz Agent (Coverage Engineer)

A dedicated agent focused **exclusively on achieving relevant coverage**. It is a test engineer, not an auditor — it does not analyze or interpret results. If fuzz output needs interpretation, the relevant specialist agent handles that.

#### Scope: What Gets Fuzzed

- **State-changing functions** (write to storage, emit events, transfer value) — always included
- **State-reading functions** (view functions that read storage) — included as assertions/invariant checks
- **Pure functions** — excluded. No relevant state to fuzz, deterministic by definition

#### Design Principles

- **Bounded + unbounded entry points:** Every fuzz target gets both. Bounded entry points constrain inputs to realistic ranges (avoids noise from impossible states). Unbounded entry points explore the full input space (catches edge cases bounded tests miss). **Never only bounded** — that over-constrains the fuzzer and hides real bugs
- **Corpus-driven revert fixing:** After each run, the agent reads the corpus (Foundry's `cache/` or Chimera's output). Its only goal is to identify unwanted reverts that block coverage and fix them — adjusting bounds, adding proper `vm.assume()`, or restructuring call sequences. The agent iterates until coverage stabilizes
- **Dual framework:** Generates tests for both Foundry (native fuzz) and Chimera (Echidna/Medusa/Halmos via Recon). Foundry for speed, Chimera for deeper stateful exploration
- **Invariant-first:** Tests are organized around protocol invariants (e.g., "total shares <= total assets", "only admin can pause"), not around individual functions
- **No analysis:** The agent reports what happened (coverage, reverts, violations). It does not decide what it means. Interpretation is the specialist's job

#### Outputs (raw, uninterpreted)

- Coverage metrics (which state-changing/reading functions were reached, which branches)
- Coverage gaps (what the fuzzer couldn't reach, and why — which reverts blocked it)
- Raw invariant violations (reproduction sequences, without severity assessment)
- Revert log (expected vs unexpected, with the fix applied or reason it couldn't be fixed)
- Ghost variable snapshots (expected vs actual state — raw data for specialists)

#### Handoff to Specialists

When fuzz results need interpretation, the Consolidator routes them to the relevant specialist:
- Invariant violation in a vault → ERC4626 specialist interprets
- Unexpected revert in a bridge callback → CCIP specialist interprets
- Coverage gap in fee logic → Fee Distribution specialist flags for manual review
- General state inconsistency → Heuristics specialist analyzes

### Final Report Format

The pipeline produces a structured report delivered to the auditor.

#### Report Structure

```
1. EXECUTIVE SUMMARY
   - Total findings by severity
   - @audit-attention tag coverage (confirmed / new / missed)
   - Key risk areas

2. CODEBASE PROFILE (from Architect P1)
   - Architecture overview
   - Integrations (confirmed + discovered)
   - Trust boundaries
   - Entry points

3. FINDINGS (from Consolidator — specialists only)
   Per finding:
   - Severity, title, location
   - Description + exploit scenario
   - Source: which specialist produced it
   - @audit tag traceability (if linked to auditor's initial annotation)
   - Solodit references (if any)
   - Fuzz corroboration (if fuzz coverage data supports this finding)

4. INTERSECTION ANALYSIS (from Architect P2)
   - Cross-domain interaction risks
   - Findings that span multiple specialist domains
   - Fuzz coverage gaps correlated with specialist findings

5. FUZZ COVERAGE SUMMARY (from Fuzz Agent — raw, uninterpreted)
   - Functions reached (state-changing + state-reading)
   - Coverage gaps (what couldn't be reached, which reverts blocked it)
   - Raw invariant violations (routed to specialists for interpretation)
   - Revert fixes applied (what the agent tuned to achieve coverage)

6. STATIC ANALYSIS SUMMARY
   - Slither + Aderyn findings (deduplicated against agent findings)
   - False positive rate from previous audits (if KB has history)

7. COVERAGE GAPS
   - What wasn't analyzed (no specialist matched, no fuzz coverage, no static detection)
   - Recommended manual review areas
```

### Context Budget Strategy

LLM agents have finite context. The pipeline passes significant data through multiple stages. Strategy for staying within limits:

- **Architect P1:** Receives full codebase + tags + static analysis. For large codebases, prioritize files with @audit tags and high static analysis signal. Summarize low-signal files
- **Specialists:** Receive only scoped codebase (files relevant to their domain). Architect defines the file set per specialist
- **Fuzz Agent:** Receives full codebase (needs it for compilation) but Architect-defined targets focus its generation effort
- **Consolidator:** Receives structured findings only (not code). Compact format
- **Architect P2:** Receives Codebase Profile summary (not full code) + consolidated findings + @audit tags. If findings exceed budget, prioritize by severity (critical/high first)

### Finding Output Schema (specialists only)

Findings are produced by specialist agents (including Heuristics). The Fuzz Agent does not produce findings — it produces coverage data and raw results that specialists interpret.

Each finding must include:
- **severity:** critical / high / medium / low / informational
- **title:** concise description
- **location:** file path + function/line
- **source:** which specialist produced this finding
- **description:** what the issue is and why it matters
- **exploit_scenario:** step-by-step how an attacker could exploit this
- **solodit_reference:** related Solodit bug ID(s) that informed the finding (if any)
- **audit_tag_reference:** linked `@audit` tag(s) that prompted this finding, if any (traceability back to auditor's initial signals)
- **fuzz_corroboration:** reproduction sequence or invariant violation from fuzz results that supports this finding (if any)
- **static_analysis_corroboration:** Slither/Aderyn detector that flagged the same area (if any)

### MCP Integration — Solodit

- MCP server connects to Solodit API
- Orchestrator builds scoped queries per specialist (category + severity + sub-category)
- Each specialist receives only bugs relevant to its domain
- Sub-filtering is critical: not all CCIP bugs apply to every CCIP integration — filter by usage pattern (messaging vs. token pools vs. fee handling)
- **Agents never consume raw MCP output** — all results pass through the Knowledge Base first (see below)

### Knowledge Base (Local Curated Vulnerability Index)

Raw Solodit queries return noisy, loosely-related results. Feeding those directly to specialists wastes context and reduces accuracy. The Knowledge Base is a local layer between the MCP and the agents that caches, filters, ranks, and learns over time.

#### Severity Filter

Only **critical**, **high**, and **medium** severity findings are stored and surfaced. Low, informational, and gas findings are discarded at ingestion — they add noise without meaningful signal for security auditing.

#### Directory Layout

```
KnowledgeBase/
├── raw/                          # Layer 1 — Raw API cache
│   └── {query-fingerprint}.json  # One file per unique query
├── curated/                      # Layer 2 — Curated lists, ordered by severity
│   ├── critical.json             # Critical findings (highest priority)
│   ├── high.json                 # High findings
│   └── medium.json               # Medium findings
└── seeds/                        # Auditor pre-seeded entries
    └── {domain-or-protocol}.json # Manual imports, war stories, bookmarks
```

- Curated lists are **ordered by severity**: critical → high → medium
- Agents always read in this order — critical entries are consumed first within context budget
- `KnowledgeBase/` is gitignored (generated + audit-specific, not shared)

#### Three-Layer Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Layer 3: AGENT CONSUMPTION                                 │
│  Specialists receive pre-filtered, ranked entries            │
│  scoped to their domain — never raw Solodit output          │
│  Read order: critical.json → high.json → medium.json        │
├─────────────────────────────────────────────────────────────┤
│  Layer 2: CURATED INDEX                                     │
│  Ranking + tagging + feedback loop                          │
│  Entries marked: useful / noise / critical                   │
│  Auditor or Architect can curate over time                  │
│  Relevance improves with each audit                         │
│  Only critical/high/medium — low/info/gas discarded         │
├─────────────────────────────────────────────────────────────┤
│  Layer 1: RAW CACHE                                         │
│  MCP fetches from Solodit → local cache                     │
│  TTL-based: same query never hits API twice within window   │
│  Stored as structured JSON per query fingerprint             │
│  Severity filter applied at ingestion                        │
└─────────────────────────────────────────────────────────────┘
```

#### Layer 1 — Raw Cache

- Every MCP query is fingerprinted (category + severity + sub-category + keywords)
- Results stored locally as structured JSON in `KnowledgeBase/raw/`
- **Severity filter at ingestion:** low, informational, and gas findings are discarded before caching
- TTL-based deduplication — same fingerprint skips the API within the cache window
- Cache can be refreshed on demand (`--refresh`) or on a schedule

#### Layer 2 — Curated Index

- After each fetch, a filtering pass scores results by relevance to the current audit context
- Scoring signals: keyword overlap with codebase, integration type match, severity, recency
- Entries are sorted into `KnowledgeBase/curated/critical.json`, `high.json`, `medium.json`
- Each entry carries a curation status:
  - `unreviewed` — fresh from API, not yet seen by a human or agent
  - `useful` — confirmed relevant by auditor or agent feedback
  - `noise` — irrelevant to this domain/pattern, deprioritized in future queries
  - `critical` — high-signal, should always surface for this integration type
- Auditor can curate manually (bulk or per-entry) or let the Architect auto-curate based on which bugs actually led to findings
- Curation persists across audits — the index gets smarter over time

#### Layer 3 — Agent Consumption

- Specialists query the curated index, not the raw cache
- **Read order: critical → high → medium** (always severity-first)
- Within each severity, ranked: `critical` curation > `useful` > `unreviewed` (noise excluded)
- Each specialist gets only entries matching its domain scope
- Context budget: if too many results, truncate from the bottom (lowest severity + lowest relevance first)
- Entries include a `confidence` field so the specialist knows how much to trust each reference

#### Pre-Seeding

The auditor can **pre-seed** the knowledge base before launching the pipeline:
- Import known bugs into `KnowledgeBase/seeds/` (one file per domain or protocol)
- Add personal notes or war stories as curated entries
- Bookmark specific Solodit findings they've seen before
- Same severity filter applies — only critical/high/medium

This follows the same philosophy as `@audit` tags — human knowledge accelerates the agents.

#### Refresh Strategy

- **Per-audit:** fresh queries for new integration types not in the cache
- **Periodic:** scheduled refresh of stale entries (e.g., weekly for active domains)
- **On-demand:** auditor or Architect triggers `--refresh` for a specific domain when Solodit data might have new entries

### Agent Roles Summary

| Agent | Pass | Focus | Context |
|---|---|---|---|
| Auditor (human) | 0 | Annotate codebase with @audit tags | Full codebase |
| Static Analysis | 0.5 | Slither + Aderyn pre-processing | Full codebase |
| Architect P1 | 1 | Validate tags, find gaps, build profile, launch specialists + fuzz | Full codebase + @audit tags + static analysis output |
| Specialists (N) | 2 | Domain-specific vulnerability hunting | Scoped codebase + relevant @audit-attention tags + curated KB entries |
| Heuristics | 2 | Logic bugs, economic issues (excludes static analysis scope) | Full codebase + general Solodit patterns |
| Fuzz Agent | 2 | Achieve coverage: generate tests, fix reverts, report raw results (no analysis) | Full codebase + Architect fuzz targets + @audit-entry tags |
| Consolidator | 3 | Deduplicate, normalize, route fuzz results to specialists, map to @audit tags | Specialist findings + fuzz raw results + static analysis + @audit tags |
| Architect P2 | 4 | Cross-domain interactions, cross-reference fuzz + specialist findings | Codebase Profile (summary) + consolidated findings + @audit tags |
| Auditor (human) | 5 | Review findings, respond with post-pipeline @audit tags, invoke /poc | Final Report + codebase |

---

## Component Evaluation

### Keep (directly useful for auditing)

- [x] `foundry.toml` — core compile, test, fuzz, invariant config
- [x] `forge-std` (lib) — testing foundation

### Drop (not relevant for auditors)

- [ ] `.github/workflows/*` (all 4) — no CI needed
- [ ] `.husky/` (all hooks) — no pushing, no commit format enforcement, no lint-staged
- [ ] `commitlint.config.js` — commit format irrelevant for local POC work
- [ ] `@commitlint/*` (node deps) — goes with above
- [ ] `husky` (node dep) — goes with above
- [ ] `lint-staged` (node dep) — no style enforcement on third-party code
- [ ] `solhint` (node dep) — style linting irrelevant for auditing untrusted code
- [ ] `.solhint.json` (all tiers) — goes with above
- [ ] `.solhintignore` — goes with above
- [ ] `script/` directory — deployment scripts, auditors don't deploy
- [ ] `/foundry-dev:pre-deploy` skill — pre-deploy checklist, irrelevant
- [ ] `/foundry-dev:commit` skill — conventional commit workflow, overkill
- [ ] Plugin hook: `block-dangerous-commands.sh` (PreToolUse) — guards push/branch, not needed
- [ ] Plugin hook: `check-sol-format.sh` (PostToolUse) — formatting reminder, noise on third-party code
- [ ] `.claude/rules/git-conventions.md` — branch naming, commit format, not relevant
- [ ] `.claude/rules/solidity-style.md` — style conventions, not relevant for reviewing third-party code
- [ ] `lcov.info` — committed coverage artifact, shouldn't be in repo
- [ ] `bulloak` — tree-driven scaffolding doesn't match how auditors write POCs
- [ ] `/foundry-dev:btt-workflow` skill — BTT workflow, auditors target specific scenarios not exhaustive branch coverage
- [ ] Plugin hook: `bulloak check` (Stop) — no trees to sync
- [ ] All `.tree` files — BTT specs not needed
- [ ] `package.json`, `yarn.lock`, `.yarnrc`, `.nvmrc` — no node deps remain, entire Node.js toolchain dropped
- [ ] `gas_reports = ["*"]` in foundry.toml — gas optimization reporting is a dev concern, not audit focus

### Adapt (keep but modify)

- [ ] Counter example code (`src/`, `test/`) — remove entirely, replace with empty audit workspace dirs
- [ ] `test/Base.t.sol` — rewrite actors for audit context: `attacker`, `victim`, `admin`, `protocolOwner` instead of generic `alice`/`bob`
- [ ] Test directory structure — rename from dev-centric `unit/integration/invariant` to audit-centric `poc/`, `invariant/`, `fuzz/`
- [ ] `foundry.toml` profiles — remove `integration` profile with fork exclusion (fork is the default for auditors); simplify to default + ci profiles; remove `gas_reports`; possibly add profiles for audit tools (Slither, Medusa, etc.)
- [ ] `CLAUDE.md` — rewrite for audit workflow
- [ ] `README.md` — rewrite for audit container purpose
- [ ] `.gitignore` — adapt for audit artifacts
- [ ] `.env.example` — reframe around target protocol RPC, not "integration testing"
- [ ] `.claude/rules/best-practices.md` — rewrite: shift from "how to write good code" to "what to look for" — architectural risks, state machine inconsistencies, trust boundary violations, edge conditions, subtle interactions between features
- [ ] `.claude/rules/testing.md` — rewrite: remove BTT/bulloak methodology, keep fuzz + invariant patterns, reframe around POC writing (targeted scenario tests, not exhaustive branch coverage)
- [ ] `.claude/rules/guardrails.md` — keep secrets policy, remove "Key Management" section about deployment keystores (auditors don't deploy)
- [ ] `foundry-dev` plugin — rename to `audit` or `sec-audit` plugin, restructure for audit workflow
- [ ] `solidity-reviewer` agent — rewrite as Architect Pass 1 (orchestrator) or replace entirely with the new agent pipeline
- [ ] `test-writer` agent — rewrite as Fuzz Agent (generates fuzz suites, not POCs). POC writing moves to `/poc` skill
- [ ] `/foundry-dev:review-contract` skill — rewrite to reference security-focused rules, not style-focused ones

---

## Review Philosophy

The goal is **not** to check if code follows conventions. It's to find the small thing that, under a specific condition, triggers a massive loss.

Focus areas for the review approach:
- **State machine inconsistencies** — can the contract reach a state the developer didn't anticipate?
- **Trust boundary violations** — where does the code assume trusted input from untrusted sources?
- **Edge condition interactions** — what happens when two features interact at boundary values?
- **Reentrancy paths** — not just the obvious ones, but cross-function and cross-contract
- **Oracle/price manipulation** — stale data, sandwich opportunities, manipulation windows
- **Rounding and precision loss** — truncations that only matter at specific boundaries
- **Access control gaps** — missing checks, privilege escalation paths, unprotected initializers
- **Token assumption failures** — weird ERC20 behaviors (rebasing, fee-on-transfer, blacklists, return value quirks)
- **Temporal dependencies** — block.timestamp manipulation, deadline bypasses, ordering assumptions
- **Economic invariant violations** — can an attacker extract more value than they should?

---

## High-Level Tasks

### 1. Dev Container Setup
- [ ] Create `.devcontainer/` with Dockerfile and `devcontainer.json`
- [ ] Base image selection (Ubuntu or Foundry official)
- [ ] Install Foundry toolchain (forge, cast, anvil, chisel)
- [ ] Install Python + Slither
- [ ] Install Aderyn (Rust-based)
- [ ] Install Chimera / Recon tooling (research exact install steps — see docs: https://github.com/Recon-Fuzz/recon-docs)
- [ ] Install Echidna, Medusa, Halmos (Chimera deps)

### 2. Network Isolation (DEFERRED)

Deferred until the core tooling is running. Layer in as a separate Docker Compose profile.

**Decision notes:**
- Base approach: custom Docker bridge network + domain-level proxy (e.g., Squid) for whitelisting
- DNS-based is simpler but bypassable via hardcoded IPs — not sufficient alone
- iptables inside the container is brittle and hard to maintain
- Default state: no internet access, whitelist only what's needed

**Whitelist targets:**
- Package registries: PyPI, crates.io, GitHub (foundryup, forge install)
- RPC endpoints: Alchemy, Infura, Tenderly (auditor-configured)
- Solodit API (KB pipeline)

**TODO when revisited:**
- [ ] Define network restriction strategy (Docker Compose profile with proxy)
- [ ] Whitelist: package registries, RPC endpoints, Solodit API
- [ ] Document how to add custom whitelist entries
- [ ] Test: verify forge install, pip install, cargo install work through the proxy
- [ ] Test: verify arbitrary outbound connections are blocked

### 3. Repo Cleanup (execute Drop + Adapt lists above)
- [ ] Remove all items from the Drop list
- [ ] Execute all adaptations from the Adapt list
- [ ] Verify nothing is broken after cleanup (`forge build`)

### 4. MCP + Solodit Integration
- [ ] Research Solodit API capabilities (endpoints, filtering, rate limits)
- [ ] Build MCP server for Solodit (query by category, severity, sub-category)
- [ ] Define bug taxonomy mapping: Solodit categories → specialist domains
- [ ] Test filtering quality (are the right bugs reaching the right specialists?)

### 5. Knowledge Base (Local Curated Vulnerability Index)
- [ ] Design storage format (structured JSON, query fingerprinting, directory layout)
- [ ] Implement Layer 1 — Raw Cache (fetch, fingerprint, TTL dedup, `--refresh`)
- [ ] Implement Layer 2 — Curated Index (scoring, curation status: unreviewed/useful/noise/critical)
- [ ] Implement Layer 3 — Agent Consumption API (domain-scoped queries, ranking, context budget truncation)
- [ ] Implement auditor pre-seeding workflow (import known bugs, personal notes, bookmarked findings)
- [ ] Implement feedback loop (Architect auto-curates based on which bugs led to findings)
- [ ] Define refresh strategy (per-audit, periodic, on-demand)

### 6. Static Analysis Integration
- [ ] Define unified JSON schema for `analysis/static-analysis.json`
- [ ] Implement Slither runner (→ `analysis/slither.json`)
- [ ] Implement Aderyn runner (→ `analysis/aderyn.json`)
- [ ] Implement normalizer: merge Slither + Aderyn → `analysis/static-analysis.json` (deduplicated, unified schema)
- [ ] Add `analysis/` to `.gitignore`
- [ ] Integration test: verify Architect P1 reads `analysis/static-analysis.json` correctly

### 7. Agent Pipeline Implementation
- [ ] Define `@audit` tag parser (extract pre-pipeline + post-pipeline tags from source files)
- [ ] Define Codebase Profile schema (orchestrator output format — includes static analysis highlights)
- [ ] Define Finding schema (specialist output format — includes `source`, `audit_tag_reference`, `fuzz_evidence`, `static_analysis_corroboration`)
- [ ] Implement Architect Pass 1 (validator + gap finder) agent
- [ ] Implement specialist agent template (parameterized by domain + curated KB entries + @audit-attention tags)
- [ ] Implement Heuristics specialist (logic bugs, economic issues — explicitly excludes static analysis scope)
- [ ] Implement Fuzz Agent — coverage engineer (see Fuzz Agent section):
  - [ ] Target selector (state-changing + state-reading functions only, exclude pure)
  - [ ] Foundry fuzz test generator (bounded + unbounded entry points)
  - [ ] Chimera/Recon test generator (stateful exploration)
  - [ ] Corpus reader + unwanted revert fixer loop (iterate until coverage stabilizes)
  - [ ] Ghost variable tracking + invariant assertion generation
  - [ ] Raw output format (coverage, gaps, violations, revert log — no interpretation)
- [ ] Implement fuzz results → specialist routing in Consolidator
- [ ] Implement Consolidator agent (dedup across specialists + fuzz + static, map to @audit tags)
- [ ] Implement Architect Pass 2 (intersection auditor — cross-references fuzz + specialist findings)
- [ ] Implement Final Report generator (see Report Format section)
- [ ] End-to-end pipeline test on a known vulnerable codebase

### 8. Auditor Feedback Loop
- [ ] Implement post-pipeline @audit tag parser (`@audit-confirmed`, `@audit-false-positive`, `@audit-discuss`, `@audit-escalate`)
- [ ] Implement @audit-discuss handler (focused conversation about specific concern)
- [ ] Implement @audit-escalate handler (re-trigger specialist analysis on specific area)
- [ ] Implement feedback → KB flow (false positives deprioritize similar patterns, confirmed → useful)

### 9. Skills
- [ ] `/poc` — POC writer skill (human-invoked only, for specific confirmed findings)
- [ ] `/audit-start` — launch the agent pipeline (parse tags, run static analysis, trigger Architect P1)
- [ ] `/audit-review` — process post-pipeline @audit tags and enter feedback dialogue
- [ ] Evaluate and implement additional audit-focused skills

### 10. CI/CD Adaptation
- [ ] Adapt or remove GitHub Actions for audit workflows

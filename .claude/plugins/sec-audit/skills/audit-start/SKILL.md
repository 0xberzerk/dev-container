---
name: audit-start
description: "Launch the full audit agent pipeline. Runs static analysis, profiles the codebase, launches specialist and fuzz agents in parallel, consolidates findings, performs cross-domain analysis, and generates the final report."
user-invokable: true
disable-model-invocation: true
argument-hint: ""
---

# Audit Pipeline — Start

Launch the full agent pipeline for the current codebase.

## Pre-flight Checks

Before starting the pipeline:

1. **Verify the project compiles:**
   ```bash
   forge build
   ```
   If it fails, stop and report. The pipeline cannot run on broken code.

2. **Check for `src/` contracts:**
   Use Glob to find `src/**/*.sol`. If no Solidity files exist, stop — there's nothing to audit.

3. **Check for `@audit` tags** (optional but recommended):
   Use Grep to find `@audit-` tags in `src/**/*.sol`. Report how many tags were found and of which types. If zero tags, warn the auditor that the pipeline will run in autonomous mode with lower confidence — but proceed.

4. **Create analysis directory:**
   ```bash
   mkdir -p analysis/findings
   ```

## Phase 0.5 — Static Analysis

Run Slither and Aderyn via the static-analysis MCP server:

1. Call `sa_run` with the project root to run both tools and produce normalized output
2. Verify `analysis/static-analysis.json` was created
3. Report: number of findings by severity (high/medium), any tool failures

If static analysis fails (tools not available, compilation issues), warn the auditor but continue — the pipeline can run without it.

## Phase 1 — Architect Pass 1

Launch the `architect` agent with this context:

> Profile the codebase. Parse all @audit tags from source files. Ingest static analysis output from analysis/static-analysis.json. Build the codebase profile, decide which specialists to launch, and define fuzz targets. Write output to analysis/codebase-profile.json.

Wait for it to complete. Then read `analysis/codebase-profile.json` to get:
- The list of specialists to launch (names + domain scopes + file assignments)
- Fuzz targets (contracts, entry points, invariants to test)

Report to auditor: what the Architect found, which specialists it's launching, how many fuzz targets.

## Phase 1.5 — Maturity Assessment

Launch the `maturity` agent:

> Assess code maturity across 9 quality categories. Read all source files in src/ and test files in test/. Use analysis/codebase-profile.json for the contract list. Write output to analysis/maturity-assessment.json.

Wait for it to complete. Report: overall score, any weak areas (score <= 2.0).

Then read `analysis/maturity-assessment.json` to extract `weak_areas` — these will be passed to specialists in Phase 2.

## Phase 2 — Parallel Execution

Launch ALL of the following in parallel using the Agent tool:

### Specialists (from Architect's plan)
For each specialist in the codebase profile's `specialist_plan`:
- Launch the `specialist` agent with the domain assignment, scoped files, relevant @audit-attention tags from the profile, AND `maturity_weak_areas` relevant to that specialist's scoped files

### Heuristics
- Launch the `heuristics` agent — it always runs regardless of what specialists are launched

### Fuzz Engineer
- Launch the `fuzz-engineer` agent with the fuzz targets from the codebase profile

### Storage Safety
- Launch the `storage-safety` agent — it always runs. Pass `analysis/codebase-profile.json` context for proxy/upgrade patterns.

**All of these run in parallel.** Do not wait for one before launching others.

Wait for all parallel agents to complete. Report progress as each finishes.

## Phase 3 — Consolidation

Launch the `consolidator` agent:

> Read all specialist findings from analysis/findings/, fuzz results from analysis/fuzz-results.json, static analysis from analysis/static-analysis.json, and the codebase profile from analysis/codebase-profile.json. Deduplicate, route fuzz violations, normalize severity, and map findings to @audit tags. Write output to analysis/consolidated-findings.json.

Wait for completion. Report: total findings by severity, dedup stats, fuzz routing summary.

## Phase 3.5 — Falsification

Launch the `falsification` agent:

> Challenge every finding in analysis/consolidated-findings.json. For each finding, trace the exploit path through the actual source code, check for existing mitigations, and attempt to construct a counterargument. Assign verdict: survived, weakened, or falsified. Write output to analysis/falsification-results.json.

Wait for completion. Report: how many findings survived, weakened, falsified.

## Phase 4 — Architect Pass 2

Launch the `architect-p2` agent:

> Perform cross-domain intersection analysis. Read the codebase profile, consolidated findings, falsification results, and fuzz results. Deprioritize falsified findings. Look for bugs that emerge from domain interactions, multi-step attack chains, and blind spots. Write output to analysis/intersection-analysis.json.

Wait for completion. Report: any intersection findings discovered, blind spots identified.

## Phase 4.5 — Triage

Launch the `triage` agent:

> Re-evaluate severity of every finding with full pipeline context. Read consolidated findings, falsification results, intersection analysis, and maturity assessment. Assign final severity and exploitability rating. Write output to analysis/triage-results.json.

Wait for completion. Report: severity changes (upgrades, downgrades, confirmed, false positives).

## Phase 5 — Report Generation

Launch the `report-generator` agent:

> Generate the final audit report from all pipeline outputs. Read codebase profile, consolidated findings, falsification results, triage results, intersection analysis, maturity assessment, fuzz results, and static analysis. Use triage final severity for finding ordering. Write the report to analysis/final-report.md.

Wait for completion.

## Done

Report to the auditor:
1. Pipeline completed successfully (or with warnings if any phase had issues)
2. Summary: total findings by **triage final severity** across all sources
3. Maturity score: overall and any weak areas
4. Falsification: how many survived / weakened / falsified
5. Key output files:
   - `analysis/final-report.md` — the full report
   - `analysis/triage-results.json` — final severity assignments
   - `analysis/consolidated-findings.json` — structured findings data
   - `analysis/maturity-assessment.json` — code quality scorecard
   - `analysis/falsification-results.json` — challenge verdicts
   - `analysis/codebase-profile.json` — codebase map
6. Next step: review the report, then annotate source files with post-pipeline `@audit` tags and run `/audit-review`

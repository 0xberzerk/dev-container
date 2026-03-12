---
name: audit-review
description: "Process the auditor's post-pipeline @audit tags (confirmed, false-positive, discuss, escalate) and enter a feedback dialogue. Routes discussions, suggests KB updates, and surfaces escalations."
user-invokable: true
disable-model-invocation: true
argument-hint: ""
---

# Audit Review — Process Feedback

Process the auditor's post-pipeline `@audit` tags and enter the feedback loop.

## Prerequisites

Verify that the pipeline has run by checking for these files:
- `analysis/consolidated-findings.json`
- `analysis/final-report.md`

If they don't exist, tell the auditor to run `/audit-start` first.

## Phase 1 — Parse Feedback

Launch the `feedback` agent:

> Scan all source files for post-pipeline @audit tags (@audit-confirmed, @audit-false-positive, @audit-discuss, @audit-escalate). Match each tag to pipeline findings from analysis/consolidated-findings.json and analysis/intersection-analysis.json. Produce analysis/feedback-summary.json with categorized responses and KB update suggestions.

Wait for completion. Read `analysis/feedback-summary.json`.

## Phase 2 — Report Summary

Present the feedback summary to the auditor:

### Confirmed Findings
List each `@audit-confirmed` tag with the linked finding (severity, title, location). These are ready for POC writing via `/poc`.

### False Positives
List each `@audit-false-positive` with the auditor's reason and the linked finding. These will be suggested as `noise` for the Knowledge Base.

### Escalations
List each `@audit-escalate` with the auditor's reason and the linked finding. Flag what action the auditor might want to take:
- Re-run a specific specialist with expanded scope
- Re-run Architect P2 with the escalated finding as a focal point
- Write a POC immediately via `/poc`

### Discussion Items
List each `@audit-discuss` tag with the auditor's question and the linked finding (if any).

### Unmatched Tags
List any tags that couldn't be matched to a pipeline finding — these may be new concerns the auditor identified during review.

## Phase 3 — Knowledge Base Suggestions

If the feedback summary contains `kb_feedback` suggestions:

1. Present the suggested KB updates:
   - Findings to mark as `useful` (from confirmed tags)
   - Findings to mark as `noise` (from false-positive tags)
   - Findings to mark as `critical` (from escalate tags)
2. Ask the auditor if they want to apply these updates
3. If yes, call `kb_apply_feedback` with the auditor-approved updates

Do NOT auto-apply KB changes. Always confirm with the auditor first.

## Phase 4 — Discussion Mode

For each `@audit-discuss` tag, ask the auditor if they want to dive into it now.

When the auditor picks a discussion item, launch the `discuss` agent with:

> The auditor wants to discuss: "[exact tag text]"
> Location: [file:line]
> Linked finding: [finding title and ID, if matched]
> Question: [auditor's specific concern]
>
> Analyze the concern, trace the code paths, check pipeline context, and present your verdict with evidence. Be specific with file:line references.

After the discuss agent returns, present its analysis to the auditor. The auditor may:
- Accept the analysis → the tag effectively becomes `@audit-confirmed` or `@audit-false-positive`
- Ask follow-up questions → continue the discussion (re-launch discuss agent with updated context)
- Move on to the next discussion item

Repeat for each discussion item the auditor wants to explore.

## Done

After all feedback is processed:
1. Summarize the final state: how many confirmed, how many false-positives, how many escalated, how many discussed (and outcomes)
2. Remind the auditor of next steps:
   - `/poc` for confirmed findings that need exploit tests
   - Re-run specific pipeline stages for escalated findings (manual — launch the relevant agent directly)
   - KB updates applied (or pending if the auditor deferred)

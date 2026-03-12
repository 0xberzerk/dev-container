use crate::types::*;
use sha2::{Digest, Sha256};

/// Parse Slither JSON output and extract findings that pass severity filter.
pub fn parse_slither(raw: &str) -> Result<(Vec<UnifiedFinding>, ToolRunStatus), String> {
    let output: SlitherOutput =
        serde_json::from_str(raw).map_err(|e| format!("Failed to parse Slither JSON: {e}"))?;

    if !output.success {
        return Ok((
            vec![],
            ToolRunStatus {
                ran: true,
                success: false,
                raw_findings: 0,
                kept_findings: 0,
                error: output.error,
            },
        ));
    }

    let detectors = output
        .results
        .and_then(|r| r.detectors)
        .unwrap_or_default();

    let raw_count = detectors.len();
    let mut findings = Vec::new();

    for det in &detectors {
        let severity = match map_slither_severity(&det.impact) {
            Some(s) => s,
            None => continue, // Dropped: Low, Informational, Optimization
        };

        let confidence = map_slither_confidence(&det.confidence);

        // Extract locations from elements (skip dependencies).
        let locations = extract_slither_locations(&det.elements);

        // Build title from description (first sentence or first 120 chars).
        let title = build_title(&det.description);

        let id = compute_finding_id("slither", &det.check, &locations);

        findings.push(UnifiedFinding {
            id,
            tools: vec![ToolSource {
                tool: AnalysisTool::Slither,
                detector: det.check.clone(),
            }],
            detector: det.check.clone(),
            severity,
            confidence: Some(confidence),
            title,
            description: det.description.clone(),
            locations,
            hint: None,
        });
    }

    let kept = findings.len();
    Ok((
        findings,
        ToolRunStatus {
            ran: true,
            success: true,
            raw_findings: raw_count,
            kept_findings: kept,
            error: None,
        },
    ))
}

/// Parse Aderyn JSON output and extract findings that pass severity filter.
pub fn parse_aderyn(raw: &str) -> Result<(Vec<UnifiedFinding>, ToolRunStatus), String> {
    let output: AderynOutput =
        serde_json::from_str(raw).map_err(|e| format!("Failed to parse Aderyn JSON: {e}"))?;

    let high_issues = output.high_issues.map(|g| g.issues).unwrap_or_default();
    let low_issues = output.low_issues.map(|g| g.issues).unwrap_or_default();
    let raw_count = high_issues.len() + low_issues.len();

    let mut findings = Vec::new();

    // Only high severity passes the filter. Aderyn "low" maps to low/informational — dropped.
    for issue in &high_issues {
        let detector = issue
            .detector_name
            .clone()
            .unwrap_or_else(|| "unknown".to_string());

        // Each issue can have multiple instances (locations).
        let locations = extract_aderyn_locations(&issue.instances);

        // Collect hints from instances.
        let hint = issue
            .instances
            .iter()
            .filter_map(|i| i.hint.as_deref())
            .next()
            .map(String::from);

        let id = compute_finding_id("aderyn", &detector, &locations);

        findings.push(UnifiedFinding {
            id,
            tools: vec![ToolSource {
                tool: AnalysisTool::Aderyn,
                detector: detector.clone(),
            }],
            detector,
            severity: Severity::High,
            confidence: None, // Aderyn doesn't provide confidence.
            title: issue.title.clone(),
            description: issue.description.clone(),
            locations,
            hint,
        });
    }

    let kept = findings.len();
    Ok((
        findings,
        ToolRunStatus {
            ran: true,
            success: true,
            raw_findings: raw_count,
            kept_findings: kept,
            error: None,
        },
    ))
}

/// Merge findings from both tools, deduplicating overlapping detections.
///
/// Dedup rule: two findings are duplicates if they share at least one overlapping
/// location (same file, overlapping line ranges) AND the same severity.
/// When merged, both tool sources are preserved.
pub fn merge_and_dedup(
    slither: Vec<UnifiedFinding>,
    aderyn: Vec<UnifiedFinding>,
) -> Vec<UnifiedFinding> {
    let mut merged: Vec<UnifiedFinding> = slither;

    for af in aderyn {
        if let Some(existing) = merged.iter_mut().find(|sf| is_duplicate(sf, &af)) {
            // Merge: add the aderyn tool source to the existing finding.
            for ts in &af.tools {
                if !existing.tools.iter().any(|t| t.tool == ts.tool) {
                    existing.tools.push(ts.clone());
                }
            }
            // Prefer aderyn hint if slither didn't have one.
            if existing.hint.is_none() && af.hint.is_some() {
                existing.hint = af.hint;
            }
        } else {
            merged.push(af);
        }
    }

    // Sort by severity (high first), then by first file location.
    merged.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then_with(|| {
                let a_loc = a.locations.first().map(|l| (&l.file, l.start_line));
                let b_loc = b.locations.first().map(|l| (&l.file, l.start_line));
                a_loc.cmp(&b_loc)
            })
    });

    merged
}

/// Build the full report.
pub fn build_report(
    findings: Vec<UnifiedFinding>,
    project_path: &str,
    slither_status: ToolRunStatus,
    aderyn_status: ToolRunStatus,
) -> StaticAnalysisReport {
    let by_severity = SeverityCounts {
        high: findings.iter().filter(|f| f.severity == Severity::High).count(),
        medium: findings.iter().filter(|f| f.severity == Severity::Medium).count(),
    };

    StaticAnalysisReport {
        metadata: ReportMetadata {
            timestamp: chrono::Utc::now().to_rfc3339(),
            project_path: project_path.to_string(),
            slither: slither_status,
            aderyn: aderyn_status,
            total_findings: findings.len(),
            by_severity,
        },
        findings,
    }
}

// ─── Helpers ──────────────────────────────────────────────────────

fn map_slither_severity(impact: &str) -> Option<Severity> {
    match impact {
        "High" => Some(Severity::High),
        "Medium" => Some(Severity::Medium),
        _ => None, // Low, Informational, Optimization — dropped.
    }
}

fn map_slither_confidence(conf: &str) -> Confidence {
    match conf {
        "High" => Confidence::High,
        "Medium" => Confidence::Medium,
        _ => Confidence::Low,
    }
}

fn extract_slither_locations(elements: &[SlitherElement]) -> Vec<Location> {
    let mut locations = Vec::new();
    for el in elements {
        if let Some(sm) = &el.source_mapping {
            // Skip dependency files.
            if sm.is_dependency {
                continue;
            }
            if let Some(file) = &sm.filename_relative {
                if file.is_empty() {
                    continue;
                }
                let start_line = sm.lines.iter().copied().min().unwrap_or(0);
                let end_line = sm.lines.iter().copied().max().unwrap_or(start_line);
                if start_line == 0 {
                    continue;
                }
                locations.push(Location {
                    file: file.clone(),
                    start_line,
                    end_line,
                    start_column: sm.starting_column,
                    end_column: sm.ending_column,
                });
            }
        }
    }
    // Dedup locations (same file + same lines).
    locations.sort_by(|a, b| (&a.file, a.start_line).cmp(&(&b.file, b.start_line)));
    locations.dedup_by(|a, b| a.file == b.file && a.start_line == b.start_line && a.end_line == b.end_line);
    locations
}

fn extract_aderyn_locations(instances: &[AderynInstance]) -> Vec<Location> {
    let mut locations = Vec::new();
    for inst in instances {
        if let (Some(file), Some(line)) = (&inst.contract_path, inst.line_no) {
            if !file.is_empty() && line > 0 {
                locations.push(Location {
                    file: file.clone(),
                    start_line: line,
                    end_line: line, // Aderyn only provides a single line.
                    start_column: None,
                    end_column: None,
                });
            }
        }
    }
    locations.sort_by(|a, b| (&a.file, a.start_line).cmp(&(&b.file, b.start_line)));
    locations.dedup_by(|a, b| a.file == b.file && a.start_line == b.start_line);
    locations
}

fn build_title(description: &str) -> String {
    // Take the first sentence or first 120 chars.
    let trimmed = description.trim();
    if let Some(pos) = trimmed.find(". ") {
        let sentence = &trimmed[..pos];
        if sentence.len() <= 150 {
            return sentence.to_string();
        }
    }
    if trimmed.len() <= 150 {
        trimmed.to_string()
    } else {
        format!("{}...", &trimmed[..147])
    }
}

fn compute_finding_id(tool: &str, detector: &str, locations: &[Location]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(tool.as_bytes());
    hasher.update(detector.as_bytes());
    for loc in locations {
        hasher.update(loc.file.as_bytes());
        hasher.update(loc.start_line.to_le_bytes());
        hasher.update(loc.end_line.to_le_bytes());
    }
    let hash = hasher.finalize();
    format!("{:x}", hash)
}

/// Two findings are duplicates if they have the same severity and at least one
/// overlapping location (same file, overlapping line ranges).
fn is_duplicate(a: &UnifiedFinding, b: &UnifiedFinding) -> bool {
    if a.severity != b.severity {
        return false;
    }
    for al in &a.locations {
        for bl in &b.locations {
            if al.file == bl.file && lines_overlap(al.start_line, al.end_line, bl.start_line, bl.end_line) {
                return true;
            }
        }
    }
    false
}

fn lines_overlap(a_start: usize, a_end: usize, b_start: usize, b_end: usize) -> bool {
    a_start <= b_end && b_start <= a_end
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Slither parsing ──────────────────────────────────────────

    #[test]
    fn parse_slither_filters_severity() {
        let json = r#"{
            "success": true,
            "error": null,
            "results": {
                "detectors": [
                    {
                        "check": "reentrancy-eth",
                        "impact": "High",
                        "confidence": "Medium",
                        "description": "Reentrancy in Contract.withdraw(uint256)",
                        "id": "abc123",
                        "elements": [{
                            "type": "function",
                            "name": "withdraw",
                            "source_mapping": {
                                "filename_relative": "src/Contract.sol",
                                "lines": [45, 46, 47],
                                "starting_column": 5,
                                "ending_column": 6,
                                "is_dependency": false
                            }
                        }]
                    },
                    {
                        "check": "solc-version",
                        "impact": "Informational",
                        "confidence": "High",
                        "description": "Pragma version 0.8.30 is too recent",
                        "id": "def456",
                        "elements": []
                    },
                    {
                        "check": "uninitialized-local",
                        "impact": "Medium",
                        "confidence": "Medium",
                        "description": "Uninitialized local variable x in foo()",
                        "id": "ghi789",
                        "elements": [{
                            "type": "variable",
                            "name": "x",
                            "source_mapping": {
                                "filename_relative": "src/Foo.sol",
                                "lines": [10],
                                "is_dependency": false
                            }
                        }]
                    },
                    {
                        "check": "constable-states",
                        "impact": "Optimization",
                        "confidence": "High",
                        "description": "State variable should be constant",
                        "id": "jkl012",
                        "elements": []
                    },
                    {
                        "check": "missing-zero-check",
                        "impact": "Low",
                        "confidence": "Medium",
                        "description": "Missing zero address check",
                        "id": "mno345",
                        "elements": []
                    }
                ]
            }
        }"#;

        let (findings, status) = parse_slither(json).unwrap();
        assert!(status.success);
        assert_eq!(status.raw_findings, 5);
        assert_eq!(status.kept_findings, 2);
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].severity, Severity::High);
        assert_eq!(findings[0].detector, "reentrancy-eth");
        assert_eq!(findings[1].severity, Severity::Medium);
        assert_eq!(findings[1].detector, "uninitialized-local");
    }

    #[test]
    fn parse_slither_failure() {
        let json = r#"{
            "success": false,
            "error": "Compilation failed",
            "results": null
        }"#;
        let (findings, status) = parse_slither(json).unwrap();
        assert!(!status.success);
        assert_eq!(findings.len(), 0);
        assert_eq!(status.error.as_deref(), Some("Compilation failed"));
    }

    #[test]
    fn parse_slither_skips_dependencies() {
        let json = r#"{
            "success": true,
            "error": null,
            "results": {
                "detectors": [{
                    "check": "reentrancy-eth",
                    "impact": "High",
                    "confidence": "High",
                    "description": "Reentrancy in dependency",
                    "id": "dep1",
                    "elements": [{
                        "type": "function",
                        "name": "foo",
                        "source_mapping": {
                            "filename_relative": "lib/openzeppelin/ERC20.sol",
                            "lines": [100],
                            "is_dependency": true
                        }
                    }]
                }]
            }
        }"#;
        let (findings, _) = parse_slither(json).unwrap();
        assert_eq!(findings.len(), 1);
        // Finding exists but with no locations (dependency was skipped).
        assert!(findings[0].locations.is_empty());
    }

    #[test]
    fn parse_slither_empty_detectors() {
        let json = r#"{
            "success": true,
            "error": null,
            "results": { "detectors": [] }
        }"#;
        let (findings, status) = parse_slither(json).unwrap();
        assert!(status.success);
        assert_eq!(findings.len(), 0);
        assert_eq!(status.raw_findings, 0);
    }

    // ─── Aderyn parsing ───────────────────────────────────────────

    #[test]
    fn parse_aderyn_keeps_high_only() {
        let json = r#"{
            "files_summary": { "total_source_units": 5, "total_sloc": 200 },
            "files_details": { "files_details": [] },
            "issue_count": { "high": 1, "low": 2 },
            "high_issues": {
                "issues": [{
                    "title": "Reentrancy vulnerability",
                    "description": "State change after external call",
                    "detector_name": "reentrancy-state-change",
                    "instances": [{
                        "contract_path": "src/Contract.sol",
                        "line_no": 45,
                        "src": "1234:58",
                        "src_char": "1100:42"
                    }]
                }]
            },
            "low_issues": {
                "issues": [
                    {
                        "title": "Missing zero check",
                        "description": "No zero address validation",
                        "detector_name": "missing-zero-check",
                        "instances": [{ "contract_path": "src/Foo.sol", "line_no": 10 }]
                    },
                    {
                        "title": "Unused return",
                        "description": "Return value ignored",
                        "detector_name": "unused-return",
                        "instances": [{ "contract_path": "src/Bar.sol", "line_no": 20 }]
                    }
                ]
            },
            "detectors_used": ["reentrancy-state-change", "missing-zero-check", "unused-return"]
        }"#;

        let (findings, status) = parse_aderyn(json).unwrap();
        assert!(status.success);
        assert_eq!(status.raw_findings, 3);
        assert_eq!(status.kept_findings, 1);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::High);
        assert_eq!(findings[0].title, "Reentrancy vulnerability");
    }

    #[test]
    fn parse_aderyn_no_high_issues() {
        let json = r#"{
            "files_summary": { "total_source_units": 1, "total_sloc": 50 },
            "issue_count": { "high": 0, "low": 1 },
            "high_issues": { "issues": [] },
            "low_issues": {
                "issues": [{
                    "title": "Low issue",
                    "description": "Low desc",
                    "detector_name": "some-low",
                    "instances": []
                }]
            },
            "detectors_used": []
        }"#;
        let (findings, status) = parse_aderyn(json).unwrap();
        assert_eq!(findings.len(), 0);
        assert_eq!(status.raw_findings, 1);
        assert_eq!(status.kept_findings, 0);
    }

    #[test]
    fn parse_aderyn_preserves_hints() {
        let json = r#"{
            "issue_count": { "high": 1, "low": 0 },
            "high_issues": {
                "issues": [{
                    "title": "Issue with hint",
                    "description": "Desc",
                    "detector_name": "det",
                    "instances": [{
                        "contract_path": "src/A.sol",
                        "line_no": 5,
                        "hint": "Use ReentrancyGuard"
                    }]
                }]
            },
            "low_issues": { "issues": [] },
            "detectors_used": []
        }"#;
        let (findings, _) = parse_aderyn(json).unwrap();
        assert_eq!(findings[0].hint.as_deref(), Some("Use ReentrancyGuard"));
    }

    // ─── Dedup & merge ────────────────────────────────────────────

    #[test]
    fn merge_deduplicates_overlapping() {
        let slither = vec![UnifiedFinding {
            id: "s1".into(),
            tools: vec![ToolSource {
                tool: AnalysisTool::Slither,
                detector: "reentrancy-eth".into(),
            }],
            detector: "reentrancy-eth".into(),
            severity: Severity::High,
            confidence: Some(Confidence::Medium),
            title: "Reentrancy in withdraw".into(),
            description: "Slither desc".into(),
            locations: vec![Location {
                file: "src/Contract.sol".into(),
                start_line: 45,
                end_line: 47,
                start_column: Some(5),
                end_column: Some(6),
            }],
            hint: None,
        }];

        let aderyn = vec![UnifiedFinding {
            id: "a1".into(),
            tools: vec![ToolSource {
                tool: AnalysisTool::Aderyn,
                detector: "reentrancy-state-change".into(),
            }],
            detector: "reentrancy-state-change".into(),
            severity: Severity::High,
            confidence: None,
            title: "Reentrancy vulnerability".into(),
            description: "Aderyn desc".into(),
            locations: vec![Location {
                file: "src/Contract.sol".into(),
                start_line: 45,
                end_line: 45,
                start_column: None,
                end_column: None,
            }],
            hint: Some("Use ReentrancyGuard".into()),
        }];

        let merged = merge_and_dedup(slither, aderyn);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].tools.len(), 2);
        assert_eq!(merged[0].hint.as_deref(), Some("Use ReentrancyGuard"));
    }

    #[test]
    fn merge_keeps_different_files_separate() {
        let slither = vec![UnifiedFinding {
            id: "s1".into(),
            tools: vec![ToolSource {
                tool: AnalysisTool::Slither,
                detector: "reentrancy-eth".into(),
            }],
            detector: "reentrancy-eth".into(),
            severity: Severity::High,
            confidence: Some(Confidence::High),
            title: "Reentrancy A".into(),
            description: "Desc A".into(),
            locations: vec![Location {
                file: "src/A.sol".into(),
                start_line: 10,
                end_line: 15,
                start_column: None,
                end_column: None,
            }],
            hint: None,
        }];

        let aderyn = vec![UnifiedFinding {
            id: "a1".into(),
            tools: vec![ToolSource {
                tool: AnalysisTool::Aderyn,
                detector: "reentrancy-state-change".into(),
            }],
            detector: "reentrancy-state-change".into(),
            severity: Severity::High,
            confidence: None,
            title: "Reentrancy B".into(),
            description: "Desc B".into(),
            locations: vec![Location {
                file: "src/B.sol".into(),
                start_line: 20,
                end_line: 20,
                start_column: None,
                end_column: None,
            }],
            hint: None,
        }];

        let merged = merge_and_dedup(slither, aderyn);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn merge_keeps_different_severity_separate() {
        let f1 = UnifiedFinding {
            id: "s1".into(),
            tools: vec![ToolSource {
                tool: AnalysisTool::Slither,
                detector: "det".into(),
            }],
            detector: "det".into(),
            severity: Severity::High,
            confidence: None,
            title: "High".into(),
            description: "".into(),
            locations: vec![Location {
                file: "src/A.sol".into(),
                start_line: 10,
                end_line: 10,
                start_column: None,
                end_column: None,
            }],
            hint: None,
        };

        let f2 = UnifiedFinding {
            id: "s2".into(),
            tools: vec![ToolSource {
                tool: AnalysisTool::Slither,
                detector: "det2".into(),
            }],
            detector: "det2".into(),
            severity: Severity::Medium,
            confidence: None,
            title: "Medium".into(),
            description: "".into(),
            locations: vec![Location {
                file: "src/A.sol".into(),
                start_line: 10,
                end_line: 10,
                start_column: None,
                end_column: None,
            }],
            hint: None,
        };

        let merged = merge_and_dedup(vec![f1], vec![f2]);
        assert_eq!(merged.len(), 2);
    }

    // ─── Helper tests ─────────────────────────────────────────────

    #[test]
    fn title_truncation() {
        let short = "Short description. More text.";
        assert_eq!(build_title(short), "Short description");

        let no_period = "No period here";
        assert_eq!(build_title(no_period), "No period here");

        let long = "A".repeat(200);
        let title = build_title(&long);
        assert!(title.len() <= 150);
        assert!(title.ends_with("..."));
    }

    #[test]
    fn lines_overlap_cases() {
        assert!(lines_overlap(1, 5, 3, 8)); // Partial overlap
        assert!(lines_overlap(1, 10, 5, 5)); // Contained
        assert!(lines_overlap(5, 5, 1, 10)); // Contained (reverse)
        assert!(lines_overlap(1, 5, 5, 10)); // Edge overlap
        assert!(!lines_overlap(1, 4, 5, 10)); // No overlap
        assert!(!lines_overlap(10, 20, 1, 5)); // No overlap (reverse)
    }

    #[test]
    fn finding_id_is_deterministic() {
        let locs = vec![Location {
            file: "src/A.sol".into(),
            start_line: 10,
            end_line: 15,
            start_column: None,
            end_column: None,
        }];
        let id1 = compute_finding_id("slither", "reentrancy-eth", &locs);
        let id2 = compute_finding_id("slither", "reentrancy-eth", &locs);
        assert_eq!(id1, id2);

        let id3 = compute_finding_id("aderyn", "reentrancy-eth", &locs);
        assert_ne!(id1, id3);
    }

    #[test]
    fn build_report_counts() {
        let findings = vec![
            UnifiedFinding {
                id: "1".into(),
                tools: vec![],
                detector: "d".into(),
                severity: Severity::High,
                confidence: None,
                title: "H".into(),
                description: "".into(),
                locations: vec![],
                hint: None,
            },
            UnifiedFinding {
                id: "2".into(),
                tools: vec![],
                detector: "d".into(),
                severity: Severity::Medium,
                confidence: None,
                title: "M1".into(),
                description: "".into(),
                locations: vec![],
                hint: None,
            },
            UnifiedFinding {
                id: "3".into(),
                tools: vec![],
                detector: "d".into(),
                severity: Severity::Medium,
                confidence: None,
                title: "M2".into(),
                description: "".into(),
                locations: vec![],
                hint: None,
            },
        ];

        let slither_status = ToolRunStatus {
            ran: true,
            success: true,
            raw_findings: 10,
            kept_findings: 2,
            error: None,
        };
        let aderyn_status = ToolRunStatus {
            ran: true,
            success: true,
            raw_findings: 5,
            kept_findings: 1,
            error: None,
        };

        let report = build_report(findings, "/project", slither_status, aderyn_status);
        assert_eq!(report.metadata.total_findings, 3);
        assert_eq!(report.metadata.by_severity.high, 1);
        assert_eq!(report.metadata.by_severity.medium, 2);
        assert!(report.metadata.slither.success);
        assert!(report.metadata.aderyn.success);
    }
}

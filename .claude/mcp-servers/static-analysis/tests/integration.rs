mod normalize_flow {

    /// Simulates the full flow: parse both tool outputs, merge, write, read back.
    #[tokio::test]
    async fn full_normalize_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let analysis_dir = dir.path().join("analysis");
        std::fs::create_dir_all(&analysis_dir).unwrap();

        let slither_json = r#"{
            "success": true,
            "error": null,
            "results": {
                "detectors": [
                    {
                        "check": "reentrancy-eth",
                        "impact": "High",
                        "confidence": "Medium",
                        "description": "Reentrancy in Vault.withdraw(uint256). External call sends ETH before state update.",
                        "id": "abc123",
                        "elements": [{
                            "type": "function",
                            "name": "withdraw",
                            "source_mapping": {
                                "filename_relative": "src/Vault.sol",
                                "lines": [45, 46, 47, 48, 49, 50],
                                "starting_column": 5,
                                "ending_column": 6,
                                "is_dependency": false
                            }
                        }]
                    },
                    {
                        "check": "divide-before-multiply",
                        "impact": "Medium",
                        "confidence": "Medium",
                        "description": "Division before multiplication in Vault.previewRedeem(uint256). Precision loss possible.",
                        "id": "def456",
                        "elements": [{
                            "type": "node",
                            "name": "shares * totalAssets() / totalSupply()",
                            "source_mapping": {
                                "filename_relative": "src/Vault.sol",
                                "lines": [82],
                                "starting_column": 16,
                                "ending_column": 55,
                                "is_dependency": false
                            }
                        }]
                    },
                    {
                        "check": "solc-version",
                        "impact": "Informational",
                        "confidence": "High",
                        "description": "Pragma version 0.8.30",
                        "id": "ghi789",
                        "elements": []
                    },
                    {
                        "check": "constable-states",
                        "impact": "Optimization",
                        "confidence": "High",
                        "description": "MAX_FEE should be constant",
                        "id": "jkl012",
                        "elements": []
                    }
                ]
            }
        }"#;

        let aderyn_json = r#"{
            "files_summary": { "total_source_units": 3, "total_sloc": 150 },
            "issue_count": { "high": 1, "low": 2 },
            "high_issues": {
                "issues": [{
                    "title": "State change after external call in Vault.withdraw",
                    "description": "The function performs a state change after an external call, which may be vulnerable to reentrancy.",
                    "detector_name": "reentrancy-state-change",
                    "instances": [{
                        "contract_path": "src/Vault.sol",
                        "line_no": 47,
                        "src": "2048:120",
                        "hint": "Consider using ReentrancyGuard or checks-effects-interactions pattern"
                    }]
                }]
            },
            "low_issues": {
                "issues": [
                    {
                        "title": "Missing zero address check",
                        "description": "No validation for zero address",
                        "detector_name": "missing-zero-check",
                        "instances": [{ "contract_path": "src/Vault.sol", "line_no": 15 }]
                    },
                    {
                        "title": "Unused return value",
                        "description": "Return value of external call not checked",
                        "detector_name": "unused-return",
                        "instances": [{ "contract_path": "src/Vault.sol", "line_no": 60 }]
                    }
                ]
            },
            "detectors_used": ["reentrancy-state-change", "missing-zero-check", "unused-return"]
        }"#;

        // Write raw tool outputs to simulate what the runner would produce.
        std::fs::write(analysis_dir.join("slither.json"), slither_json).unwrap();
        std::fs::write(analysis_dir.join("aderyn.json"), aderyn_json).unwrap();

        // Parse both.
        let (slither_findings, slither_status) =
            static_analysis::normalize::parse_slither(slither_json).unwrap();
        let (aderyn_findings, aderyn_status) =
            static_analysis::normalize::parse_aderyn(aderyn_json).unwrap();

        // Verify severity filtering.
        assert!(slither_status.success);
        assert_eq!(slither_status.raw_findings, 4);
        assert_eq!(slither_status.kept_findings, 2); // High + Medium only.
        assert_eq!(slither_findings.len(), 2);

        assert!(aderyn_status.success);
        assert_eq!(aderyn_status.raw_findings, 3);
        assert_eq!(aderyn_status.kept_findings, 1); // Only high.
        assert_eq!(aderyn_findings.len(), 1);

        // Merge and dedup — the reentrancy finding should be deduplicated.
        let merged =
            static_analysis::normalize::merge_and_dedup(slither_findings, aderyn_findings);

        // Reentrancy from both tools should merge (same file, overlapping lines, same severity).
        // Precision loss is a separate medium finding.
        assert_eq!(merged.len(), 2, "Expected 2 after dedup: reentrancy (merged) + precision loss");

        let reentrancy = merged.iter().find(|f| f.detector == "reentrancy-eth").unwrap();
        assert_eq!(reentrancy.tools.len(), 2, "Reentrancy should have both tool sources");
        assert_eq!(
            reentrancy.hint.as_deref(),
            Some("Consider using ReentrancyGuard or checks-effects-interactions pattern")
        );

        // Build report and write to disk.
        let report = static_analysis::normalize::build_report(
            merged,
            dir.path().to_str().unwrap(),
            slither_status,
            aderyn_status,
        );

        assert_eq!(report.metadata.total_findings, 2);
        assert_eq!(report.metadata.by_severity.high, 1);
        assert_eq!(report.metadata.by_severity.medium, 1);

        let report_json = serde_json::to_string_pretty(&report).unwrap();
        static_analysis::runner::write_report(dir.path(), &report_json)
            .await
            .unwrap();

        // Read back and verify.
        let read_back = static_analysis::runner::read_report(dir.path()).await.unwrap();
        let parsed: static_analysis::types::StaticAnalysisReport =
            serde_json::from_str(&read_back).unwrap();

        assert_eq!(parsed.findings.len(), 2);
        assert_eq!(parsed.metadata.total_findings, 2);
    }

    /// Verify that when both tools fail, we get a valid empty report.
    #[tokio::test]
    async fn both_tools_fail_produces_empty_report() {
        let slither_status = static_analysis::types::ToolRunStatus {
            ran: true,
            success: false,
            error: Some("slither not found".into()),
            ..Default::default()
        };
        let aderyn_status = static_analysis::types::ToolRunStatus {
            ran: true,
            success: false,
            error: Some("aderyn not found".into()),
            ..Default::default()
        };

        let report = static_analysis::normalize::build_report(
            vec![],
            "/fake/project",
            slither_status,
            aderyn_status,
        );

        assert_eq!(report.metadata.total_findings, 0);
        assert!(!report.metadata.slither.success);
        assert!(!report.metadata.aderyn.success);

        // Should serialize fine.
        let json = serde_json::to_string_pretty(&report).unwrap();
        assert!(json.contains("\"total_findings\": 0"));
    }
}

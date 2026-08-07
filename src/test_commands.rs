//! Cross-cutting CLI integration tests that don't belong to a single command
//! module: read-only guard detection, top-level CLI shape, and multi-domain
//! clap parsing checks.
//!
//! Most per-command tests have been colocated with their modules in
//! `#[cfg(test)] mod tests { ... }` blocks. Shared helpers live in
//! `crate::test_support`.

use clap::CommandFactory;

// -------------------------------------------------------------------------
// Read-only mode
// -------------------------------------------------------------------------

#[test]
fn test_is_write_command_name_writes() {
    assert!(crate::is_write_command_name("delete"));
    assert!(crate::is_write_command_name("create"));
    assert!(crate::is_write_command_name("update"));
    assert!(crate::is_write_command_name("cancel"));
    assert!(crate::is_write_command_name("trigger"));
    assert!(crate::is_write_command_name("submit"));
    assert!(crate::is_write_command_name("send"));
    assert!(crate::is_write_command_name("move"));
    assert!(crate::is_write_command_name("link"));
    assert!(crate::is_write_command_name("unlink"));
    assert!(crate::is_write_command_name("configure"));
    assert!(crate::is_write_command_name("upgrade"));
    assert!(crate::is_write_command_name("update-status"));
    assert!(crate::is_write_command_name("create-page"));
    assert!(crate::is_write_command_name("patch"));
    assert!(crate::is_write_command_name("patch-deployment"));
    // Mutation verbs added to fix issue #528
    assert!(crate::is_write_command_name("run"));
    assert!(crate::is_write_command_name("enable"));
    assert!(crate::is_write_command_name("disable"));
    assert!(crate::is_write_command_name("edit"));
    assert!(crate::is_write_command_name("upsert"));
    assert!(crate::is_write_command_name("upload"));
    assert!(crate::is_write_command_name("publish"));
    assert!(crate::is_write_command_name("unpublish"));
    assert!(crate::is_write_command_name("comment"));
    assert!(crate::is_write_command_name("start"));
    assert!(crate::is_write_command_name("stop"));
    assert!(crate::is_write_command_name("pause"));
    assert!(crate::is_write_command_name("resume"));
    assert!(crate::is_write_command_name("generate"));
    assert!(crate::is_write_command_name("unassign"));
    assert!(crate::is_write_command_name("batch-create"));
}

#[test]
fn test_is_write_command_name_reads() {
    assert!(!crate::is_write_command_name("list"));
    assert!(!crate::is_write_command_name("get"));
    assert!(!crate::is_write_command_name("search"));
    assert!(!crate::is_write_command_name("query"));
    assert!(!crate::is_write_command_name("aggregate"));
    assert!(!crate::is_write_command_name("status"));
    assert!(!crate::is_write_command_name("dispatch"));
}

#[test]
fn test_read_only_guard_blocks_write() {
    let matches = crate::Cli::command()
        .try_get_matches_from(["pup", "monitors", "delete", "12345"])
        .unwrap();
    let leaf = crate::get_leaf_subcommand_name(&matches).unwrap();
    assert!(crate::is_write_command_name(&leaf));
}

#[test]
fn test_read_only_guard_allows_read() {
    let matches = crate::Cli::command()
        .try_get_matches_from(["pup", "monitors", "list"])
        .unwrap();
    let leaf = crate::get_leaf_subcommand_name(&matches).unwrap();
    assert!(!crate::is_write_command_name(&leaf));
}

#[test]
fn test_read_only_guard_nested_read() {
    let matches = crate::Cli::command()
        .try_get_matches_from(["pup", "rum", "apps", "list"])
        .unwrap();
    let leaf = crate::get_leaf_subcommand_name(&matches).unwrap();
    assert!(!crate::is_write_command_name(&leaf));
}

#[test]
fn test_read_only_guard_on_call_pages_list() {
    let matches = crate::Cli::command()
        .try_get_matches_from([
            "pup",
            "on-call",
            "pages",
            "list",
            "--team",
            "core-platform",
            "--responder",
            "user-1",
            "--sort",
            "-created_at",
        ])
        .unwrap();
    let leaf = crate::get_leaf_subcommand_name(&matches).unwrap();
    assert_eq!(leaf, "list");
    assert!(!crate::is_write_command_name(&leaf));
}

#[test]
fn test_on_call_pages_list_rejects_invalid_page_size() {
    let result = crate::Cli::command().try_get_matches_from([
        "pup",
        "on-call",
        "pages",
        "list",
        "--page-size",
        "0",
    ]);
    assert!(result.is_err());
}

#[test]
fn test_on_call_pages_list_rejects_invalid_sort() {
    let result = crate::Cli::command().try_get_matches_from([
        "pup",
        "on-call",
        "pages",
        "list",
        "--sort",
        "started_at",
    ]);
    assert!(result.is_err());
}

#[test]
fn test_read_only_guard_nested_write() {
    let matches = crate::Cli::command()
        .try_get_matches_from([
            "pup",
            "cases",
            "jira",
            "create-issue",
            "123",
            "--file",
            "f.json",
        ])
        .unwrap();
    let leaf = crate::get_leaf_subcommand_name(&matches).unwrap();
    assert!(crate::is_write_command_name(&leaf));
}

#[test]
fn test_read_only_guard_exempts_alias() {
    let matches = crate::Cli::command()
        .try_get_matches_from(["pup", "alias", "set", "foo", "logs search *"])
        .unwrap();
    let top = crate::get_top_level_subcommand_name(&matches);
    assert_eq!(top.as_deref(), Some("alias"));
}

#[test]
fn test_read_only_guard_exempts_auth() {
    let matches = crate::Cli::command()
        .try_get_matches_from(["pup", "auth", "login"])
        .unwrap();
    let top = crate::get_top_level_subcommand_name(&matches);
    assert_eq!(top.as_deref(), Some("auth"));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn test_read_only_exempts_local_skills_install() {
    // `skills install` writes local files only — must stay exempt from the guard.
    let matches = crate::Cli::command()
        .try_get_matches_from(["pup", "skills", "install", "claude"])
        .unwrap();
    assert!(crate::is_read_only_exempt(&matches));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn test_read_only_blocks_skills_remote_sessions_create() {
    // `skills remote sessions create` writes to the onboarding API, so it must
    // NOT be exempt, and its leaf verb must classify as a write.
    let matches = crate::Cli::command()
        .try_get_matches_from([
            "pup",
            "skills",
            "remote",
            "sessions",
            "create",
            "--session-id",
            "run-1",
            "--skill-id",
            "aws-integration-setup",
            "--summary",
            "s",
            "--status",
            "completed",
        ])
        .unwrap();
    assert!(!crate::is_read_only_exempt(&matches));
    let leaf = crate::get_leaf_subcommand_name(&matches).unwrap();
    assert!(crate::is_write_command_name(&leaf));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn test_read_only_allows_skills_remote_reads() {
    // `skills remote list`/`get` are reads: not exempt, but not write verbs, so
    // the guard lets them through.
    for args in [
        vec!["pup", "skills", "remote", "list"],
        vec!["pup", "skills", "remote", "get", "orchestrator"],
    ] {
        let matches = crate::Cli::command().try_get_matches_from(args).unwrap();
        assert!(!crate::is_read_only_exempt(&matches));
        let leaf = crate::get_leaf_subcommand_name(&matches).unwrap();
        assert!(!crate::is_write_command_name(&leaf));
    }
}

// -------------------------------------------------------------------------
// Auth status --site flag
// -------------------------------------------------------------------------

#[test]
fn test_auth_status_accepts_site_flag() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from(["pup", "auth", "status", "--site", "datadoghq.eu"])
        .expect("auth status --site should parse");

    match cli.command {
        crate::Commands::Auth { action } => match action {
            crate::AuthActions::Status { site } => {
                assert_eq!(site, Some("datadoghq.eu".to_string()));
            }
            _ => panic!("expected AuthActions::Status"),
        },
        _ => panic!("expected Commands::Auth"),
    }
}

#[test]
fn test_auth_status_site_flag_is_optional() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from(["pup", "auth", "status"])
        .expect("auth status without --site should parse");

    match cli.command {
        crate::Commands::Auth { action } => match action {
            crate::AuthActions::Status { site } => {
                assert_eq!(site, None);
            }
            _ => panic!("expected AuthActions::Status"),
        },
        _ => panic!("expected Commands::Auth"),
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn test_extension_install_accepts_remote_extension_selector() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from([
        "pup",
        "extension",
        "install",
        "owner/repo",
        "--extension",
        "foo",
    ])
    .expect("extension install --extension should parse");

    match cli.command {
        crate::Commands::Extension { action } => match action {
            crate::ExtensionActions::Install {
                source, extension, ..
            } => {
                assert_eq!(source, "owner/repo");
                assert_eq!(extension.as_deref(), Some("foo"));
            }
            _ => panic!("expected ExtensionActions::Install"),
        },
        _ => panic!("expected Commands::Extension"),
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn test_extension_install_accepts_all_remote_extensions() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from(["pup", "extension", "install", "owner/repo", "--all"])
        .expect("extension install --all should parse");

    match cli.command {
        crate::Commands::Extension { action } => match action {
            crate::ExtensionActions::Install { all, .. } => {
                assert!(all);
            }
            _ => panic!("expected ExtensionActions::Install"),
        },
        _ => panic!("expected Commands::Extension"),
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn test_extension_install_rejects_remote_extension_with_name_override() {
    use clap::Parser;

    let result = crate::Cli::try_parse_from([
        "pup",
        "extension",
        "install",
        "owner/repo",
        "--extension",
        "foo",
        "--name",
        "bar",
    ]);

    assert!(result.is_err());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn test_extension_install_rejects_all_with_description() {
    use clap::Parser;

    let result = crate::Cli::try_parse_from([
        "pup",
        "extension",
        "install",
        "owner/repo",
        "--all",
        "--description",
        "example",
    ]);

    assert!(result.is_err());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn test_extension_list_remote_parses() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from([
        "pup",
        "extension",
        "list-remote",
        "owner/repo",
        "--extension",
        "foo",
    ])
    .expect("extension list-remote should parse");

    match cli.command {
        crate::Commands::Extension { action } => match action {
            crate::ExtensionActions::ListRemote { source, extension } => {
                assert_eq!(source, "owner/repo");
                assert_eq!(extension.as_deref(), Some("foo"));
            }
            _ => panic!("expected ExtensionActions::ListRemote"),
        },
        _ => panic!("expected Commands::Extension"),
    }
}

#[test]
fn test_top_level_commands_sorted_alphabetically() {
    let app = crate::Cli::command();
    let names: Vec<&str> = app
        .get_subcommands()
        .filter(|cmd| cmd.get_name() != "help" && !cmd.is_hide_set())
        .map(|cmd| cmd.get_name())
        .collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(
        names, sorted,
        "top-level commands must be in alphabetical order.\nActual:   {names:?}\nExpected: {sorted:?}"
    );
}

#[test]
fn test_dbm_samples_search_parses() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from([
        "pup",
        "dbm",
        "samples",
        "search",
        "--query",
        "service:db",
        "--from",
        "1h",
        "--limit",
        "10",
        "--sort",
        "asc",
    ])
    .expect("dbm samples search should parse");

    match cli.command {
        crate::Commands::Dbm { action } => match action {
            crate::DbmActions::Samples { action } => match action {
                crate::DbmSamplesActions::Search {
                    query,
                    from,
                    to,
                    limit,
                    sort,
                } => {
                    assert_eq!(query, "service:db");
                    assert_eq!(from, "1h");
                    assert_eq!(to, "now");
                    assert_eq!(limit, 10);
                    assert_eq!(sort, "asc");
                }
            },
        },
        _ => panic!("expected Commands::Dbm"),
    }
}

#[test]
fn test_ddsql_table_query_accepts_leading_comment() {
    use clap::Parser;

    let query = "-- owner breakdown\nSELECT 1";
    let cli = crate::Cli::try_parse_from(["pup", "ddsql", "table", "--query", query])
        .expect("ddsql table with leading SQL comment should parse");

    match cli.command {
        crate::Commands::Ddsql { action } => match action {
            crate::DdsqlActions::Table { query: parsed, .. } => {
                assert_eq!(parsed, query);
            }
            _ => panic!("expected DdsqlActions::Table"),
        },
        _ => panic!("expected Commands::Ddsql"),
    }
}

#[test]
fn test_ddsql_table_query_accepts_explicit_stdin_marker() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from(["pup", "ddsql", "table", "--query", "-"])
        .expect("ddsql table --query - should parse");

    match cli.command {
        crate::Commands::Ddsql { action } => match action {
            crate::DdsqlActions::Table { query, .. } => {
                assert_eq!(query, "-");
            }
            _ => panic!("expected DdsqlActions::Table"),
        },
        _ => panic!("expected Commands::Ddsql"),
    }
}

#[test]
fn test_ddsql_time_series_query_accepts_explicit_stdin_marker() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from(["pup", "ddsql", "time-series", "--query", "-"])
        .expect("ddsql time-series --query - should parse");

    match cli.command {
        crate::Commands::Ddsql { action } => match action {
            crate::DdsqlActions::TimeSeries { query, .. } => {
                assert_eq!(query, "-");
            }
            _ => panic!("expected DdsqlActions::TimeSeries"),
        },
        _ => panic!("expected Commands::Ddsql"),
    }
}

#[test]
fn test_ddsql_table_query_requires_explicit_value() {
    let result = crate::Cli::command().try_get_matches_from(["pup", "ddsql", "table", "--query"]);
    assert!(
        result.is_err(),
        "expected ddsql table --query to require a value"
    );
}

// -------------------------------------------------------------------------
// --sort with hyphen-prefixed values (e.g. -failure_rate, -timestamp)
// -------------------------------------------------------------------------

#[test]
fn test_cicd_flaky_tests_search_sort_accepts_hyphen_value() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from([
        "pup",
        "cicd",
        "flaky-tests",
        "search",
        "--query",
        "*",
        "--sort",
        "-failure_rate",
    ])
    .expect("cicd flaky-tests search --sort -failure_rate should parse");

    match cli.command {
        crate::Commands::Cicd { action } => match action {
            crate::CicdActions::FlakyTests { action } => match action {
                crate::CicdFlakyTestActions::Search { sort, .. } => {
                    assert_eq!(sort.as_deref(), Some("-failure_rate"));
                }
                _ => panic!("expected CicdFlakyTestActions::Search"),
            },
            _ => panic!("expected CicdActions::FlakyTests"),
        },
        _ => panic!("expected Commands::Cicd"),
    }
}

#[test]
fn test_cicd_flaky_tests_search_sort_accepts_positive_value() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from(["pup", "cicd", "flaky-tests", "search", "--sort", "fqn"])
        .expect("cicd flaky-tests search --sort fqn should parse");

    match cli.command {
        crate::Commands::Cicd { action } => match action {
            crate::CicdActions::FlakyTests { action } => match action {
                crate::CicdFlakyTestActions::Search { sort, .. } => {
                    assert_eq!(sort.as_deref(), Some("fqn"));
                }
                _ => panic!("expected CicdFlakyTestActions::Search"),
            },
            _ => panic!("expected CicdActions::FlakyTests"),
        },
        _ => panic!("expected Commands::Cicd"),
    }
}

#[test]
fn test_logs_list_sort_accepts_hyphen_timestamp() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from(["pup", "logs", "list", "--sort", "-timestamp"])
        .expect("logs list --sort -timestamp should parse");

    match cli.command {
        crate::Commands::Logs { action } => match action {
            crate::LogActions::List { sort, .. } => {
                assert_eq!(sort, "-timestamp");
            }
            _ => panic!("expected LogActions::List"),
        },
        _ => panic!("expected Commands::Logs"),
    }
}

#[test]
fn test_logs_saved_views_create_parses() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from([
        "pup",
        "logs",
        "saved-views",
        "create",
        "--file",
        "view.json",
    ])
    .expect("logs saved-views create --file should parse");

    match cli.command {
        crate::Commands::Logs { action } => match action {
            crate::LogActions::SavedViews { action } => match action {
                crate::LogSavedViewActions::Create { file } => {
                    assert_eq!(file, "view.json");
                }
                _ => panic!("expected LogSavedViewActions::Create"),
            },
            _ => panic!("expected LogActions::SavedViews"),
        },
        _ => panic!("expected Commands::Logs"),
    }
}

#[test]
fn test_traces_search_sort_accepts_hyphen_timestamp() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from([
        "pup",
        "traces",
        "search",
        "--query",
        "*",
        "--sort",
        "-timestamp",
    ])
    .expect("traces search --sort -timestamp should parse");

    match cli.command {
        crate::Commands::Traces { action } => match action {
            crate::TracesActions::Search { sort, .. } => {
                assert_eq!(sort, "-timestamp");
            }
            _ => panic!("expected TracesActions::Search"),
        },
        _ => panic!("expected Commands::Traces"),
    }
}

#[test]
fn test_security_rules_list_sort_accepts_hyphen_name() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from(["pup", "security", "rules", "list", "--sort", "-name"])
        .expect("security rules list --sort -name should parse");

    match cli.command {
        crate::Commands::Security { action } => match action {
            crate::SecurityActions::Rules { action } => match action {
                crate::SecurityRuleActions::List { sort, .. } => {
                    assert_eq!(sort.as_deref(), Some("-name"));
                }
                _ => panic!("expected SecurityRuleActions::List"),
            },
            _ => panic!("expected SecurityActions::Rules"),
        },
        _ => panic!("expected Commands::Security"),
    }
}

// -------------------------------------------------------------------------
// SymDB (duplicate of commands::symdb::tests::test_symdb_view_display, kept
// here because colocating would collide with the pre-existing copy).
// -------------------------------------------------------------------------

#[test]
fn test_symdb_view_display() {
    assert_eq!(crate::commands::symdb::SymdbView::Full.to_string(), "full");
    assert_eq!(
        crate::commands::symdb::SymdbView::Names.to_string(),
        "names"
    );
    assert_eq!(
        crate::commands::symdb::SymdbView::ProbeLocations.to_string(),
        "probe-locations"
    );
}

// -------------------------------------------------------------------------
// Audit logs alias: `pup audit` == `pup audit-logs`
// -------------------------------------------------------------------------

#[test]
fn test_audit_alias_search_parses() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from([
        "pup",
        "audit",
        "search",
        "--query",
        "@action:deleted",
        "--from",
        "24h",
    ])
    .expect("pup audit search should parse via alias");

    match cli.command {
        crate::Commands::AuditLogs { action } => match action {
            crate::AuditLogActions::Search {
                query,
                from,
                to,
                limit,
            } => {
                assert_eq!(query, "@action:deleted");
                assert_eq!(from, "24h");
                assert_eq!(to, "now");
                assert_eq!(limit, 100);
            }
            _ => panic!("expected AuditLogActions::Search"),
        },
        _ => panic!("expected Commands::AuditLogs"),
    }
}

#[test]
fn test_audit_alias_list_parses() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from(["pup", "audit", "list", "--from", "6h", "--limit", "50"])
        .expect("pup audit list should parse via alias");

    match cli.command {
        crate::Commands::AuditLogs { action } => match action {
            crate::AuditLogActions::List { from, to, limit } => {
                assert_eq!(from, "6h");
                assert_eq!(to, "now");
                assert_eq!(limit, 50);
            }
            _ => panic!("expected AuditLogActions::List"),
        },
        _ => panic!("expected Commands::AuditLogs"),
    }
}

#[test]
fn test_audit_canonical_name_still_parses() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from([
        "pup",
        "audit-logs",
        "search",
        "--query",
        "@usr.email:admin@example.com",
    ])
    .expect("pup audit-logs search should still parse");

    match cli.command {
        crate::Commands::AuditLogs { action } => match action {
            crate::AuditLogActions::Search { query, .. } => {
                assert_eq!(query, "@usr.email:admin@example.com");
            }
            _ => panic!("expected AuditLogActions::Search"),
        },
        _ => panic!("expected Commands::AuditLogs"),
    }
}

#[test]
fn test_audit_search_all_flags() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from([
        "pup",
        "audit",
        "search",
        "--query",
        "@metadata.api_key.id:KEY123",
        "--from",
        "90d",
        "--to",
        "2026-01-01T00:00:00Z",
        "--limit",
        "200",
    ])
    .expect("pup audit search with all flags should parse");

    match cli.command {
        crate::Commands::AuditLogs { action } => match action {
            crate::AuditLogActions::Search {
                query,
                from,
                to,
                limit,
            } => {
                assert_eq!(query, "@metadata.api_key.id:KEY123");
                assert_eq!(from, "90d");
                assert_eq!(to, "2026-01-01T00:00:00Z");
                assert_eq!(limit, 200);
            }
            _ => panic!("expected AuditLogActions::Search"),
        },
        _ => panic!("expected Commands::AuditLogs"),
    }
}

#[test]
fn test_audit_alias_is_visible() {
    use clap::CommandFactory;

    let app = crate::Cli::command();
    // find_subcommand searches both canonical names and aliases
    let found = app.find_subcommand("audit");
    assert!(
        found.is_some(),
        "`audit` should be findable as a visible alias of audit-logs"
    );
    // confirm it resolves to the audit-logs command, not a different one
    assert_eq!(
        found.unwrap().get_name(),
        "audit-logs",
        "`audit` alias should resolve to the audit-logs command"
    );
}

// -------------------------------------------------------------------------
// Dashboard embedded widgets (pup dashboards widgets *)
// -------------------------------------------------------------------------

#[test]
fn test_dashboards_widgets_add_parses_as_write() {
    let matches = crate::Cli::command()
        .try_get_matches_from([
            "pup",
            "dashboards",
            "widgets",
            "add",
            "abc-123",
            "--file",
            "w.json",
        ])
        .unwrap();
    let leaf = crate::get_leaf_subcommand_name(&matches).unwrap();
    assert!(
        crate::is_write_command_name(&leaf),
        "dashboards widgets add must be classified as a write command, got leaf={leaf:?}"
    );
}

#[test]
fn test_dashboards_widgets_remove_parses_as_write() {
    let matches = crate::Cli::command()
        .try_get_matches_from([
            "pup",
            "dashboards",
            "widgets",
            "remove",
            "abc-123",
            "--index",
            "0",
        ])
        .unwrap();
    let leaf = crate::get_leaf_subcommand_name(&matches).unwrap();
    assert!(
        crate::is_write_command_name(&leaf),
        "dashboards widgets remove must be classified as a write command, got leaf={leaf:?}"
    );
}

#[test]
fn test_dashboards_widgets_types_parses_as_read() {
    let matches = crate::Cli::command()
        .try_get_matches_from(["pup", "dashboards", "widgets", "types"])
        .unwrap();
    let leaf = crate::get_leaf_subcommand_name(&matches).unwrap();
    assert!(
        !crate::is_write_command_name(&leaf),
        "dashboards widgets types must be classified as a read command, got leaf={leaf:?}"
    );
}

#[test]
fn test_dashboards_widgets_schema_parses_as_read() {
    let matches = crate::Cli::command()
        .try_get_matches_from(["pup", "dashboards", "widgets", "schema", "timeseries"])
        .unwrap();
    let leaf = crate::get_leaf_subcommand_name(&matches).unwrap();
    assert!(
        !crate::is_write_command_name(&leaf),
        "dashboards widgets schema must be classified as a read command, got leaf={leaf:?}"
    );
}

#[test]
fn test_dashboards_widgets_add_parses_args() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from([
        "pup",
        "dashboards",
        "widgets",
        "add",
        "abc-123",
        "--file",
        "widget.json",
    ])
    .expect("dashboards widgets add should parse");

    match cli.command {
        crate::Commands::Dashboards { action } => {
            let crate::DashboardActions::Widgets { action } = action else {
                panic!("expected DashboardActions::Widgets");
            };
            let crate::DashboardWidgetActions::Add { dash_id, file } = action else {
                panic!("expected DashboardWidgetActions::Add");
            };
            assert_eq!(dash_id, "abc-123");
            assert_eq!(file, "widget.json");
        }
        _ => panic!("expected Commands::Dashboards"),
    }
}

#[test]
fn test_dashboards_widgets_get_by_index_parses() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from([
        "pup",
        "dashboards",
        "widgets",
        "get",
        "abc-123",
        "--index",
        "2",
    ])
    .expect("dashboards widgets get --index should parse");

    match cli.command {
        crate::Commands::Dashboards { action } => {
            let crate::DashboardActions::Widgets { action } = action else {
                panic!("expected DashboardActions::Widgets");
            };
            let crate::DashboardWidgetActions::Get {
                dash_id,
                widget_id,
                index,
            } = action
            else {
                panic!("expected DashboardWidgetActions::Get");
            };
            assert_eq!(dash_id, "abc-123");
            assert_eq!(widget_id, None);
            assert_eq!(index, Some(2));
        }
        _ => panic!("expected Commands::Dashboards"),
    }
}

#[test]
fn test_dashboards_widgets_get_requires_selector() {
    // Neither --widget-id nor --index provided — should fail clap validation.
    let result = crate::Cli::command().try_get_matches_from([
        "pup",
        "dashboards",
        "widgets",
        "get",
        "abc-123",
    ]);
    assert!(
        result.is_err(),
        "dashboards widgets get must require --widget-id or --index"
    );
}

#[test]
fn test_dashboards_widgets_get_rejects_both_selectors() {
    // Both --widget-id and --index provided — should fail clap's conflicts_with.
    let result = crate::Cli::command().try_get_matches_from([
        "pup",
        "dashboards",
        "widgets",
        "get",
        "abc-123",
        "--widget-id",
        "1",
        "--index",
        "0",
    ]);
    assert!(
        result.is_err(),
        "dashboards widgets get must reject --widget-id and --index together"
    );
}

#[test]
fn test_dashboards_widgets_schema_parses() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from(["pup", "dashboards", "widgets", "schema", "timeseries"])
        .expect("dashboards widgets schema should parse");

    match cli.command {
        crate::Commands::Dashboards { action } => {
            let crate::DashboardActions::Widgets { action } = action else {
                panic!("expected DashboardActions::Widgets");
            };
            let crate::DashboardWidgetActions::Schema { r#type } = action else {
                panic!("expected DashboardWidgetActions::Schema");
            };
            assert_eq!(r#type, "timeseries");
        }
        _ => panic!("expected Commands::Dashboards"),
    }
}

// -------------------------------------------------------------------------
// Top-level saved widgets (pup saved-widgets *)
// -------------------------------------------------------------------------

#[test]
fn test_saved_widgets_list_parses() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from([
        "pup",
        "saved-widgets",
        "list",
        "logs_reports",
        "--page-size",
        "10",
    ])
    .expect("pup saved-widgets list should parse");

    match cli.command {
        crate::Commands::SavedWidgets { action } => match action {
            crate::WidgetActions::List {
                experience_type,
                page_size,
                ..
            } => {
                assert_eq!(experience_type, "logs_reports");
                assert_eq!(page_size, Some(10));
            }
            _ => panic!("expected WidgetActions::List"),
        },
        _ => panic!("expected Commands::SavedWidgets"),
    }
}

#[test]
fn test_saved_widgets_get_parses() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from([
        "pup",
        "saved-widgets",
        "get",
        "ccm_reports",
        "uuid-here-123",
    ])
    .expect("pup saved-widgets get should parse");

    match cli.command {
        crate::Commands::SavedWidgets { action } => match action {
            crate::WidgetActions::Get {
                experience_type,
                widget_id,
            } => {
                assert_eq!(experience_type, "ccm_reports");
                assert_eq!(widget_id, "uuid-here-123");
            }
            _ => panic!("expected WidgetActions::Get"),
        },
        _ => panic!("expected Commands::SavedWidgets"),
    }
}

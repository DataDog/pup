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

// -------------------------------------------------------------------------
// Global --site flag
// -------------------------------------------------------------------------

#[test]
fn test_auth_status_accepts_global_site_flag() {
    use clap::Parser;

    // --site is now a global flag; clap accepts it after the subcommand and
    // stores it on the top-level Cli, not on AuthActions::Status.
    let cli = crate::Cli::try_parse_from(["pup", "auth", "status", "--site", "datadoghq.eu"])
        .expect("auth status --site should parse");

    assert_eq!(cli.site.as_deref(), Some("datadoghq.eu"));
    match cli.command {
        crate::Commands::Auth { action } => {
            assert!(matches!(action, crate::AuthActions::Status));
        }
        _ => panic!("expected Commands::Auth"),
    }
}

#[test]
fn test_auth_status_site_flag_is_optional() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from(["pup", "auth", "status"])
        .expect("auth status without --site should parse");

    assert_eq!(cli.site, None);
    match cli.command {
        crate::Commands::Auth { action } => {
            assert!(matches!(action, crate::AuthActions::Status));
        }
        _ => panic!("expected Commands::Auth"),
    }
}

#[test]
fn test_global_site_flag_on_non_auth_command() {
    use clap::Parser;

    // Regression for the original bug: `--site` on a non-auth command used to
    // error with "unexpected argument '--site'". As a global flag it now parses
    // both before and after the subcommand.
    let cli = crate::Cli::try_parse_from(["pup", "--site", "datadoghq.eu", "monitors", "list"])
        .expect("--site before a non-auth subcommand should parse");
    assert_eq!(cli.site.as_deref(), Some("datadoghq.eu"));

    let cli = crate::Cli::try_parse_from(["pup", "monitors", "list", "--site=us3.datadoghq.com"])
        .expect("--site after a non-auth subcommand should parse");
    assert_eq!(cli.site.as_deref(), Some("us3.datadoghq.com"));
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

//! Cross-cutting CLI integration tests that don't belong to a single command
//! module: read-only guard detection, top-level CLI shape, and multi-domain
//! clap parsing checks.
//!
//! Most per-command tests have been colocated with their modules in
//! `#[cfg(test)] mod tests { ... }` blocks. Shared helpers live in
//! `crate::test_support`.

use clap::CommandFactory;

// -------------------------------------------------------------------------
// Notebook discovery
// -------------------------------------------------------------------------

#[test]
fn test_notebooks_search_parses_without_query() {
    use clap::Parser;

    let cli =
        crate::Cli::try_parse_from(["pup", "notebooks", "search", "--filter", "tags:production"])
            .expect("notebooks search should not require --query");

    let crate::Commands::Notebooks { action } = cli.command else {
        panic!("expected Commands::Notebooks");
    };
    let crate::NotebookActions::Search { query, options } = action else {
        panic!("expected NotebookActions::Search");
    };
    assert_eq!(query, None);
    assert_eq!(options.filters, ["tags:production"]);
    assert_eq!(options.limit, 20);
}

#[test]
fn test_notebooks_list_is_a_hidden_search_alias() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from(["pup", "notebooks", "list"])
        .expect("notebooks list should remain a compatibility alias");

    let crate::Commands::Notebooks { action } = cli.command else {
        panic!("expected Commands::Notebooks");
    };
    let crate::NotebookActions::Search { query, options } = action else {
        panic!("expected the list alias to resolve to NotebookActions::Search");
    };
    assert_eq!(query, None);
    assert!(options.filters.is_empty());
    assert_eq!(options.sort, "name");
    assert_eq!(options.limit, 20);

    let help = crate::Cli::command()
        .find_subcommand("notebooks")
        .expect("notebooks command should exist")
        .clone()
        .render_long_help()
        .to_string();
    assert!(
        !help.contains("list"),
        "hidden alias leaked into help: {help}"
    );
}

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
fn test_on_call_pages_list_accepts_page_zero() {
    let result = crate::Cli::command()
        .try_get_matches_from(["pup", "on-call", "pages", "list", "--page", "0"]);
    assert!(result.is_ok());
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

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn test_auth_token_parses_and_appears_in_human_help() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from(["pup", "auth", "token"])
        .expect("auth token should parse in native builds");
    assert!(matches!(
        cli.command,
        crate::Commands::Auth {
            action: crate::AuthActions::Token
        }
    ));

    let help = crate::Cli::command()
        .find_subcommand("auth")
        .expect("auth command should exist")
        .clone()
        .render_long_help()
        .to_string();
    assert!(
        help.lines()
            .any(|line| line.split_whitespace().next() == Some("token")),
        "auth token missing from help: {help}"
    );
}

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
fn test_ddsql_table_uses_api_row_limit_default() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from(["pup", "ddsql", "table", "--query", "SELECT 1"])
        .expect("ddsql table should parse");

    match cli.command {
        crate::Commands::Ddsql { action } => match action {
            crate::DdsqlActions::Table { limit, .. } => assert_eq!(limit, 5000),
            _ => panic!("expected DdsqlActions::Table"),
        },
        _ => panic!("expected Commands::Ddsql"),
    }
}

#[test]
fn test_ddsql_table_accepts_limit_override() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from([
        "pup", "ddsql", "table", "--query", "SELECT 1", "--limit", "10000",
    ])
    .expect("ddsql table should accept a row limit override");

    match cli.command {
        crate::Commands::Ddsql { action } => match action {
            crate::DdsqlActions::Table { limit, .. } => assert_eq!(limit, 10000),
            _ => panic!("expected DdsqlActions::Table"),
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

#[test]
fn test_ddsql_time_help_documents_supported_formats() {
    let root = crate::Cli::command();
    let table_help = root
        .find_subcommand("ddsql")
        .unwrap()
        .find_subcommand("table")
        .unwrap()
        .clone()
        .render_long_help()
        .to_string();
    let security_help = root
        .find_subcommand("security")
        .unwrap()
        .find_subcommand("findings")
        .unwrap()
        .find_subcommand("analyze")
        .unwrap()
        .clone()
        .render_long_help()
        .to_string();

    for (command, help) in [
        ("ddsql table", table_help),
        ("security findings analyze", security_help),
    ] {
        for expected in [
            "now-<duration>",
            "now-24h",
            "relative duration",
            "RFC 3339 timestamp",
            "Unix seconds",
            "Unix milliseconds",
        ] {
            assert!(
                help.contains(expected),
                "{command} help is missing {expected:?}: {help}"
            );
        }
    }
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
fn test_logs_patterns_parses_and_is_read_only() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from([
        "pup",
        "logs",
        "patterns",
        "--query",
        "status:error",
        "--pattern-field",
        "message",
        "--index",
        "main,security",
        "--group-by",
        "service,status",
    ])
    .expect("logs patterns should parse");

    match cli.command {
        crate::Commands::Logs {
            action:
                crate::LogActions::Patterns {
                    pattern_field,
                    sample_limit,
                    event_limit,
                    index,
                    group_by,
                    ..
                },
        } => {
            assert_eq!(pattern_field, "message");
            assert_eq!(sample_limit, 50);
            assert_eq!(event_limit, 10_000);
            assert_eq!(index, vec!["main", "security"]);
            assert_eq!(group_by, vec!["service", "status"]);
        }
        _ => panic!("expected LogActions::Patterns"),
    }

    let command = crate::Cli::command();
    let schema = crate::build_agent_schema(&command);
    let logs = schema["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|command| command["name"] == "logs")
        .expect("logs must be present in the agent schema");
    let patterns = logs["subcommands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|command| command["name"] == "patterns")
        .expect("logs patterns must be present in the agent schema");
    assert_eq!(patterns["read_only"], true);
}

#[test]
fn test_idp_entity_graph_commands_parse() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from([
        "pup",
        "idp",
        "entities",
        "query",
        "kind:service AND owner:payments",
        "--field",
        "name,owner",
        "--include",
        "owner_teams",
        "--order-by",
        "name:desc",
        "--limit",
        "50",
        "--cursor",
        "next-page",
        "--free-text-match",
        "fuzzy",
        "--include-total-count",
        "--timeseries-interval",
        "24h",
        "--relation-limit",
        "10",
        "--raw",
    ])
    .expect("IDP entity query should parse");

    match cli.command {
        crate::Commands::Idp {
            action:
                crate::IdpActions::Entities {
                    action:
                        crate::IdpEntitiesActions::Query {
                            query,
                            field,
                            include,
                            order_by,
                            limit,
                            cursor,
                            free_text_match,
                            include_total_count,
                            timeseries_interval,
                            relation_limit,
                            raw,
                        },
                },
        } => {
            assert_eq!(query, "kind:service AND owner:payments");
            assert_eq!(field, vec!["name", "owner"]);
            assert_eq!(include, vec!["owner_teams"]);
            assert_eq!(order_by, vec!["name:desc"]);
            assert_eq!(limit, 50);
            assert_eq!(cursor.as_deref(), Some("next-page"));
            assert_eq!(free_text_match.as_deref(), Some("fuzzy"));
            assert!(include_total_count);
            assert_eq!(timeseries_interval, "24h");
            assert_eq!(relation_limit, 10);
            assert!(raw);
        }
        _ => panic!("expected IdpEntitiesActions::Query"),
    }

    let kinds = crate::Cli::try_parse_from([
        "pup",
        "idp",
        "kinds",
        "list",
        "--all",
        "--include-custom",
        "--include-low-level",
        "--exclude-experimental",
    ])
    .expect("IDP kinds list should parse");
    match kinds.command {
        crate::Commands::Idp {
            action:
                crate::IdpActions::Kinds {
                    action:
                        crate::IdpKindsActions::List {
                            all,
                            include_custom,
                            include_low_level,
                            exclude_experimental,
                        },
                },
        } => {
            assert!(all);
            assert!(include_custom);
            assert!(include_low_level);
            assert!(exclude_experimental);
        }
        _ => panic!("expected IdpKindsActions::List"),
    }
}

#[test]
fn test_idp_entity_query_rejects_unknown_free_text_match_mode() {
    use clap::Parser;

    let result = crate::Cli::try_parse_from([
        "pup",
        "idp",
        "entities",
        "query",
        "kind:service",
        "--free-text-match",
        "exact",
    ]);
    let Err(error) = result else {
        panic!("unsupported matching modes should fail during CLI parsing");
    };

    let message = error.to_string();
    assert!(message.contains("invalid value 'exact'"));
    assert!(message.contains("partial"));
    assert!(message.contains("fuzzy"));
}

#[test]
fn test_idp_entity_graph_schema_marks_commands_read_only() {
    use clap::CommandFactory;

    let schema = crate::build_agent_schema(&crate::Cli::command());
    let idp = schema["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|command| command["name"] == "idp")
        .unwrap();
    let kinds = idp["subcommands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|command| command["name"] == "kinds")
        .unwrap();
    let describe = kinds["subcommands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|command| command["name"] == "describe")
        .unwrap();
    assert_eq!(describe["read_only"], true);

    let entities = idp["subcommands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|command| command["name"] == "entities")
        .unwrap();
    let query = entities["subcommands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|command| command["name"] == "query")
        .unwrap();
    assert_eq!(query["read_only"], true);
    assert!(query["flags"]
        .as_array()
        .unwrap()
        .iter()
        .any(|flag| flag["name"] == "--relation-limit"));
}

#[test]
fn test_read_only_guard_allows_idp_entity_graph_commands() {
    use clap::CommandFactory;

    for args in [
        vec!["pup", "idp", "kinds", "list"],
        vec!["pup", "idp", "kinds", "describe", "service"],
        vec!["pup", "idp", "entities", "query", "kind:service"],
    ] {
        let matches = crate::Cli::command().try_get_matches_from(args).unwrap();
        let leaf = crate::get_leaf_subcommand_name(&matches).unwrap();
        assert!(!crate::is_write_command_name(&leaf));
    }
}

#[test]
fn test_logs_search_and_aggregate_default_to_auto_storage() {
    use clap::Parser;

    let search = crate::Cli::try_parse_from(["pup", "logs", "search", "--query", "*"])
        .expect("logs search should parse");
    let aggregate = crate::Cli::try_parse_from(["pup", "logs", "aggregate"])
        .expect("logs aggregate should parse");

    match search.command {
        crate::Commands::Logs {
            action: crate::LogActions::Search { storage, .. },
        } => assert_eq!(storage.as_deref(), None),
        _ => panic!("expected LogActions::Search"),
    }
    match aggregate.command {
        crate::Commands::Logs {
            action: crate::LogActions::Aggregate { storage, .. },
        } => assert_eq!(storage.as_deref(), None),
        _ => panic!("expected LogActions::Aggregate"),
    }
}

#[test]
fn test_logs_search_and_aggregate_accept_explicit_auto_storage() {
    use clap::Parser;

    let search =
        crate::Cli::try_parse_from(["pup", "logs", "search", "--query", "*", "--storage", "auto"])
            .expect("logs search --storage auto should parse");
    let aggregate = crate::Cli::try_parse_from(["pup", "logs", "aggregate", "--storage", "auto"])
        .expect("logs aggregate --storage auto should parse");

    match search.command {
        crate::Commands::Logs {
            action: crate::LogActions::Search { storage, .. },
        } => assert_eq!(storage.as_deref(), Some("auto")),
        _ => panic!("expected LogActions::Search"),
    }
    match aggregate.command {
        crate::Commands::Logs {
            action: crate::LogActions::Aggregate { storage, .. },
        } => assert_eq!(storage.as_deref(), Some("auto")),
        _ => panic!("expected LogActions::Aggregate"),
    }
}

#[test]
fn test_logs_search_cursor_parses() {
    use clap::Parser;

    let cli = crate::Cli::try_parse_from([
        "pup",
        "logs",
        "search",
        "--query",
        "*",
        "--cursor",
        "cursor-abc",
    ])
    .expect("logs search --cursor should parse");

    match cli.command {
        crate::Commands::Logs {
            action: crate::LogActions::Search { cursor, .. },
        } => {
            assert_eq!(cursor.as_deref(), Some("cursor-abc"));
        }
        _ => panic!("expected LogActions::Search"),
    }
}

#[test]
fn test_logs_list_and_query_cursor_parse() {
    use clap::Parser;

    let list = crate::Cli::try_parse_from(["pup", "logs", "list", "--cursor", "list-cursor"])
        .expect("logs list --cursor should parse");
    let query = crate::Cli::try_parse_from([
        "pup",
        "logs",
        "query",
        "--query",
        "status:error",
        "--cursor",
        "query-cursor",
    ])
    .expect("logs query --cursor should parse");

    match list.command {
        crate::Commands::Logs {
            action: crate::LogActions::List { cursor, .. },
        } => assert_eq!(cursor.as_deref(), Some("list-cursor")),
        _ => panic!("expected LogActions::List"),
    }
    match query.command {
        crate::Commands::Logs {
            action: crate::LogActions::Query { cursor, .. },
        } => assert_eq!(cursor.as_deref(), Some("query-cursor")),
        _ => panic!("expected LogActions::Query"),
    }
}

#[test]
fn test_logs_search_and_aggregate_storage_overrides_are_preserved() {
    use clap::Parser;

    let search = crate::Cli::try_parse_from([
        "pup",
        "logs",
        "search",
        "--query",
        "*",
        "--storage",
        "indexes",
    ])
    .expect("logs search --storage indexes should parse");
    let aggregate =
        crate::Cli::try_parse_from(["pup", "logs", "aggregate", "--storage", "online-archives"])
            .expect("logs aggregate --storage online-archives should parse");

    match search.command {
        crate::Commands::Logs {
            action: crate::LogActions::Search { storage, .. },
        } => assert_eq!(storage.as_deref(), Some("indexes")),
        _ => panic!("expected LogActions::Search"),
    }
    match aggregate.command {
        crate::Commands::Logs {
            action: crate::LogActions::Aggregate { storage, .. },
        } => assert_eq!(storage.as_deref(), Some("online-archives")),
        _ => panic!("expected LogActions::Aggregate"),
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
fn test_logs_storage_help_mentions_long_lookback_storage() {
    let cmd = crate::Cli::command();
    let logs_cmd = cmd
        .find_subcommand("logs")
        .expect("logs subcommand should exist");

    for subcommand in ["search", "aggregate", "list", "query"] {
        let mut command = logs_cmd
            .find_subcommand(subcommand)
            .unwrap_or_else(|| panic!("logs {subcommand} subcommand should exist"))
            .clone();
        let help = command.render_help().to_string();

        assert!(
            help.contains(
                "Long lookback queries may require flex or online-archives for full retention"
            ),
            "logs {subcommand} help should mention long-lookback storage guidance"
        );
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

// -------------------------------------------------------------------------
// Agent-mode --help intercept: subcommand resolution
//
// The `--help` intercept in `main_inner` emits a JSON schema only when the
// requested command resolves. `find_subcommand` drives that decision:
//   Some(_)          -> scoped schema
//   None (empty)     -> root schema
//   None (non-empty) -> fall through to clap, which reports the typo
// -------------------------------------------------------------------------

#[test]
fn test_find_subcommand_resolves_when_name_valid() {
    let cmd = crate::Cli::command();
    let found = crate::find_subcommand(&cmd, &["monitors"]);
    assert_eq!(
        found.map(|c| c.get_name()),
        Some("monitors"),
        "a valid top-level subcommand should resolve to itself"
    );
}

#[test]
fn test_find_subcommand_returns_none_when_name_is_typo() {
    let cmd = crate::Cli::command();
    // `monitor` (singular) is a typo for `monitors`; it must not resolve so the
    // intercept falls through to clap's "did you mean" suggestion.
    assert!(
        crate::find_subcommand(&cmd, &["monitor"]).is_none(),
        "an unknown subcommand must not resolve"
    );
}

#[test]
fn test_find_subcommand_returns_none_when_path_empty() {
    let cmd = crate::Cli::command();
    // No subcommand given -> root schema branch, not scoped.
    assert!(
        crate::find_subcommand(&cmd, &[]).is_none(),
        "an empty path must not resolve to any subcommand"
    );
}

#[test]
fn test_find_subcommand_resolves_when_alias_used() {
    let cmd = crate::Cli::command();
    // `audit` is a visible alias of `audit-logs`; it must resolve so agents
    // still get the scoped JSON schema rather than clap's plain-text help.
    let found = crate::find_subcommand(&cmd, &["audit"]);
    assert_eq!(
        found.map(|c| c.get_name()),
        Some("audit-logs"),
        "a visible alias should resolve to its canonical command"
    );
}

#[test]
fn test_clap_reports_invalid_nested_subcommand_with_suggestion() {
    let result = crate::Cli::command()
        .try_get_matches_from(["pup", "monitors", "lits", "--help", "--agent"]);
    let err = result.expect_err("clap should reject an unknown subcommand");
    assert_eq!(
        err.kind(),
        clap::error::ErrorKind::InvalidSubcommand,
        "unknown subcommand should surface as InvalidSubcommand"
    );
    let rendered = err.to_string();
    assert!(rendered.contains("unrecognized subcommand 'lits'"));
    assert!(rendered.contains("a similar subcommand exists: 'list'"));
}

#[test]
fn test_find_subcommand_resolves_nested_path() {
    let cmd = crate::Cli::command();
    // A valid two-level path resolves to the leaf command.
    let found = crate::find_subcommand(&cmd, &["monitors", "list"]);
    assert_eq!(
        found.map(|c| c.get_name()),
        Some("list"),
        "a valid nested path should resolve to the leaf subcommand"
    );
}

fn owned(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}

#[test]
fn test_top_level_subcommand_returns_first_positional() {
    let args = owned(&["pup", "monitors", "list", "--help", "--agent"]);
    assert_eq!(crate::top_level_subcommand(&args), Some("monitors"));
}

#[test]
fn test_top_level_subcommand_skips_value_global_before_subcommand() {
    // The value of `--org` must not be mistaken for the subcommand.
    let args = owned(&["pup", "--org", "myorg", "monitors", "--help", "--agent"]);
    assert_eq!(crate::top_level_subcommand(&args), Some("monitors"));
}

#[test]
fn test_top_level_subcommand_skips_short_value_global() {
    let args = owned(&["pup", "-o", "table", "logs", "--help", "--agent"]);
    assert_eq!(crate::top_level_subcommand(&args), Some("logs"));
}

#[test]
fn test_top_level_subcommand_handles_attached_value_form() {
    // `--output=table` is one token and consumes no following token.
    let args = owned(&["pup", "--output=table", "logs", "--help"]);
    assert_eq!(crate::top_level_subcommand(&args), Some("logs"));
}

#[test]
fn test_top_level_subcommand_returns_none_when_flags_only() {
    let args = owned(&["pup", "--agent", "--help"]);
    assert_eq!(crate::top_level_subcommand(&args), None);
}

#[test]
fn test_agent_help_schema_for_valid_nested_subcommand() {
    let cmd = crate::Cli::command();
    let args = owned(&["pup", "monitors", "list", "--help", "--agent"]);

    let schema = crate::agent_help_schema(&cmd, &args)
        .expect("valid nested agent help should return a schema");

    assert_eq!(schema["description"], "Manage monitors");
}

#[test]
fn test_agent_help_falls_through_for_invalid_nested_subcommand() {
    let cmd = crate::Cli::command();
    let args = owned(&["pup", "monitors", "lits", "--help", "--agent"]);

    assert!(crate::agent_help_schema(&cmd, &args).is_none());
}

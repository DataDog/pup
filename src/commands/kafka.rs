//! [Experimental] Kafka inspection commands.
//!
//! These endpoints are experimental — the API surface is not covered by
//! Datadog's public API compatibility guarantees and may change without
//! notice.
//!
//! Authentication: OAuth2 bearer token (e.g. `pup auth login`). DD_API_KEY +
//! DD_APP_KEY is not accepted by these UI routes.

use anyhow::Result;
use serde_json::{json, Value};

use crate::config::Config;
use crate::formatter;
use crate::raw_client;

const TOPIC_CONFIGS_PATH: &str = "/api/ui/data_streams/kafka_topic_configs";
const BROKER_CONFIGS_PATH: &str = "/api/ui/data_streams/kafka_broker_configs";
const CLIENT_CONFIGS_PATH: &str = "/api/ui/data_streams/kafka_client_configs";
const READ_MESSAGES_PATH: &str = "/api/ui/data_streams/kafka_actions/read_messages";
const SUBJECT_SCHEMAS_PATH: &str = "/api/ui/data_streams/subject_kafka_schemas";

pub async fn topic_configs(cfg: &Config, kafka_cluster_id: &str, topic: &str) -> Result<()> {
    let query = [("kafka_cluster_id", kafka_cluster_id), ("topic", topic)];
    let resp = raw_client::raw_get(cfg, TOPIC_CONFIGS_PATH, &query)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get kafka topic configs: {e:?}"))?;
    formatter::output(cfg, &resp)
}

pub async fn broker_configs(cfg: &Config, kafka_cluster_id: &str, broker_id: &str) -> Result<()> {
    let query = [
        ("kafka_cluster_id", kafka_cluster_id),
        ("broker_id", broker_id),
    ];
    let resp = raw_client::raw_get(cfg, BROKER_CONFIGS_PATH, &query)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get kafka broker configs: {e:?}"))?;
    formatter::output(cfg, &resp)
}

/// `services` is a list of `service:config_type` pairs where `config_type` is
/// either `producer` or `consumer`.
pub async fn client_configs(
    cfg: &Config,
    kafka_cluster_id: &str,
    services: Vec<(String, String)>,
) -> Result<()> {
    if services.is_empty() {
        anyhow::bail!("at least one --service SERVICE:producer|consumer is required");
    }
    let services_json: Vec<Value> = services
        .into_iter()
        .map(|(service, config_type)| json!({ "service": service, "config_type": config_type }))
        .collect();
    let body = json!({
        "kafka_cluster_id": kafka_cluster_id,
        "services": services_json,
    });
    let resp = raw_client::raw_post(cfg, CLIENT_CONFIGS_PATH, body)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get kafka client configs: {e:?}"))?;
    formatter::output(cfg, &resp)
}

/// Dispatches a Kafka read-messages action to the agent via Remote Config and
/// polls until the agent responds.
#[allow(clippy::too_many_arguments)]
pub async fn read_messages(
    cfg: &Config,
    cluster: &str,
    topic: &str,
    bootstrap_servers: &str,
    partition: Option<i32>,
    start_offset: i64,
    start_timestamp: Option<i64>,
    n_messages_retrieved: u32,
    max_scanned_messages: u32,
    filter: Option<String>,
    consumer_group_id: Option<String>,
) -> Result<()> {
    let mut attrs = json!({
        "cluster": cluster,
        "topic": topic,
        "bootstrap_servers": bootstrap_servers,
        "start_offset": start_offset,
        "n_messages_retrieved": n_messages_retrieved,
        "max_scanned_messages": max_scanned_messages,
    });
    if let Some(p) = partition {
        attrs["partition"] = json!(p);
    }
    if let Some(ts) = start_timestamp {
        attrs["start_timestamp"] = json!(ts);
    }
    if let Some(f) = filter {
        attrs["filter"] = json!(f);
    }
    if let Some(cg) = consumer_group_id {
        attrs["consumer_group_id"] = json!(cg);
    }

    let resp =
        raw_client::raw_post_jsonapi(cfg, READ_MESSAGES_PATH, "kafka_action_read_messages", attrs)
            .await
            .map_err(|e| anyhow::anyhow!("failed to read kafka messages: {e:?}"))?;
    formatter::output(cfg, &resp)
}

/// All version history of a single Schema Registry subject on a Kafka cluster.
pub async fn subject_schemas(cfg: &Config, kafka_cluster_id: &str, subject: &str) -> Result<()> {
    let query = [("kafka_cluster_id", kafka_cluster_id), ("subject", subject)];
    let resp = raw_client::raw_get(cfg, SUBJECT_SCHEMAS_PATH, &query)
        .await
        .map_err(|e| anyhow::anyhow!("failed to get subject kafka schemas: {e:?}"))?;
    formatter::output(cfg, &resp)
}

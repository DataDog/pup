---
description: Use pup CLI for immediate Datadog operations or generate code for integration into applications
tags: [pup, cli, code-generation, typescript, python, java, go, rust]
---

# Datadog Integration Skill

This skill helps users interact with Datadog through two complementary approaches:
1. **Immediate execution** using the `pup` CLI tool
2. **Code generation** for application integration using Datadog API clients

## When to Use This Skill

Use this skill when the user:
- Wants to query Datadog data (logs, traces, metrics, etc.)
- Needs to configure Datadog (monitors, dashboards, SLOs, etc.)
- Asks to "generate code" for a Datadog operation
- Wants to integrate Datadog operations into their application
- Needs examples of using Datadog API clients in a specific language

## Pup CLI Tool

The `pup` CLI is a command-line wrapper for Datadog APIs written in Rust. It provides:
- OAuth2 authentication (preferred) or API key authentication
- 28 command groups covering 33+ API domains
- JSON, YAML, and table output formats
- 200+ subcommands for comprehensive Datadog operations

### Pup Authentication

```bash
# OAuth2 (preferred)
pup auth login

# API Keys (fallback)
export DD_API_KEY="your-api-key"
export DD_APP_KEY="your-app-key"
export DD_SITE="datadoghq.com"
```

### Pup Command Structure

```bash
pup <domain> <action> [options]
pup <domain> <subgroup> <action> [options]

# Examples
pup monitors list --tags="env:prod"
pup logs search --query="status:error" --from="1h"
pup metrics query --query="avg:system.cpu.user{*}" --from="1h"
```

## Supported Operations

### Core Observability
- **Metrics**: Query, list, search, submit metrics
- **Logs**: Search and aggregate log data
- **Traces**: Query APM traces and spans
- **Events**: List and search events
- **RUM**: Real user monitoring data

### Monitoring & Alerting
- **Monitors**: Full CRUD operations
- **Dashboards**: Create, list, get, delete
- **SLOs**: Service level objectives management
- **Synthetics**: Synthetic test management
- **Downtimes**: Monitor downtime management
- **Notebooks**: Investigation notebooks

### Security & Compliance
- **Security Monitoring**: Rules, signals, findings
- **Vulnerabilities**: Security vulnerability scanning
- **Static Analysis**: Code security analysis
- **Audit Logs**: Organizational audit trail
- **Data Governance**: Sensitive data scanning

### Infrastructure & Cloud
- **Infrastructure**: Host inventory and metrics
- **Tags**: Resource tagging
- **Cloud Integrations**: AWS, GCP, Azure

### Incident & Operations
- **Incidents**: Incident management
- **On-Call**: On-call team management
- **Error Tracking**: Application error tracking
- **Service Catalog**: Service registry
- **Scorecards**: Service quality metrics

### Organization & Access
- **Users**: User and role management
- **Organizations**: Org settings
- **API Keys**: API key management

See `pup --help` for complete command reference.

## Usage Patterns

### Pattern 1: Quick Query (Use Pup Directly)

When users want immediate results, execute pup commands:

```bash
# Query metrics
pup metrics query --query="avg:system.cpu.user{*}" --from="1h" --to="now"

# Search logs
pup logs search --query="status:error service:api" --from="30m"

# List monitors
pup monitors list --tags="team:backend"

# Get dashboard
pup dashboards get abc-123-def
```

### Pattern 2: Code Generation (For Application Integration)

When users want to integrate into their application, provide code examples using official Datadog API clients.

#### TypeScript Example (using @datadog/datadog-api-client)

```typescript
import { client, v2 } from '@datadog/datadog-api-client';

// Configure authentication
const configuration = client.createConfiguration({
  authMethods: {
    apiKeyAuth: process.env.DD_API_KEY || '',
    appKeyAuth: process.env.DD_APP_KEY || '',
  },
});

// Query metrics
async function queryMetrics() {
  const apiInstance = new v2.MetricsApi(configuration);

  try {
    const params: v2.MetricsApiQueryTimeseriesDataRequest = {
      body: {
        data: {
          type: 'timeseries_request',
          attributes: {
            formulas: [{
              formula: 'query1'
            }],
            queries: [{
              name: 'query1',
              dataSource: 'metrics',
              query: 'avg:system.cpu.user{*}'
            }],
            from: Date.now() - 3600000, // 1 hour ago
            to: Date.now()
          }
        }
      }
    };

    const result = await apiInstance.queryTimeseriesData(params);
    console.log(JSON.stringify(result, null, 2));
  } catch (error) {
    console.error('Error:', error);
  }
}

queryMetrics();
```

**Installation**: `npm install @datadog/datadog-api-client`

#### Python Example (using datadog-api-client)

```python
#!/usr/bin/env python3
import os
from datetime import datetime, timedelta
from datadog_api_client import ApiClient, Configuration
from datadog_api_client.v2.api.metrics_api import MetricsApi
from datadog_api_client.v2.model.timeseries_formula_request import TimeseriesFormulaRequest
from datadog_api_client.v2.model.timeseries_formula_query_request import TimeseriesFormulaQueryRequest
from datadog_api_client.v2.model.timeseries_formula_request_attributes import TimeseriesFormulaRequestAttributes
from datadog_api_client.v2.model.timeseries_formula_request_type import TimeseriesFormulaRequestType

def configure_datadog():
    configuration = Configuration()
    configuration.api_key['apiKeyAuth'] = os.getenv('DD_API_KEY')
    configuration.api_key['appKeyAuth'] = os.getenv('DD_APP_KEY')
    configuration.server_variables['site'] = os.getenv('DD_SITE', 'datadoghq.com')
    return configuration

def query_metrics():
    configuration = configure_datadog()

    with ApiClient(configuration) as api_client:
        api_instance = MetricsApi(api_client)

        # Query parameters
        now = int(datetime.now().timestamp())
        one_hour_ago = int((datetime.now() - timedelta(hours=1)).timestamp())

        body = TimeseriesFormulaRequest(
            data=TimeseriesFormulaQueryRequest(
                type=TimeseriesFormulaRequestType.TIMESERIES_REQUEST,
                attributes=TimeseriesFormulaRequestAttributes(
                    formulas=[{"formula": "query1"}],
                    queries=[{
                        "name": "query1",
                        "data_source": "metrics",
                        "query": "avg:system.cpu.user{*}"
                    }],
                    _from=one_hour_ago,
                    to=now
                )
            )
        )

        try:
            result = api_instance.query_timeseries_data(body=body)
            print(result)
        except Exception as e:
            print(f"Error: {e}")

if __name__ == "__main__":
    query_metrics()
```

**Installation**: `pip install datadog-api-client`

#### Java Example (using com.datadoghq:datadog-api-client)

```java
package com.datadog.api.example;

import com.datadog.api.client.ApiClient;
import com.datadog.api.client.ApiException;
import com.datadog.api.client.v2.api.MetricsApi;
import com.datadog.api.client.v2.model.*;
import java.time.Instant;
import java.time.temporal.ChronoUnit;
import java.util.Collections;

public class MetricsQueryExample {
    public static void main(String[] args) {
        // Validate environment variables
        String apiKey = System.getenv("DD_API_KEY");
        String appKey = System.getenv("DD_APP_KEY");
        String site = System.getenv().getOrDefault("DD_SITE", "datadoghq.com");

        if (apiKey == null || appKey == null) {
            System.err.println("Error: DD_API_KEY and DD_APP_KEY must be set");
            System.exit(1);
        }

        // Configure API client
        ApiClient apiClient = ApiClient.getDefaultApiClient();
        apiClient.setServerVariableValue("site", site);
        apiClient.configureApiKeys(Collections.singletonMap("apiKeyAuth", apiKey));
        apiClient.configureApiKeys(Collections.singletonMap("appKeyAuth", appKey));

        try {
            queryMetrics(apiClient);
        } catch (ApiException e) {
            System.err.println("API Error: " + e.getMessage());
            e.printStackTrace();
        }
    }

    private static void queryMetrics(ApiClient apiClient) throws ApiException {
        MetricsApi apiInstance = new MetricsApi(apiClient);

        // Time range: last hour
        long now = Instant.now().getEpochSecond();
        long oneHourAgo = Instant.now().minus(1, ChronoUnit.HOURS).getEpochSecond();

        // Build query
        TimeseriesFormulaQueryRequest query = new TimeseriesFormulaQueryRequest()
            .type(TimeseriesFormulaRequestType.TIMESERIES_REQUEST)
            .attributes(new TimeseriesFormulaRequestAttributes()
                .formulas(Collections.singletonList(new QueryFormula().formula("query1")))
                .queries(Collections.singletonList(
                    new MetricsTimeseriesQuery()
                        .name("query1")
                        .dataSource(MetricsDataSource.METRICS)
                        .query("avg:system.cpu.user{*}")
                ))
                .from(oneHourAgo)
                .to(now)
            );

        TimeseriesFormulaRequest body = new TimeseriesFormulaRequest().data(query);

        // Execute query
        TimeseriesFormulaResponse result = apiInstance.queryTimeseriesData(body);
        System.out.println(result);
    }
}
```

**Installation**: Add to `pom.xml`:
```xml
<dependency>
    <groupId>com.datadoghq</groupId>
    <artifactId>datadog-api-client</artifactId>
    <version>2.30.0</version>
</dependency>
```

#### Go Example (using github.com/DataDog/datadog-api-client-go)

```go
package main

import (
    "context"
    "encoding/json"
    "fmt"
    "os"
    "time"

    datadog "github.com/DataDog/datadog-api-client-go/v2/api/datadog"
    "github.com/DataDog/datadog-api-client-go/v2/api/datadogV2"
)

func main() {
    // Validate environment variables
    apiKey := os.Getenv("DD_API_KEY")
    appKey := os.Getenv("DD_APP_KEY")

    if apiKey == "" || appKey == "" {
        fmt.Println("Error: DD_API_KEY and DD_APP_KEY must be set")
        os.Exit(1)
    }

    // Configure API client
    ctx := context.WithValue(
        context.Background(),
        datadog.ContextAPIKeys,
        map[string]datadog.APIKey{
            "apiKeyAuth": {Key: apiKey},
            "appKeyAuth": {Key: appKey},
        },
    )

    configuration := datadog.NewConfiguration()
    apiClient := datadog.NewAPIClient(configuration)
    api := datadogV2.NewMetricsApi(apiClient)

    // Time range: last hour
    now := time.Now().Unix()
    oneHourAgo := time.Now().Add(-1 * time.Hour).Unix()

    // Build query
    body := datadogV2.TimeseriesFormulaRequest{
        Data: datadogV2.TimeseriesFormulaQueryRequest{
            Type: datadogV2.TIMESERIESFORMULAREQUESTTYPE_TIMESERIES_REQUEST,
            Attributes: datadogV2.TimeseriesFormulaRequestAttributes{
                Formulas: []datadogV2.QueryFormula{
                    {Formula: "query1"},
                },
                Queries: []datadogV2.TimeseriesQuery{
                    datadogV2.MetricsTimeseriesQuery{
                        Name:       datadog.PtrString("query1"),
                        DataSource: datadogV2.METRICSDATASOURCE_METRICS,
                        Query:      "avg:system.cpu.user{*}",
                    },
                },
                From: oneHourAgo,
                To:   now,
            },
        },
    }

    // Execute query
    result, _, err := api.QueryTimeseriesData(ctx, body)
    if err != nil {
        fmt.Printf("Error: %v\n", err)
        os.Exit(1)
    }

    jsonData, _ := json.MarshalIndent(result, "", "  ")
    fmt.Println(string(jsonData))
}
```

**Installation**: `go get github.com/DataDog/datadog-api-client-go/v2`

#### Rust Example (using datadog-api-client)

```rust
use datadog_api_client::datadog;
use datadog_api_client::datadogV2::api_metrics::MetricsAPI;
use datadog_api_client::datadogV2::model::*;
use std::collections::HashMap;

#[tokio::main]
async fn main() {
    // Validate environment variables
    let api_key = std::env::var("DD_API_KEY")
        .expect("DD_API_KEY must be set");
    let app_key = std::env::var("DD_APP_KEY")
        .expect("DD_APP_KEY must be set");

    // Configure API client
    let mut configuration = datadog::Configuration::new();
    configuration.api_key = Some(HashMap::from([
        ("apiKeyAuth".to_string(), api_key),
        ("appKeyAuth".to_string(), app_key),
    ]));

    let api = MetricsAPI::with_config(configuration);

    // Time range: last hour
    let now = chrono::Utc::now().timestamp();
    let one_hour_ago = (chrono::Utc::now() - chrono::Duration::hours(1)).timestamp();

    // Build query
    let body = TimeseriesFormulaRequest::new(
        TimeseriesFormulaQueryRequest::new(
            TimeseriesFormulaRequestAttributes::new(
                vec![QueryFormula::new("query1".to_string())],
                one_hour_ago,
                vec![
                    TimeseriesQuery::MetricsTimeseriesQuery(Box::new(
                        MetricsTimeseriesQuery::new(
                            MetricsDataSource::METRICS,
                            "avg:system.cpu.user{*}".to_string(),
                        )
                        .name("query1".to_string())
                    ))
                ],
                now,
            ),
            TimeseriesFormulaRequestType::TIMESERIES_REQUEST,
        )
    );

    // Execute query
    match api.query_timeseries_data(body).await {
        Ok(result) => {
            println!("{:#?}", result);
        }
        Err(err) => {
            eprintln!("Error: {}", err);
        }
    }
}
```

**Installation**: Add to `Cargo.toml`:
```toml
[dependencies]
datadog-api-client = "0.3"
tokio = { version = "1", features = ["full"] }
chrono = "0.4"
```

## Example Interactions

### Example 1: Quick Metrics Query

**User**: "Show me CPU usage for the last hour"

**Response**:
```bash
# Execute pup command for immediate results
pup metrics query --query="avg:system.cpu.user{*}" --from="1h" --to="now" --output=table
```

### Example 2: Code Generation for Application

**User**: "Generate Python code to query error logs"

**Response**:
```
Here's Python code to query error logs using the Datadog API client:

[Provide Python code example using datadog-api-client]

To use this code:
1. Save to a file: `query_logs.py`
2. Install: `pip install datadog-api-client`
3. Set environment variables: `export DD_API_KEY="..." DD_APP_KEY="..."`
4. Run: `python query_logs.py`

For immediate results without writing code, you can also use:
pup logs search --query="status:error" --from="1h"
```

### Example 3: Monitor Management

**User**: "List all production monitors"

**Response**:
```bash
# Using pup CLI
pup monitors list --tags="env:production" --output=table

# Or generate code for your application (specify language: typescript, python, java, go, rust)
```

## When to Use Each Approach

### Use Pup CLI When:
- User wants immediate results
- Exploring/experimenting with Datadog
- One-off queries or operations
- Quick troubleshooting
- Testing queries before coding

### Generate Code When:
- User asks to "generate code" or "create a script"
- Integrating into an application
- Automating recurring operations
- Building custom tools or dashboards
- User specifies a programming language

## Best Practices

1. **Start with pup for exploration**: Use pup to test queries before generating code
2. **Match the user's language**: If they mention TypeScript, Python, Java, Go, or Rust, use that language
3. **Provide complete examples**: Include imports, error handling, and configuration
4. **Explain authentication**: Always mention DD_API_KEY, DD_APP_KEY, DD_SITE
5. **Security reminders**: Warn about not committing credentials to version control
6. **Show both approaches**: Mention pup for quick testing + code for integration

## Integration with Agents

This skill works with all 46 domain agents in the plugin:
- Each agent describes Datadog functionality (logs, traces, metrics, monitors, etc.)
- Use pup commands that match the agent's domain
- Generate code using the corresponding Datadog API client methods

## Common User Phrases

- "Query [logs/metrics/traces]"
- "Generate code to..."
- "Show me [data type]"
- "Create a [monitor/dashboard/SLO]"
- "Write a [Python/TypeScript/Java/Go/Rust] script that..."
- "I need a script to..."
- "How do I integrate Datadog with..."

## Resources

- **Pup CLI**: `pup --help`
- **Pup Documentation**: [Pup CLI Repository](https://github.com/DataDog/pup)
- **TypeScript Client**: [@datadog/datadog-api-client](https://github.com/DataDog/datadog-api-client-typescript)
- **Python Client**: [datadog-api-client](https://github.com/DataDog/datadog-api-client-python)
- **Go Client**: [datadog-api-client-go](https://github.com/DataDog/datadog-api-client-go)
- **Java Client**: [datadog-api-client-java](https://github.com/DataDog/datadog-api-client-java)
- **Rust Client**: [datadog-api-client-rust](https://github.com/DataDog/datadog-api-client-rust)
- **API Documentation**: [Datadog API Reference](https://docs.datadoghq.com/api/latest/)

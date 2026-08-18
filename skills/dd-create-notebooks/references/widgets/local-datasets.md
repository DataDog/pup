# Notebook local datasets

Read this page only when the user needs SQL, transformation, derived fields,
joins, reusable intermediate data, or a visualization over derived notebook
data.

Canonical notebook `local_dataset` support is limited to:

- `timeseries`
- `toplist`
- `query_table`
- `treemap`
- `sunburst`
- `scatterplot`
- `wildcard`
- `bar_chart`
- `point_plot`

A local-dataset request has this shape:

```json
{
  "request_type": "local_dataset",
  "query": {
    "type": "structured_analysis",
    "source_dataset": "errors_by_service",
    "transformations": []
  },
  "response_format": "tabular"
}
```

The named source must be produced by an earlier analysis cell. Do not use this
shape with `log_stream`, `list_stream`, or any widget absent from the list.

## Use one complete analysis chain

For data source to SQL to visualization requests, keep every entry in the same
cell envelope. Never mix raw definitions with enveloped cells. Choose one
visualization when the user says “toplist or table.”

```json
[
  {
    "type": "notebook_cells",
    "attributes": {
      "definition": {
        "type": "analysis_data_source",
        "query": {
          "data_source": "logs",
          "name": "error_logs",
          "columns": [
            {"column": "service", "type": "string"},
            {"column": "message", "type": "string"}
          ],
          "search": {"query": "service:checkout status:error"},
          "indexes": []
        }
      }
    }
  },
  {
    "type": "notebook_cells",
    "attributes": {
      "definition": {
        "type": "analysis_sql",
        "query": {
          "data_source": "analysis_dataset",
          "name": "errors_by_service",
          "query": {
            "type": "sql_analysis",
            "sql_query": "SELECT service, COUNT(*) AS error_count FROM error_logs GROUP BY service"
          }
        }
      }
    }
  },
  {
    "type": "notebook_cells",
    "attributes": {
      "definition": {
        "type": "toplist",
        "requests": [
          {
            "request_type": "local_dataset",
            "response_format": "tabular",
            "query": {
              "type": "structured_analysis",
              "source_dataset": "errors_by_service",
              "transformations": [
                {"type": "filter", "filter": ""},
                {
                  "type": "aggregation",
                  "compute": [{"aggregation": "sum", "column": "error_count"}],
                  "group_by": [{"column": "service"}]
                }
              ]
            }
          }
        ]
      }
    }
  }
]
```

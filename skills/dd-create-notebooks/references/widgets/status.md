# Status and service widgets

Choose from user intent:

- monitor history: `alert_graph`
- current monitor value: `alert_value`
- check health: `check_status`
- monitor summary: `manage_status`
- one SLO: `slo`
- multiple SLOs: `slo_list`
- service or resource topology: `topology_map`
- service summary: `trace_service`

After choosing one type, read its exact reference from
[the widget type index](../widget-types.md).

These widgets use specialized product identifiers and schemas. Do not replace
their fields with generic metric `requests` unless the selected type reference requires
them. Do not invent IDs.

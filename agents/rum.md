---
description: Query Real User Monitoring events and analytics.
---

# Real User Monitoring Agent

Use the typed `rum` commands for RUM event search, aggregation, and session replay queries.

```bash
pup rum events --query="@type:error" --from="1h" --to="now"
pup rum aggregate --query="@type:view" --compute=count --from="1h" --to="now"
pup rum sessions search --query="@session.has_replay:true" --from="1d"
pup rum sessions list --from="1h"
pup rum replay segments get --session-id=<session-id> --view-id=<view-id>
pup rum playlists list
pup rum playlists sessions list <playlist-id>
```

The typed CLI does not provide a top-level RUM `search` command. Use `rum events`, `rum aggregate`, or `rum sessions search` for the corresponding supported data. Playlist session actions use separate `add`, `remove`, `bulk-remove`, and `list` subcommands; do not document pipe-separated pseudo-commands.

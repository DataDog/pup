# Agent Templates

This directory contains reusable template sections for Datadog API agents. These templates reduce duplication across the 46 agent files and ensure consistency.

## Purpose

Each agent file shares common sections like environment variables, time formats, and permission models. Instead of duplicating these sections 46 times, we maintain them here as templates.

## Available Templates

### 1. `pup-context.md` - CLI Tool Context
Common context about the pup CLI tool and environment variables.

**Used in**: All agents
**Contains**:
- Project location
- CLI tool description
- Required environment variables (DD_API_KEY, DD_APP_KEY, DD_SITE)

### 2. `time-formats.md` - Time Format Documentation
Comprehensive documentation for `--from` and `--to` time parameters.

**Used in**: logs, metrics, traces, rum, security, events, and other time-based query agents
**Contains**:
- Relative time format (1h, 30m, 2d, etc.)
- Unix timestamps
- ISO date format
- "now" keyword

### 3. `permission-model-read.md` - Read-Only Operations
Permission documentation for read-only agents.

**Used in**: logs, metrics, traces, rum, security, events, infrastructure, etc.
**Contains**:
- READ operations description
- Automatic execution note

### 4. `permission-model-write.md` - Write Operations
Permission documentation for agents with write capabilities.

**Used in**: monitors, dashboards, slos, synthetics, incidents, etc.
**Contains**:
- WRITE operations description
- Confirmation requirement note

### 5. `permission-model-mixed.md` - Mixed Operations
Permission documentation for agents with both read and write operations.

**Used in**: monitoring-alerting, log-configuration, apm-configuration, etc.
**Contains**:
- Both READ and WRITE sections
- Clear separation of automatic vs confirmation-required operations

## Usage in Agent Files

Agents reference these templates in their "Important Context" and "Permission Model" sections:

**Before (with templates)**:
```markdown
## Important Context

{{template: pup-context}}

## Time Format Options

{{template: time-formats}}

## Permission Model

{{template: permission-model-read}}
```

**Current (without templates)**:
```markdown
## Important Context

**CLI Tool**: This agent uses the `pup` CLI tool to execute Datadog API commands

**Environment Variables Required**:
- `DD_API_KEY`: Datadog API key
- `DD_APP_KEY`: Datadog Application key
- `DD_SITE`: Datadog site (default: datadoghq.com)

## Time Format Options

When using `--from` and `--to` parameters, you can use:
- **Relative time**: `1h`, `30m`, `2d`, `3600s` (hours, minutes, days, seconds ago)
- **Unix timestamp**: `1704067200`
- **"now"**: Current time
- **ISO date**: `2024-01-01T00:00:00Z`

## Permission Model

### READ Operations (Automatic)
- Listing/querying data
- Viewing content

These operations execute automatically without prompting.
```

## Implementation Strategy

### Phase 1: Extract Templates (Current)
- ✅ Create template directory
- ✅ Extract common sections to template files
- ✅ Document template usage

### Phase 2: Validate Templates (Next)
- Test templates with 2-3 sample agents
- Ensure consistency and completeness
- Adjust as needed

### Phase 3: Rollout (Future)
- Refactor all 46 agents to use templates
- Maintain domain-specific content in each agent
- Reduce total agent content by ~30-50%

## Benefits

1. **Consistency**: All agents use identical wording for common sections
2. **Maintainability**: Update once in template, applies to all agents
3. **Clarity**: Agent files focus on domain-specific content
4. **Size Reduction**: ~15,000 lines removed from agent files

## Template Adoption

| Template | Agents Using | Status |
|----------|--------------|--------|
| pup-context.md | 46 | ⏳ Pending |
| time-formats.md | ~35 | ⏳ Pending |
| permission-model-read.md | ~25 | ⏳ Pending |
| permission-model-write.md | ~15 | ⏳ Pending |
| permission-model-mixed.md | ~6 | ⏳ Pending |

## Future Enhancements

- Add template for common response formatting guidance
- Create template for pup CLI error handling
- Consider templates for common query examples
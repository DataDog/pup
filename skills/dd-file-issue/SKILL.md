---
name: dd-file-issue
description: File GitHub issues to the right repository (pup CLI or plugin)
trigger: |
  Use this skill when the user wants to:
  - Report a bug
  - Request a feature
  - File an issue
  - Submit a problem report
  - Report something not working
  - Suggest an improvement
examples:
  - "This isn't working, file an issue"
  - "Can you create a bug report for this?"
  - "I want to request a new feature"
  - "Report this error to the team"
---

# DD File Issue Skill

This skill helps you file GitHub issues to the correct repository. All issues — whether related to the `pup` CLI, skills, agents, or documentation — are tracked in `DataDog/pup`.

**Invocation**: `/dd-file-issue`

## Decision Logic: Which Repository?

### File to `DataDog/pup` Repository
All issues are filed here, including:
- **API Functionality**: API calls failing, incorrect responses, missing API endpoints
- **Authentication**: OAuth2 issues, token refresh problems, API key handling
- **CLI Behavior**: Command syntax, flags not working, output format issues
- **pup Installation**: Binary installation, PATH issues, version problems
- **API Coverage**: Missing Datadog API endpoints, incomplete command coverage
- **Performance**: Slow API calls, timeout issues
- **Error Messages**: Confusing pup error messages, API errors
- **Agent Behavior**: Agent selection, agent prompts, agent responses
- **Documentation**: README, CLAUDE.md, AGENTS.md, agent files inaccurate
- **Agent Prompts**: Agent giving wrong guidance, unclear instructions
- **Skill Issues**: Skills not working correctly
- **Agent Selection**: Claude picking wrong agent for task
- **Examples/Guides**: Missing or incorrect examples in agent files
- **User Experience**: Confusing workflows, unclear instructions

**Example Issues**:
- "pup logs search returns 404 error"
- "OAuth token refresh not working"
- "Need support for new Datadog API endpoint"
- "pup --output=json returns invalid JSON"
- "pup auth login fails on Linux"
- "logs agent doesn't explain query syntax clearly"
- "Agent tells me to use wrong pup command"
- "Documentation says to use TypeScript but we use pup now"
- "Need better examples for metrics queries"
- "Agent selection guide is confusing"

## Workflow

1. **Gather Information**
   - What went wrong?
   - What were you trying to do?
   - What error message did you see (if any)?
   - What agent/command were you using?

2. **Determine Repository**
   - All issues go to `DataDog/pup`

3. **Check for Existing Issues**
   - Search the target repository for similar issues
   - If found, suggest commenting on existing issue instead

4. **Collect Issue Details**
   - Title: Clear, concise summary (e.g., "logs agent: incorrect time format example")
   - Body: Detailed description with:
     - What happened
     - Expected behavior
     - Steps to reproduce
     - Environment details (OS, pup version, plugin version)
     - Relevant logs/errors

5. **Select Issue Type**
   - Bug report
   - Feature request
   - Documentation improvement
   - Question

6. **Create Issue**
   - Use `gh issue create` to file the issue
   - Add appropriate labels
   - Provide issue URL to user

## Implementation

### 1. Determine Repository

All issues are filed against `DataDog/pup`. Ask clarifying questions to gather detail:

```markdown
To file this issue correctly, I need to understand:

1. What were you trying to do?
2. What went wrong?
3. Was this a problem with:
   - A pup command not working?
   - An agent giving bad guidance?
   - Documentation being wrong/unclear?
```

### 2. Search for Existing Issues

```bash
gh issue list \
  --repo DataDog/pup \
  --search "your search terms" \
  --limit 10
```

### 3. Create Issue

```bash
gh issue create \
  --repo DataDog/pup \
  --title "Clear, concise title" \
  --body "Detailed description" \
  --label "bug" # or "enhancement", "documentation"
```

### Issue Body Template

Use this template for comprehensive issue reports:

```markdown
## Description
[Clear description of the issue]

## Expected Behavior
[What should happen]

## Actual Behavior
[What actually happened]

## Steps to Reproduce
1. [First step]
2. [Second step]
3. [...]

## Environment
- OS: [e.g., macOS 14.1, Ubuntu 22.04]
- Pup version: [run `pup --version`]
- Plugin version: [from plugin.json]
- Claude Code version: [if relevant]

## Error Messages / Logs
```
[Paste any error messages or relevant logs]
```

## Additional Context
[Any other relevant information]

## Suggested Fix (Optional)
[If you have ideas for how to fix this]
```

## Best Practices

### Good Issue Titles
✅ Good:
- "logs agent: incorrect time format documentation"
- "pup metrics query: --from flag ignores relative time"
- "Agent selection guide: missing decision tree for security agents"

❌ Bad:
- "It doesn't work"
- "Bug"
- "Help"

### When to Bundle Issues
- **Don't bundle** unrelated issues - file separately
- **Do bundle** if multiple issues stem from same root cause
- **Do bundle** if documenting several examples of same problem

### Labels to Use

**For pup repository**:
- `bug` - Something isn't working
- `enhancement` - New feature or request
- `documentation` - Improvements or additions to documentation
- `agent` - Related to specific agent behavior
- `skill` - Related to skills
- `question` - Further information is requested

## Examples

### Example 1: Filing a pup Bug

**User**: "The pup logs search command times out after 30 seconds"

**Analysis**: This is a pup CLI issue (command behavior)

**Steps**:
1. Search existing issues: `gh issue list --repo DataDog/pup --search "timeout"`
2. If not found, create issue:

```bash
gh issue create \
  --repo DataDog/pup \
  --title "pup logs search: command times out after 30 seconds" \
  --body "## Description
The \`pup logs search\` command times out after 30 seconds when searching large time ranges.

## Expected Behavior
Should either complete the query or allow configurable timeout.

## Actual Behavior
Command fails with timeout error after 30 seconds.

## Steps to Reproduce
1. Run: \`pup logs search --query='*' --from='7d' --to='now'\`
2. Wait 30 seconds
3. See timeout error

## Environment
- OS: macOS 14.1
- Pup version: 1.2.3
- DD_SITE: datadoghq.com

## Error Message
\`\`\`
Error: request timeout after 30000ms
\`\`\`
" \
  --label "bug"
```

### Example 2: Filing a Documentation Issue

**User**: "The logs agent documentation shows TypeScript examples instead of pup commands"

**Analysis**: This is a documentation issue

**Steps**:
1. Search existing issues: `gh issue list --repo DataDog/pup --search "TypeScript"`
2. Create issue:

```bash
gh issue create \
  --repo DataDog/pup \
  --title "logs agent: outdated TypeScript examples in documentation" \
  --body "## Description
The logs agent documentation still shows TypeScript/Node.js examples instead of pup CLI commands.

## Location
\`agents/logs.md\` lines 150-200

## Expected
Should show pup CLI command examples like:
\`\`\`bash
pup logs search --query='*' --from='1h'
\`\`\`

## Actual
Shows TypeScript code with imports and API clients

## Impact
Confusing for users who expect pup CLI commands

## Files Affected
- agents/logs.md
- Possibly other agent files (need audit)
" \
  --label "documentation" \
  --label "agent"
```

### Example 3: Feature Request

**User**: "Can we add an agent for Datadog SLO reporting?"

**Analysis**: This is a pup enhancement (new agent)

```bash
gh issue create \
  --repo DataDog/pup \
  --title "enhancement: add SLO reporting agent" \
  --body "## Feature Request

### Description
Add a new agent focused on SLO reporting and analysis, separate from the SLO management agent.

### Use Cases
- Generate SLO reports for stakeholders
- Analyze SLO trends over time
- Compare SLOs across services

### Proposed Agent Capabilities
- Query SLO history
- Generate formatted reports
- Calculate SLO burn rates
- Identify at-risk SLOs

### Related Agents
- Current \`slos.md\` focuses on SLO CRUD operations
- New agent would focus on analytics and reporting

### Priority
Medium - would improve SLO workflow
" \
  --label "enhancement" \
  --label "agent"
```

## Error Handling

If issue creation fails:
1. Check network connectivity
2. Verify gh CLI is authenticated: `gh auth status`
3. Check repository permissions
4. Provide user with issue details to file manually

## Success Response

After successfully filing an issue:

```markdown
✅ Issue created successfully!

**Repository**: DataDog/pup
**Issue**: #123
**URL**: https://github.com/DataDog/pup/issues/123
**Title**: [issue title]

The team will review your issue and respond soon. You can:
- Track the issue at the URL above
- Add more details by commenting on the issue
- Subscribe to notifications for updates
```

## Tips for Users

1. **Be specific** - Include exact commands, error messages, agent names
2. **Provide context** - What were you trying to accomplish?
3. **Include environment details** - OS, versions, configuration
4. **One issue per report** - Don't bundle unrelated problems
5. **Check existing issues first** - Avoid duplicates
6. **Be patient** - Maintainers will respond when available

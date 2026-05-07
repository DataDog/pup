# Troubleshooting Guide

Common issues and solutions for Pup CLI.

## Authentication Issues

### OAuth2 Login Fails

**Symptoms:**
```
Error: failed to complete OAuth login
```

**Common causes:**

1. **Network connectivity**
   ```bash
   # Test connectivity to Datadog
   curl -I https://datadoghq.com

   # Check DNS resolution
   nslookup datadoghq.com
   ```

2. **Firewall blocking localhost**
   - Callback server needs to bind to `127.0.0.1:<random-port>`
   - Check firewall allows connections to localhost
   - Try temporarily disabling firewall

3. **Browser doesn't open**
   ```
   ⚠️  Could not open browser automatically
   Please open this URL manually: https://...
   ```
   - Copy URL and paste in browser manually
   - Check `$BROWSER` environment variable
   - Try setting: `export BROWSER=chrome`

4. **Port already in use**
   - CLI automatically tries random available port
   - If error persists, check for port conflicts:
   ```bash
   # List processes listening on local ports
   lsof -i -P | grep LISTEN | grep 127.0.0.1
   ```

**Solutions:**
```bash
# Try with verbose logging
pup --verbose auth login

# Specify site explicitly
pup --site=datadoghq.com auth login

# Check authentication status
pup auth status
```

### Token Refresh Fails

**Symptoms:**
```
Error: failed to refresh access token
⚠️  Token expired. Run 'pup auth refresh' or 'pup auth login'
```

**Causes:**
- Refresh token expired (30-day lifetime)
- Network connectivity lost
- OAuth client revoked
- Invalid stored tokens

**Solutions:**
```bash
# Try manual refresh
pup auth refresh

# If refresh fails, re-authenticate
pup auth logout
pup auth login

# Check stored tokens (debug only)
ls -la ~/.config/pup/tokens_*.json
```

### Keychain Access Denied

**macOS symptoms:**
```
Warning: keychain access denied, using file storage
```

**Solutions:**

1. **Grant keychain access:**
   - Open "Keychain Access" app
   - Search for "pup"
   - Right-click → "Get Info"
   - Grant access to pup binary

2. **Use fallback storage:**
   - Pup automatically falls back to a JSON file
   - Check: `~/.config/pup/tokens_<site>.json`
   - File permissions should be `0600`

### API Key Authentication Fails

**Symptoms:**
```
Error: authentication failed: 403 Forbidden
```

**Check environment variables:**
```bash
# Verify keys are set
echo $DD_API_KEY
echo $DD_APP_KEY
echo $DD_SITE

# Set if missing
export DD_API_KEY="your-api-key"
export DD_APP_KEY="your-app-key"
export DD_SITE="datadoghq.com"
```

**Validate keys:**
```bash
# Test with curl
curl -X GET "https://api.datadoghq.com/api/v1/validate" \
  -H "DD-API-KEY: ${DD_API_KEY}" \
  -H "DD-APPLICATION-KEY: ${DD_APP_KEY}"
```

## API Call Issues

### Rate Limiting

**Symptoms:**
```
Error: 429 Too Many Requests
Rate limit exceeded
```

**Solutions:**
- Wait before retrying
- Reduce number of concurrent requests
- Check your Datadog plan limits
- Use pagination with smaller page sizes

**Workaround:**
```bash
# Add delay between requests
for id in $(cat ids.txt); do
  pup monitors get "$id"
  sleep 1  # Wait 1 second between requests
done
```

### Timeout Errors

**Symptoms:**
```
Error: context deadline exceeded
Error: request timeout
```

**Causes:**
- Network latency
- Large result set
- Datadog API slow response

**Solutions:**
```bash
# Use pagination
pup monitors list --limit=100

# Use shorter time ranges
pup logs search --query="..." --from="30m"  # Instead of 24h

# Check network latency
ping api.datadoghq.com
```

### 404 Not Found

**Symptoms:**
```
Error: 404 Not Found
Resource not found: monitor 12345678
```

**Causes:**
- Resource deleted
- Wrong resource ID
- Wrong Datadog site
- Insufficient permissions

**Solutions:**
```bash
# Verify resource exists
pup monitors list | grep "12345678"

# Check you're on correct site
pup --verbose monitors get 12345678

# Try with different site
pup --site=datadoghq.eu monitors get 12345678
```

## Command Issues

### Command Not Found

**Symptoms:**
```
Error: unknown command "foo" for "pup"
```

**Solutions:**
```bash
# List available commands
pup --help

# Check command spelling
pup metrics --help

# Verify command exists
pup help metrics query
```

### Invalid Flags

**Symptoms:**
```
Error: unknown flag: --foo
```

**Solutions:**
```bash
# Check available flags
pup metrics query --help

# Common flag mistakes:
pup metrics query --query="..." --from="1h"  # Correct
pup metrics query -query="..." -from="1h"    # Wrong (single dash)
```

### Missing Required Flags

**Symptoms:**
```
Error: required flag "query" not set
```

**Solutions:**
```bash
# Check required flags in help
pup metrics query --help

# Provide required flags
pup metrics query --query="avg:system.cpu.user{*}" --from="1h"
```

## Build Issues

### Compilation Errors

**Symptoms:**
```
error[E0433]: failed to resolve: use of undeclared type
```

**Solutions:**
```bash
# Clean and rebuild
cargo clean
cargo build

# Update dependencies
cargo update
```

### Missing Dependencies

**Symptoms:**
```
error: failed to select a version for `some-crate`
```

**Solutions:**
```bash
# Update the lock file
cargo update

# Check dependency tree
cargo tree
```

### Test Failures

**Symptoms:**
```
test result: FAILED
```

**Solutions:**
```bash
# Run tests with verbose output
cargo test -- --nocapture

# Run specific test
cargo test test_oauth_flow

# Run tests in specific module
cargo test auth::

# Check test output
cargo test 2>&1 | less
```

## Output Issues

### JSON Parse Errors

**Symptoms:**
```
Error: invalid character '<' looking for beginning of value
```

**Causes:**
- HTML error response instead of JSON
- API returned non-JSON
- Corrupted response

**Solutions:**
```bash
# Check raw response
pup --verbose monitors list

# Try different output format
pup monitors list --output=yaml
```

### Table Formatting Issues

**Symptoms:**
- Columns misaligned
- Text truncated
- Wide output

**Solutions:**
```bash
# Use JSON for complete output
pup monitors list --output=json | jq .

# Specify custom fields
pup monitors list --fields="id,name,status"

# Use YAML for readability
pup monitors list --output=yaml
```

## Performance Issues

### Slow Commands

**Causes:**
- Large result sets
- Wide time ranges
- Network latency
- Datadog API slow response

**Solutions:**
```bash
# Use pagination
pup monitors list --limit=50

# Narrow time range
pup logs search --from="30m"  # Instead of 24h

# Filter results
pup monitors list --tag="env:prod"  # Instead of all
```

### High Memory Usage

**Causes:**
- Loading large result sets
- Not using pagination
- Processing too much data

**Solutions:**
```bash
# Use streaming/pagination
pup monitors list --limit=100

# Process in batches
for page in {0..10}; do
  pup monitors list --offset=$((page * 100)) --limit=100
done
```

## Debug Mode

Enable verbose logging to troubleshoot issues:

```bash
# Global verbose flag
pup --verbose <command>

# Set log level via env var
export PUP_LOG_LEVEL=debug
pup <command>

# Trace HTTP requests
export DD_DEBUG=true
pup --verbose <command>
```

**Verbose output includes:**
- HTTP request details
- API endpoint URLs
- Authentication method used
- Response status codes
- Error stack traces

## Configuration Issues

### Config File Not Loaded

**Check locations:**
```bash
# Default location
ls -la ~/.config/pup/config.yaml

# Custom location
pup --config=/path/to/config.yaml <command>

# Verify config syntax
cat ~/.config/pup/config.yaml | yq .
```

### Environment Variable Conflicts

**Precedence order:**
1. Command flags (highest)
2. Environment variables
3. Config file
4. Defaults (lowest)

**Debug config:**
```bash
# Show resolved config
pup --verbose auth status

# Check env vars
env | grep DD_
env | grep PUP_
```

## Getting Help

### Documentation

1. **Check command help:**
   ```bash
   pup --help
   pup metrics --help
   pup metrics query --help
   ```

2. **Read documentation:**
   - [README.md](../README.md)
   - [COMMANDS.md](COMMANDS.md)
   - [EXAMPLES.md](EXAMPLES.md)
   - [OAUTH2.md](OAUTH2.md)

3. **Check API docs:**
   - [Datadog API Reference](https://docs.datadoghq.com/api/latest/)

### Reporting Issues

When opening a GitHub issue, include:

1. **Pup version:**
   ```bash
   pup --version
   ```

2. **Command that failed:**
   ```bash
   pup --verbose <command>
   ```

3. **Environment info:**
   ```bash
   # OS version
   uname -a

   # Rust version
   rustc --version

   # Environment variables (redact keys!)
   env | grep DD_SITE
   ```

4. **Error message and stack trace**
5. **Steps to reproduce**
6. **Expected vs actual behavior**

### Community Support

- **GitHub Issues:** [github.com/DataDog/pup/issues](https://github.com/DataDog/pup/issues)
- **Datadog Community:** [community.datadoghq.com](https://community.datadoghq.com/)

## Common Workarounds

### Bypass SSL Verification (Not Recommended)

Only for testing with self-signed certs:
```bash
export DD_SKIP_SSL_VALIDATION=true
pup <command>
```

### Use Proxy

```bash
export HTTP_PROXY=http://proxy.example.com:8080
export HTTPS_PROXY=http://proxy.example.com:8080
pup <command>
```

### Override API Endpoint

For testing or custom deployments:
```bash
export DD_HOST=https://custom-api.example.com
pup <command>
```

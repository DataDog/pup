//! AWS workload identity federation for Datadog Web APIs.
//!
//! The AWS SDK owns credential discovery and rotation. Datadog receives a signed
//! STS request, never the AWS secret key. Delegated tokens stay in memory and are
//! renewed with a new proof; they are not OAuth sessions or intake credentials.

use anyhow::{bail, Context, Result};
use aws_credential_types::provider::ProvideCredentials;
use aws_credential_types::Credentials;
use aws_sigv4::http_request::{sign, SignableBody, SignableRequest, SigningSettings};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, OnceCell};

const STS_BODY: &str = "Action=GetCallerIdentity&Version=2011-06-15";
const REFRESH_BUFFER: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct AwsWifAuth(Arc<State>);

struct State {
    org_uuid: String,
    sdk: OnceCell<aws_config::SdkConfig>,
    token: Mutex<Option<CachedToken>>,
}

struct CachedToken {
    api_base: String,
    access_token: String,
    expires: SystemTime,
}

impl AwsWifAuth {
    pub fn new(org_uuid: &str) -> Result<Self> {
        let org_uuid = uuid::Uuid::parse_str(org_uuid).context(
            "AWS authentication requires a valid DD_ORG_UUID (Datadog organization UUID)",
        )?;
        if org_uuid.is_nil() {
            bail!("DD_ORG_UUID must not be the nil UUID");
        }
        Ok(Self(Arc::new(State {
            org_uuid: org_uuid.to_string(),
            sdk: OnceCell::new(),
            token: Mutex::new(None),
        })))
    }

    pub fn org_uuid(&self) -> &str {
        &self.0.org_uuid
    }

    /// Serialize exchanges so concurrent API calls share one token. The cache is
    /// also bound to the API host: changing sites cannot reuse the old token.
    pub async fn token(&self, api_base: &str) -> Result<String> {
        let mut cached = self.0.token.lock().await;
        if let Some(token) = cached.as_ref() {
            if token.api_base == api_base && token.expires > SystemTime::now() + REFRESH_BUFFER {
                return Ok(token.access_token.clone());
            }
        }
        let sdk = self
            .0
            .sdk
            .get_or_init(|| async {
                aws_config::defaults(aws_config::BehaviorVersion::latest())
                    .region(
                        aws_config::meta::region::RegionProviderChain::default_provider()
                            .or_else("us-east-1"),
                    )
                    .load()
                    .await
            })
            .await;
        let credentials = sdk
            .credentials_provider()
            .context("AWS credential provider is unavailable")?
            .provide_credentials()
            .await
            // Provider errors can contain command output or credential material.
            .map_err(|_| {
                anyhow::anyhow!(
                    "could not resolve AWS credentials; check your AWS profile or workload role"
                )
            })?;
        let proof = signed_proof(
            &credentials,
            sdk.region().context("AWS region is unavailable")?.as_ref(),
            self.org_uuid(),
            SystemTime::now(),
        )?;
        let token = exchange(api_base, &proof).await?;
        let result = token.access_token.clone();
        *cached = Some(token);
        Ok(result)
    }

    pub fn middleware(&self, api_base: String, send_bearer: bool) -> AwsWifMiddleware {
        AwsWifMiddleware {
            auth: self.clone(),
            api_base,
            send_bearer,
        }
    }
}

/// Build the same Delegated proof accepted by the Datadog Go client. Include
/// the org UUID in the signature so the proof cannot target a different org.
fn signed_proof(
    credentials: &Credentials,
    region: &str,
    org_uuid: &str,
    now: SystemTime,
) -> Result<String> {
    if region.is_empty()
        || !region
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        bail!("invalid AWS region");
    }
    let suffix = if region.starts_with("cn-") {
        "amazonaws.com.cn"
    } else {
        "amazonaws.com"
    };
    let host = format!("sts.{region}.{suffix}");
    let url = format!("https://{host}/");
    let mut request = http::Request::builder()
        .method("POST")
        .uri(&url)
        .header("host", &host)
        .header(
            "content-type",
            "application/x-www-form-urlencoded; charset=utf-8",
        )
        .header("content-length", STS_BODY.len())
        .header("x-ddog-org-id", org_uuid)
        .body(STS_BODY)?;
    let identity = credentials.clone().into();
    let params = aws_sigv4::sign::v4::SigningParams::builder()
        .identity(&identity)
        .region(region)
        .name("sts")
        .time(now)
        .settings(SigningSettings::default())
        .build()?
        .into();
    let signable = SignableRequest::new(
        "POST",
        &url,
        request
            .headers()
            .iter()
            .map(|(k, v)| (k.as_str(), v.to_str().unwrap_or_default())),
        SignableBody::Bytes(STS_BODY.as_bytes()),
    )?;
    let (instructions, _) = sign(signable, &params)
        .map_err(|_| anyhow::anyhow!("could not sign AWS workload identity proof"))?
        .into_parts();
    instructions.apply_to_request_http1x(&mut request);
    let mut headers: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (name, value) in request.headers() {
        headers
            .entry(name.as_str())
            .or_default()
            .push(value.to_str()?);
    }
    Ok(format!(
        "{}|{}|POST|{}",
        STANDARD.encode(STS_BODY),
        STANDARD.encode(serde_json::to_vec(&headers)?),
        STANDARD.encode(url),
    ))
}

#[derive(Deserialize)]
struct TokenResponse {
    data: TokenData,
}

#[derive(Deserialize)]
struct TokenData {
    attributes: TokenAttributes,
}

#[derive(Deserialize)]
struct TokenAttributes {
    access_token: String,
    expires: String,
}

async fn exchange(api_base: &str, proof: &str) -> Result<CachedToken> {
    let mut authorization = http::HeaderValue::from_str(&format!("Delegated {proof}"))
        .map_err(|_| anyhow::anyhow!("invalid AWS workload identity proof"))?;
    authorization.set_sensitive(true);
    let response = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()?
        .post(format!("{api_base}/api/v2/delegated-token"))
        .header("Authorization", authorization)
        .header("Content-Type", "application/json")
        .header("User-Agent", crate::useragent::get())
        .body("")
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("AWS delegated-token exchange request failed"))?;
    if !response.status().is_success() {
        // Never surface response bodies: a proxy or server can echo the proof.
        bail!(
            "AWS delegated-token exchange failed (HTTP {}); check the Datadog site, organization UUID, AWS integration, and identity mapping",
            response.status().as_u16()
        );
    }
    let response: TokenResponse = response
        .json()
        .await
        .map_err(|_| anyhow::anyhow!("invalid delegated-token response"))?;
    let attributes = response.data.attributes;
    let seconds: u64 = attributes
        .expires
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid delegated-token expiration"))?;
    let expires = UNIX_EPOCH
        .checked_add(Duration::from_secs(seconds))
        .context("invalid delegated-token expiration")?;
    if expires <= SystemTime::now() + REFRESH_BUFFER {
        bail!("delegated token expires too soon; check the system clock");
    }
    if attributes.access_token.is_empty() {
        bail!("delegated-token response contains an empty access token");
    }
    // Validate before caching; callers must never panic while constructing headers.
    http::HeaderValue::from_str(&format!("Bearer {}", attributes.access_token))
        .map_err(|_| anyhow::anyhow!("invalid delegated-token access token"))?;
    Ok(CachedToken {
        api_base: api_base.to_string(),
        access_token: attributes.access_token,
        expires,
    })
}

pub struct AwsWifMiddleware {
    auth: AwsWifAuth,
    api_base: String,
    send_bearer: bool,
}

#[async_trait::async_trait]
impl reqwest_middleware::Middleware for AwsWifMiddleware {
    async fn handle(
        &self,
        mut req: reqwest::Request,
        extensions: &mut http::Extensions,
        next: reqwest_middleware::Next<'_>,
    ) -> reqwest_middleware::Result<reqwest::Response> {
        if !self.send_bearer {
            return Err(anyhow::anyhow!(
                "this endpoint requires API keys and cannot use AWS workload identity"
            )
            .into());
        }
        let token = Box::pin(self.auth.token(&self.api_base)).await?;
        let mut value = http::HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| anyhow::anyhow!("invalid delegated-token access token"))?;
        value.set_sensitive(true);
        // The generated SDK also loads ambient API keys independently of Config.
        // Explicit AWS mode must never send those alongside the delegated token.
        req.headers_mut().remove("DD-API-KEY");
        req.headers_mut().remove("DD-APPLICATION-KEY");
        req.headers_mut().insert(http::header::AUTHORIZATION, value);
        next.run(req, extensions).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_credential_types::provider::SharedCredentialsProvider;
    use mockito::Matcher;

    const ORG: &str = "11111111-2222-3333-4444-555555555555";

    fn credentials() -> Credentials {
        Credentials::new(
            "test-access",
            "test-secret",
            Some("test-session".into()),
            None,
            "test",
        )
    }

    fn provider() -> AwsWifAuth {
        provider_with(SharedCredentialsProvider::new(credentials()))
    }

    fn provider_with(credentials: SharedCredentialsProvider) -> AwsWifAuth {
        let auth = AwsWifAuth::new(ORG).unwrap();
        let sdk = aws_config::SdkConfig::builder()
            .credentials_provider(credentials)
            .region(aws_config::Region::new("us-east-1"))
            .build();
        auth.0.sdk.set(sdk).unwrap();
        auth
    }

    fn response(token: &str) -> String {
        serde_json::json!({"data": {"attributes": {
            "access_token": token,
            "expires": (SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + 900).to_string(),
        }}}).to_string()
    }

    #[test]
    fn proof_signs_org_and_session_and_uses_expected_wire_format() {
        let proof = signed_proof(
            &credentials(),
            "us-east-1",
            ORG,
            UNIX_EPOCH + Duration::from_secs(1_700_000_000),
        )
        .unwrap();
        let parts: Vec<_> = proof.split('|').collect();
        assert_eq!(parts.len(), 4);
        assert_eq!(STANDARD.decode(parts[0]).unwrap(), STS_BODY.as_bytes());
        assert_eq!(parts[2], "POST");
        assert_eq!(
            STANDARD.decode(parts[3]).unwrap(),
            b"https://sts.us-east-1.amazonaws.com/"
        );
        let headers: BTreeMap<String, Vec<String>> =
            serde_json::from_slice(&STANDARD.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(headers["x-ddog-org-id"], [ORG]);
        assert_eq!(headers["x-amz-security-token"], ["test-session"]);
        assert_eq!(headers["x-amz-date"], ["20231114T221320Z"]);
        let authorization = &headers["authorization"][0];
        assert!(
            authorization.contains("Credential=test-access/20231114/us-east-1/sts/aws4_request")
        );
        let signed = authorization
            .split("SignedHeaders=")
            .nth(1)
            .unwrap()
            .split(',')
            .next()
            .unwrap();
        assert!(signed.split(';').any(|name| name == "x-ddog-org-id"));
        assert!(signed.split(';').any(|name| name == "x-amz-security-token"));
        assert!(!proof.contains("test-secret"));
    }

    #[test]
    fn rejects_invalid_configuration_and_region() {
        assert!(AwsWifAuth::new("not-a-uuid").is_err());
        assert!(AwsWifAuth::new("00000000-0000-0000-0000-000000000000").is_err());
        for region in ["", "us-east-1/evil", "us-east-1.evil.example"] {
            assert!(signed_proof(&credentials(), region, ORG, SystemTime::now()).is_err());
        }
    }

    #[tokio::test]
    async fn concurrent_requests_share_token_and_renew_before_expiration() {
        let mut server = mockito::Server::new_async().await;
        let first = server
            .mock("POST", "/api/v2/delegated-token")
            .match_header(
                "authorization",
                Matcher::Regex(
                    "^Delegated [A-Za-z0-9+/=]+\\|[A-Za-z0-9+/=]+\\|POST\\|[A-Za-z0-9+/=]+$".into(),
                ),
            )
            .match_header("dd-api-key", Matcher::Missing)
            .match_header("dd-application-key", Matcher::Missing)
            .with_body(response("first-token"))
            .expect(1)
            .create_async()
            .await;
        let auth = provider();
        let base = server.url();
        let (one, two) = tokio::join!(auth.token(&base), auth.token(&base));
        assert_eq!(one.unwrap(), "first-token");
        assert_eq!(two.unwrap(), "first-token");
        first.assert_async().await;
        first.remove_async().await;

        auth.0.token.lock().await.as_mut().unwrap().expires =
            SystemTime::now() + Duration::from_secs(30);
        let renewed = server
            .mock("POST", "/api/v2/delegated-token")
            .with_body(response("renewed-token"))
            .expect(1)
            .create_async()
            .await;
        assert_eq!(auth.token(&base).await.unwrap(), "renewed-token");
        renewed.assert_async().await;
    }

    #[tokio::test]
    async fn exchange_failure_does_not_return_stale_token_or_echo_secrets() {
        let mut server = mockito::Server::new_async().await;
        let auth = provider();
        *auth.0.token.lock().await = Some(CachedToken {
            api_base: server.url(),
            access_token: "stale-secret".into(),
            expires: UNIX_EPOCH,
        });
        let mock = server
            .mock("POST", "/api/v2/delegated-token")
            .with_status(403)
            .with_body("echoed-secret-proof")
            .create_async()
            .await;
        let error = auth.token(&server.url()).await.unwrap_err().to_string();
        assert!(error.contains("HTTP 403"));
        assert!(!error.contains("secret"));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn rejects_invalid_responses_and_redirects() {
        let mut server = mockito::Server::new_async().await;
        for body in [
            "not JSON secret".to_string(),
            response(""),
            response("secret\ninvalid-header"),
            r#"{"data":{"attributes":{"access_token":"secret","expires":"invalid"}}}"#.into(),
            r#"{"data":{"attributes":{"access_token":"secret","expires":"0"}}}"#.into(),
            r#"{"data":{"attributes":{"access_token":"secret"}}}"#.into(),
        ] {
            let mock = server
                .mock("POST", "/api/v2/delegated-token")
                .with_body(body)
                .create_async()
                .await;
            let error = exchange(&server.url(), "test-proof")
                .await
                .err()
                .unwrap()
                .to_string();
            assert!(!error.contains("secret"));
            mock.assert_async().await;
            mock.remove_async().await;
        }
        let redirect = server
            .mock("POST", "/api/v2/delegated-token")
            .with_status(307)
            .with_header("location", &format!("{}/do-not-follow", server.url()))
            .create_async()
            .await;
        let destination = server
            .mock("POST", "/do-not-follow")
            .expect(0)
            .create_async()
            .await;
        assert!(exchange(&server.url(), "test-proof").await.is_err());
        redirect.assert_async().await;
        destination.assert_async().await;
    }

    #[tokio::test]
    async fn token_cache_is_bound_to_site() {
        let mut first = mockito::Server::new_async().await;
        let mut second = mockito::Server::new_async().await;
        let one = first
            .mock("POST", "/api/v2/delegated-token")
            .with_body(response("site-one"))
            .create_async()
            .await;
        let two = second
            .mock("POST", "/api/v2/delegated-token")
            .with_body(response("site-two"))
            .create_async()
            .await;
        let auth = provider();
        assert_eq!(auth.token(&first.url()).await.unwrap(), "site-one");
        assert_eq!(auth.token(&second.url()).await.unwrap(), "site-two");
        one.assert_async().await;
        two.assert_async().await;
    }

    #[derive(Debug)]
    struct RotatingCredentials(std::sync::atomic::AtomicUsize);

    impl ProvideCredentials for RotatingCredentials {
        fn provide_credentials<'a>(
            &'a self,
        ) -> aws_credential_types::provider::future::ProvideCredentials<'a>
        where
            Self: 'a,
        {
            let version = self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            aws_credential_types::provider::future::ProvideCredentials::ready(Ok(Credentials::new(
                format!("access-{version}"),
                "secret",
                Some(format!("session-{version}")),
                None,
                "rotating-test",
            )))
        }
    }

    #[tokio::test]
    async fn renewal_reloads_aws_credentials_instead_of_reusing_the_proof() {
        let mut server = mockito::Server::new_async().await;
        let auth = provider_with(SharedCredentialsProvider::new(RotatingCredentials(
            std::sync::atomic::AtomicUsize::new(0),
        )));
        for version in 0..2 {
            let mock = server
                .mock("POST", "/api/v2/delegated-token")
                .match_request(move |request| {
                    let Some(value) = request.header("authorization").first().copied() else {
                        return false;
                    };
                    let Some(proof) = value
                        .to_str()
                        .ok()
                        .and_then(|v| v.strip_prefix("Delegated "))
                    else {
                        return false;
                    };
                    let parts: Vec<_> = proof.split('|').collect();
                    let headers: serde_json::Value =
                        serde_json::from_slice(&STANDARD.decode(parts[1]).unwrap()).unwrap();
                    headers["x-amz-security-token"][0] == format!("session-{version}")
                })
                .with_body(response("rotated-token"))
                .create_async()
                .await;
            assert_eq!(auth.token(&server.url()).await.unwrap(), "rotated-token");
            mock.assert_async().await;
            mock.remove_async().await;
            auth.0.token.lock().await.as_mut().unwrap().expires = UNIX_EPOCH;
        }
    }

    #[tokio::test]
    async fn api_key_only_sdk_calls_fail_without_exchange_or_fallback() {
        let mut server = mockito::Server::new_async().await;
        let no_request = server
            .mock("GET", "/api/v2/validate_keys")
            .expect(0)
            .create_async()
            .await;
        let no_exchange = server
            .mock("POST", "/api/v2/delegated-token")
            .expect(0)
            .create_async()
            .await;
        let client = reqwest_middleware::ClientBuilder::new(reqwest::Client::new())
            .with(provider().middleware(server.url(), false))
            .build();
        let result = client
            .get(format!("{}/api/v2/validate_keys", server.url()))
            .header("DD-API-KEY", "unwanted-key")
            .send()
            .await;
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("cannot use AWS workload identity"));
        no_request.assert_async().await;
        no_exchange.assert_async().await;
    }

    #[tokio::test]
    async fn sdk_and_raw_http_use_and_renew_the_same_delegated_token() {
        let _lock = crate::test_support::lock_env().await;
        let mut server = mockito::Server::new_async().await;
        let mut cfg = crate::test_support::test_config(&server.url());
        cfg.api_key = None;
        cfg.app_key = None;
        let auth = provider();
        cfg.aws_wif_auth = Some(auth.clone());
        cfg.validate_auth().unwrap();
        let exchange = server
            .mock("POST", "/api/v2/delegated-token")
            .with_body(response("first-token"))
            .expect(1)
            .create_async()
            .await;
        let api = server
            .mock("GET", "/api/v1/monitor")
            .match_query(Matcher::Any)
            .match_header("authorization", "Bearer first-token")
            .match_header("dd-api-key", Matcher::Missing)
            .match_header("dd-application-key", Matcher::Missing)
            .with_body("[]")
            .expect(2)
            .create_async()
            .await;
        crate::commands::monitors::list(&cfg, None, None, 10, 0)
            .await
            .unwrap();
        crate::raw_client::raw_get(&cfg, "/api/v1/monitor", &[])
            .await
            .unwrap();
        api.assert_async().await;
        exchange.assert_async().await;
        exchange.remove_async().await;
        auth.0.token.lock().await.as_mut().unwrap().expires = UNIX_EPOCH;
        let renewed = server
            .mock("POST", "/api/v2/delegated-token")
            .with_body(response("second-token"))
            .create_async()
            .await;
        let api = server
            .mock("GET", "/api/v1/monitor")
            .match_query(Matcher::Any)
            .match_header("authorization", "Bearer second-token")
            .with_body("[]")
            .create_async()
            .await;
        crate::commands::monitors::list(&cfg, None, None, 10, 0)
            .await
            .unwrap();
        renewed.assert_async().await;
        api.assert_async().await;
        crate::test_support::cleanup_env();
    }
}

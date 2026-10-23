//! Hand-maintained authentication hook for the generated SDK client.

use async_trait::async_trait;
use http::Extensions;
use reqwest_middleware::{ClientBuilder, Middleware, Next};

use crate::config::Config;

impl Config {
    /// Keep provider selection outside generated code so new authentication
    /// methods do not require another generator change.
    pub fn configure_auth(&self, builder: ClientBuilder, send_bearer: bool) -> ClientBuilder {
        if let Some(auth) = &self.aws_wif_auth {
            return builder.with(auth.middleware(self.api_base_url(), send_bearer));
        }
        if send_bearer {
            if let Some(token) = self.access_token.as_ref() {
                return builder.with(BearerAuthMiddleware {
                    token: token.clone(),
                });
            }
        }
        builder
    }
}

struct BearerAuthMiddleware {
    token: String,
}

#[async_trait]
impl Middleware for BearerAuthMiddleware {
    async fn handle(
        &self,
        mut req: reqwest_middleware::reqwest::Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> reqwest_middleware::Result<reqwest_middleware::reqwest::Response> {
        req.headers_mut().insert(
            reqwest_middleware::reqwest::header::AUTHORIZATION,
            format!("Bearer {}", self.token).parse().unwrap(),
        );
        next.run(req, extensions).await
    }
}

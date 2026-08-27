use anyhow::{Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PullResult {
    NotModified,
    Updated {
        contents: String,
        etag: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct ConfigPuller {
    client: reqwest::Client,
    endpoint: String,
    token: String,
}

impl ConfigPuller {
    pub fn new(control_url: &str, node_id: &str, token: String) -> Result<Self> {
        validate_node_id(node_id)?;
        let endpoint = format!(
            "{}/api/v1/agent/config/{node_id}",
            control_url.trim_end_matches('/')
        );
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()?,
            endpoint,
            token,
        })
    }

    pub async fn pull(&self, etag: Option<&str>) -> Result<PullResult> {
        let mut request = self.client.get(&self.endpoint).bearer_auth(&self.token);
        if let Some(etag) = etag {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        let response = request.send().await.context("pull frps configuration")?;
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(PullResult::NotModified);
        }
        if !response.status().is_success() {
            anyhow::bail!("control server returned HTTP {}", response.status());
        }
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let contents = response.text().await?;
        Ok(PullResult::Updated { contents, etag })
    }
}

fn validate_node_id(node_id: &str) -> Result<()> {
    if node_id.is_empty()
        || node_id.len() > 64
        || !node_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        anyhow::bail!("node ID must contain only ASCII letters, digits, '-' or '_'");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_traversal_node_ids() {
        assert!(ConfigPuller::new("http://localhost", "../secret", "token".into()).is_err());
        assert!(ConfigPuller::new("http://localhost", "node-01", "token".into()).is_ok());
    }
}

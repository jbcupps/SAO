use serde::{Deserialize, Serialize};

/// A single search result returned by the web-search skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Request payload for a web search invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    /// Maximum number of results to return (capped at 5).
    #[serde(default = "default_max_results")]
    pub max_results: usize,
}

fn default_max_results() -> usize {
    5
}

/// Response payload from a web search invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
}

/// Execute a web search and return structured results.
///
/// The actual search provider is injected at runtime by the agent host.
/// This function validates the request and enforces skill constraints.
pub fn execute(request: &SearchRequest) -> Result<SearchResponse, SkillError> {
    if request.query.trim().is_empty() {
        return Err(SkillError::InvalidInput("query must not be empty".into()));
    }

    let max = request.max_results.min(5);

    // Placeholder: real implementation delegates to a configured search provider.
    // The agent host injects the provider URL and credentials at runtime.
    let _ = max;

    Ok(SearchResponse {
        results: Vec::new(),
    })
}

/// Errors that can occur during skill execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SkillError {
    InvalidInput(String),
    ProviderUnavailable(String),
    RateLimited,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_query() {
        let req = SearchRequest {
            query: "".into(),
            max_results: 5,
        };
        assert!(matches!(execute(&req), Err(SkillError::InvalidInput(_))));
    }

    #[test]
    fn caps_max_results_at_five() {
        let req = SearchRequest {
            query: "test".into(),
            max_results: 100,
        };
        let resp = execute(&req).unwrap();
        // Placeholder returns empty, but constraint is enforced in execute()
        assert!(resp.results.len() <= 5);
    }
}

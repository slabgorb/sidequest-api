//! Mock [`ClaudeLike`] for tests — scripted responses + call recording.
//!
//! Usage:
//! ```
//! use std::sync::Arc;
//! use sidequest_test_support::{ClaudeLike, MockClaudeClient};
//!
//! let mut mock = MockClaudeClient::new();
//! mock.respond_with("scripted reply");
//! let client: Arc<dyn ClaudeLike> = Arc::new(mock);
//! let resp = client.send_with_model("prompt", "haiku").unwrap();
//! assert_eq!(resp.text, "scripted reply");
//! ```

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use sidequest_agents::client::{ClaudeClientError, ClaudeLike, ClaudeResponse};

/// Scripted mock for [`ClaudeLike`].
///
/// FIFO script: `respond_with` pushes a response onto the back of the
/// queue; each send call pops from the front. Unscripted calls return
/// [`ClaudeClientError::EmptyResponse`] so tests fail loudly instead of
/// silently getting empty strings.
pub struct MockClaudeClient {
    script: Mutex<VecDeque<Result<ClaudeResponse, ClaudeClientError>>>,
    recorded: Mutex<Vec<RecordedCall>>,
}

/// One recorded invocation of the mock, in call order.
#[derive(Debug, Clone)]
pub struct RecordedCall {
    prompt: String,
    model: String,
    session_id: Option<String>,
    system_prompt: Option<String>,
    allowed_tools: Vec<String>,
    env_vars: HashMap<String, String>,
}

impl RecordedCall {
    /// The prompt text passed to the mock.
    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    /// The model identifier passed to the mock (e.g., "haiku", "opus").
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Session ID on `send_with_session` calls; `None` for `send_with_model`.
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// System prompt on `send_with_session` calls; always `None` for
    /// `send_with_model`.
    pub fn system_prompt(&self) -> Option<&str> {
        self.system_prompt.as_deref()
    }

    /// Allowed-tools list on `send_with_session` calls.
    pub fn allowed_tools(&self) -> &[String] {
        &self.allowed_tools
    }

    /// Environment variables on `send_with_session` calls.
    pub fn env_vars(&self) -> &HashMap<String, String> {
        &self.env_vars
    }
}

impl MockClaudeClient {
    /// Create an empty mock. Unscripted calls return
    /// [`ClaudeClientError::EmptyResponse`] — no silent Ok(empty).
    pub fn new() -> Self {
        Self {
            script: Mutex::new(VecDeque::new()),
            recorded: Mutex::new(Vec::new()),
        }
    }

    /// Script the next response to be an `Ok(ClaudeResponse { text, .. })`.
    ///
    /// Multiple calls queue up FIFO: the first scripted response is
    /// returned by the first send call, the second by the second, etc.
    pub fn respond_with(&mut self, text: impl Into<String>) {
        self.script
            .get_mut()
            .expect("mock mutex poisoned")
            .push_back(Ok(ClaudeResponse {
                text: text.into(),
                input_tokens: None,
                output_tokens: None,
                session_id: None,
            }));
    }

    /// Script the next response to be a specific [`ClaudeClientError`].
    pub fn respond_with_error(&mut self, err: ClaudeClientError) {
        self.script
            .get_mut()
            .expect("mock mutex poisoned")
            .push_back(Err(err));
    }

    /// All invocations of the mock, in call order. Available on `&self`
    /// so the mock can be queried through `Arc<dyn ClaudeLike>`.
    pub fn recorded_calls(&self) -> Vec<RecordedCall> {
        self.recorded.lock().expect("mock mutex poisoned").clone()
    }

    fn pop_script(&self) -> Result<ClaudeResponse, ClaudeClientError> {
        self.script
            .lock()
            .expect("mock mutex poisoned")
            .pop_front()
            .unwrap_or(Err(ClaudeClientError::EmptyResponse))
    }

    fn record(&self, call: RecordedCall) {
        self.recorded
            .lock()
            .expect("mock mutex poisoned")
            .push(call);
    }
}

impl Default for MockClaudeClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeLike for MockClaudeClient {
    fn send_with_model(
        &self,
        prompt: &str,
        model: &str,
    ) -> Result<ClaudeResponse, ClaudeClientError> {
        self.record(RecordedCall {
            prompt: prompt.to_string(),
            model: model.to_string(),
            session_id: None,
            system_prompt: None,
            allowed_tools: Vec::new(),
            env_vars: HashMap::new(),
        });
        self.pop_script()
    }

    fn send_with_session(
        &self,
        prompt: &str,
        model: &str,
        session_id: Option<&str>,
        system_prompt: Option<&str>,
        allowed_tools: &[String],
        env_vars: &HashMap<String, String>,
    ) -> Result<ClaudeResponse, ClaudeClientError> {
        self.record(RecordedCall {
            prompt: prompt.to_string(),
            model: model.to_string(),
            session_id: session_id.map(str::to_string),
            system_prompt: system_prompt.map(str::to_string),
            allowed_tools: allowed_tools.to_vec(),
            env_vars: env_vars.clone(),
        });
        self.pop_script()
    }
}

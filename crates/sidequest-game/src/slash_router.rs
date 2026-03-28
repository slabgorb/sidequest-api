//! Slash command router — intercepts `/command` input before intent classification.
//!
//! Commands are pure functions of game state and arguments. They never call the LLM.
//! The router sits upstream of the intent router in the input pipeline.

use std::collections::HashMap;

use crate::axis::AxisValue;
use crate::state::{GameSnapshot, WorldStatePatch};

/// Result of executing a slash command.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum CommandResult {
    /// Text response displayed to the player.
    Display(String),
    /// A state mutation to apply (for /gm commands).
    StateMutation(WorldStatePatch),
    /// An error message (unknown command, bad args, etc.).
    Error(String),
    /// Axis value change (for /tone commands). The full new set of axis values.
    ToneChange(Vec<AxisValue>),
}

/// Trait for slash command handlers.
///
/// Handlers are pure functions: they receive an immutable state reference
/// and return a `CommandResult`. No async, no LLM calls.
pub trait CommandHandler: Send + Sync {
    /// The command name (without the leading `/`).
    fn name(&self) -> &str;
    /// A short description for `/help` output.
    fn description(&self) -> &str;
    /// Execute the command with the given game state and argument string.
    fn handle(&self, state: &GameSnapshot, args: &str) -> CommandResult;
}

/// Routes `/command` input to registered handlers.
pub struct SlashRouter {
    commands: HashMap<String, Box<dyn CommandHandler>>,
}

impl SlashRouter {
    /// Create a new empty router.
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    /// Register a command handler. If a handler with the same name already
    /// exists, it is replaced.
    pub fn register(&mut self, handler: Box<dyn CommandHandler>) {
        let name = handler.name().to_string();
        self.commands.insert(name, handler);
    }

    /// Try to dispatch input as a slash command.
    ///
    /// Returns `None` if the input does not start with `/` (passthrough to
    /// intent router). Returns `Some(CommandResult)` for all slash input,
    /// including unknown commands.
    pub fn try_dispatch(&self, input: &str, state: &GameSnapshot) -> Option<CommandResult> {
        if !input.starts_with('/') {
            return None;
        }

        let (cmd, args) = Self::parse(input);

        // Built-in /help
        if cmd == "help" {
            return Some(self.help_output());
        }

        match self.commands.get(cmd) {
            Some(handler) => Some(handler.handle(state, args)),
            None => Some(CommandResult::Error(format!(
                "Unknown command: /{}",
                cmd
            ))),
        }
    }

    /// Parse slash input into (command_name, args).
    /// Input must start with '/'.
    fn parse(input: &str) -> (&str, &str) {
        let trimmed = &input[1..]; // skip '/'
        match trimmed.split_once(' ') {
            Some((cmd, args)) => (cmd, args.trim_start()),
            None => (trimmed, ""),
        }
    }

    /// Build /help output listing all registered commands.
    fn help_output(&self) -> CommandResult {
        if self.commands.is_empty() {
            return CommandResult::Display("No commands registered. Use /help after commands are added.".to_string());
        }

        let mut lines: Vec<String> = self
            .commands
            .iter()
            .map(|(name, handler)| format!("/{} — {}", name, handler.description()))
            .collect();
        lines.sort(); // deterministic output
        CommandResult::Display(lines.join("\n"))
    }
}

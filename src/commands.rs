use super::*;

#[derive(BotCommands)]
#[command(rename_rule = "lowercase")]
pub enum Command {
    /// Display all commands
    Help,
    /// Start
    Start,
    /// Revise
    Task,
}

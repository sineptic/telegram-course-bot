use super::*;

#[derive(BotCommands)]
#[command(rename_rule = "lowercase")]
pub enum Command {
    /// Revise
    Task,
    /// Display all commands
    Help,
    #[command(hide)]
    Start,
}

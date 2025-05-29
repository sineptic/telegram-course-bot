use teloxide::utils::command::ParseError;

use super::*;

fn non_empty(input: String) -> Result<(String,), ParseError> {
    let input = input.trim();
    check!(
        !input.is_empty(),
        ParseError::TooFewArguments {
            expected: 1,
            found: 0,
            message: "You should specify card name".into()
        }
    );
    Ok((input.to_owned(),))
}

#[derive(BotCommands)]
#[command(rename_rule = "lowercase", parse_with = "split")]
pub enum Command {
    /// Revise
    #[command(parse_with = non_empty)]
    Card(String),
    /// View course structure
    Graph,
    /// Display all commands
    Help,
    Revise,
    Clear,

    ChangeCourseGraph,
    ChangeDeque,

    #[command(hide)]
    Start,
}

use super::*;

pub fn make_keyboard<I, T>(buttons: I, random_id: u64) -> InlineKeyboardMarkup
where
    I: IntoIterator<Item = T>,
    T: AsRef<str>,
{
    InlineKeyboardMarkup::new(buttons.into_iter().map(|label| {
        [InlineKeyboardButton::callback(
            label.as_ref(),
            format!("{random_id} {}", label.as_ref()),
        )]
    }))
}

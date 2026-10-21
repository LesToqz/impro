//! Unicode text rules matching the source application's casefold/printable checks.
use unicode_casefold::UnicodeCaseFold;
use unicode_general_category::{get_general_category, GeneralCategory as C};
pub fn casefold(value: &str) -> String {
    value.case_fold().collect()
}
pub fn is_printable(c: char) -> bool {
    c == ' '
        || !matches!(
            get_general_category(c),
            C::Control
                | C::Format
                | C::Surrogate
                | C::PrivateUse
                | C::Unassigned
                | C::LineSeparator
                | C::ParagraphSeparator
                | C::SpaceSeparator
        )
}

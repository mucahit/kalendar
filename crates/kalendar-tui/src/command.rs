use crate::View;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Command {
    None,
    LoadView(View),
    MoveDays(i64),
    MoveMonth(i32),
    Today,
    Refresh,
}

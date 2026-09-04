use crate::View;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Action {
    Quit,
    SetView(View),
    MoveDays(i64),
    MoveMonth(i32),
    MoveTime(i16),
    ScrollHours(i16),
    CycleEvent(i32),
    Today,
    OpenSelected,
    NewEvent,
    EditSelected,
    DeleteSelected,
    OpenCalendars,
    OpenSearch,
    OpenHelp,
    Refresh,
    ClearSelection,
}

use crate::{
    Calendar, DateRange, DeleteScope, Event, EventId, EventPatch, NewEvent, PermissionStatus,
    RecurrenceScope,
};
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait CalendarBackend: Send + Sync {
    async fn permissions(&self) -> Result<PermissionStatus> {
        Ok(PermissionStatus::Granted)
    }

    async fn request_permissions(&self) -> Result<bool> {
        Ok(true)
    }

    async fn calendars(&self) -> Result<Vec<Calendar>>;
    async fn events(&self, range: DateRange) -> Result<Vec<Event>>;
    async fn event(&self, id: &EventId) -> Result<Option<Event>>;
    async fn create_event(&self, event: NewEvent) -> Result<Event>;
    async fn update_event(&self, id: &EventId, patch: EventPatch) -> Result<Event>;
    async fn update_event_scoped(
        &self,
        id: &EventId,
        patch: EventPatch,
        scope: RecurrenceScope,
    ) -> Result<Event> {
        match scope {
            RecurrenceScope::ThisEvent => self.update_event(id, patch).await,
            RecurrenceScope::ThisAndFuture | RecurrenceScope::AllEvents => {
                anyhow::bail!("this backend does not support scoped recurring-event updates")
            }
        }
    }
    async fn delete_event(&self, id: &EventId, scope: DeleteScope) -> Result<()>;
    async fn search(&self, query: &str, range: Option<DateRange>) -> Result<Vec<Event>>;
}

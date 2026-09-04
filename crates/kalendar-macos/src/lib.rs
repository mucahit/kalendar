use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use kalendar_core::{
    Calendar, CalendarBackend, DateRange, DeleteScope, Event, EventId, EventPatch, NewEvent,
    PermissionStatus, RecurrenceScope,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
pub struct MacOsBackend {
    client: BridgeClient,
}

impl MacOsBackend {
    pub fn discover() -> Result<Self> {
        Ok(Self {
            client: BridgeClient::discover()?,
        })
    }

    #[must_use]
    pub fn with_bridge(path: impl Into<PathBuf>) -> Self {
        Self {
            client: BridgeClient::new(path),
        }
    }
}

#[derive(Clone, Debug)]
struct BridgeClient {
    executable: PathBuf,
    process: Arc<Mutex<Option<BridgeProcess>>>,
}

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

impl BridgeClient {
    fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            process: Arc::new(Mutex::new(None)),
        }
    }

    fn discover() -> Result<Self> {
        if let Some(path) = std::env::var_os("KALENDAR_EVENTKIT_BRIDGE") {
            return Ok(Self::new(path));
        }
        let current = std::env::current_exe().context("locating the kalendar executable")?;
        let current = current.canonicalize().unwrap_or(current);
        let directory = current.parent().unwrap_or_else(|| Path::new("."));
        let candidates = [
            directory.join("kalendar-eventkit"),
            directory.join("../libexec/kalendar/kalendar-eventkit"),
            option_env!("KALENDAR_EVENTKIT_BUILD_PATH")
                .map(PathBuf::from)
                .unwrap_or_default(),
            PathBuf::from("native/macos-calendar-bridge/.build/release/kalendar-eventkit"),
            PathBuf::from("native/macos-calendar-bridge/.build/debug/kalendar-eventkit"),
        ];
        let executable = candidates
            .into_iter()
            .find(|path| path.is_file())
            .unwrap_or_else(|| directory.join("../libexec/kalendar/kalendar-eventkit"));
        Ok(Self::new(executable))
    }

    async fn request<T: DeserializeOwned>(&self, method: &str, params: Value) -> Result<T> {
        let id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        let request = Request { id, method, params };
        let payload = serde_json::to_vec(&request).context("encoding EventKit request")?;
        let mut process = self.process.lock().await;
        if let Some(active) = process.as_mut()
            && active
                .child
                .try_wait()
                .context("checking EventKit bridge status")?
                .is_some()
        {
            *process = None;
        }
        if process.is_none() {
            *process = Some(BridgeProcess::spawn(&self.executable)?);
        }
        let active = process
            .as_mut()
            .context("EventKit bridge process is unavailable")?;
        active
            .stdin
            .write_all(&payload)
            .await
            .context("writing EventKit request")?;
        active
            .stdin
            .write_all(b"\n")
            .await
            .context("terminating EventKit request")?;
        active
            .stdin
            .flush()
            .await
            .context("flushing EventKit request")?;
        let mut line = String::new();
        let bytes = active
            .stdout
            .read_line(&mut line)
            .await
            .context("reading EventKit response")?;
        if bytes == 0 {
            let status = active
                .child
                .wait()
                .await
                .context("waiting for failed EventKit bridge")?;
            *process = None;
            return Err(anyhow!("EventKit bridge exited unexpectedly with {status}"));
        }
        let response: Response<T> =
            serde_json::from_str(&line).context("decoding EventKit response")?;
        if response.id != id {
            return Err(anyhow!("EventKit bridge response id did not match request"));
        }
        if response.ok {
            response
                .result
                .ok_or_else(|| anyhow!("EventKit bridge returned an empty result"))
        } else {
            let error = response.error.unwrap_or(BridgeError {
                code: "unknown".into(),
                message: "Unknown EventKit error".into(),
            });
            Err(anyhow!("{}: {}", error.code, error.message))
        }
    }
}

#[derive(Debug)]
struct BridgeProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl BridgeProcess {
    fn spawn(executable: &Path) -> Result<Self> {
        let mut child = Command::new(executable)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| {
                format!(
                    "launching {}. Reinstall Kalendar with its libexec helper or set KALENDAR_EVENTKIT_BRIDGE",
                    executable.display()
                )
            })?;
        let stdin = child
            .stdin
            .take()
            .context("opening EventKit bridge stdin")?;
        let stdout = child
            .stdout
            .take()
            .context("opening EventKit bridge stdout")?;
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }
}

#[derive(Serialize)]
struct Request<'a> {
    id: u64,
    method: &'a str,
    params: Value,
}

#[derive(Deserialize)]
struct Response<T> {
    id: u64,
    ok: bool,
    result: Option<T>,
    error: Option<BridgeError>,
}

#[derive(Deserialize)]
struct BridgeError {
    code: String,
    message: String,
}

#[async_trait]
impl CalendarBackend for MacOsBackend {
    async fn permissions(&self) -> Result<PermissionStatus> {
        self.client.request("permissions", json!({})).await
    }

    async fn request_permissions(&self) -> Result<bool> {
        self.client.request("request_permissions", json!({})).await
    }

    async fn calendars(&self) -> Result<Vec<Calendar>> {
        self.client.request("calendars", json!({})).await
    }

    async fn events(&self, range: DateRange) -> Result<Vec<Event>> {
        self.client
            .request("events", serde_json::to_value(range)?)
            .await
    }

    async fn event(&self, id: &EventId) -> Result<Option<Event>> {
        self.client
            .request("event", json!({ "event_id": id }))
            .await
    }

    async fn create_event(&self, event: NewEvent) -> Result<Event> {
        self.client
            .request("create_event", serde_json::to_value(event)?)
            .await
    }

    async fn update_event(&self, id: &EventId, patch: EventPatch) -> Result<Event> {
        self.client
            .request(
                "update_event",
                json!({ "event_id": id, "patch": patch, "scope": RecurrenceScope::ThisEvent }),
            )
            .await
    }

    async fn update_event_scoped(
        &self,
        id: &EventId,
        patch: EventPatch,
        scope: RecurrenceScope,
    ) -> Result<Event> {
        self.client
            .request(
                "update_event",
                json!({ "event_id": id, "patch": patch, "scope": scope }),
            )
            .await
    }

    async fn delete_event(&self, id: &EventId, scope: DeleteScope) -> Result<()> {
        let _: Value = self
            .client
            .request("delete_event", json!({ "event_id": id, "scope": scope }))
            .await?;
        Ok(())
    }

    async fn search(&self, query: &str, range: Option<DateRange>) -> Result<Vec<Event>> {
        self.client
            .request("search", json!({ "query": query, "range": range }))
            .await
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn native_bridge_answers_protocol_without_requesting_permission() {
        let client = BridgeClient::discover().unwrap();
        let result: Value = client.request("ping", json!({})).await.unwrap();
        assert_eq!(result["version"], "0.1.0");

        let backend = MacOsBackend { client };
        assert!(matches!(
            backend.permissions().await.unwrap(),
            PermissionStatus::Granted | PermissionStatus::NotDetermined | PermissionStatus::Denied
        ));
    }
}

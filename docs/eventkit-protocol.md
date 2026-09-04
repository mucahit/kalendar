# EventKit bridge protocol

The Rust client and `kalendar-eventkit` exchange one newline-delimited JSON request and response per process. Every message has a numeric correlation ID.

## Envelope

```json
{"id":1,"method":"calendars","params":{}}
{"id":1,"ok":true,"result":[]}
```

Errors are structured and safe to show in the TUI:

```json
{"id":1,"ok":false,"error":{"code":"permission_denied","message":"Calendar access is not granted."}}
```

Dates are ISO-8601 strings with offsets. Field names are snake case. Optional event fields are JSON null when absent.

## Methods

| Method | Parameters | Result |
| --- | --- | --- |
| `ping` | `{}` | bridge version object |
| `permissions` | `{}` | `granted`, `not_determined`, or `denied` |
| `request_permissions` | `{}` | boolean |
| `calendars` | `{}` | calendar array |
| `events` | `{from,to}` | events overlapping the half-open range |
| `event` | `{event_id}` | one event |
| `create_event` | new-event fields | created event |
| `update_event` | `{event_id,patch,scope}` | updated event |
| `delete_event` | `{event_id,scope}` | empty object |
| `search` | `{query,range?}` | matching events |

Mutation scopes are `this_event`, `this_and_future`, and `all_events`. For recurring updates and deletes, the bridge rejects `all_events` with `unsupported_scope`, because EventKit cannot safely find occurrences before an arbitrary selected occurrence.

## Error codes

Stable codes currently include `invalid_request`, `invalid_params`, `unknown_method`, `permission_denied`, `not_found`, `read_only`, `unsupported_scope`, and `eventkit_error`.

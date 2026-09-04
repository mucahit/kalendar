import EventKit
import Foundation

private struct RpcFailure: Error {
    let code: String
    let message: String
}

private let store = EKEventStore()

private func isoDate(_ value: Any?) throws -> Date {
    guard let string = value as? String else {
        throw RpcFailure(code: "invalid_params", message: "Expected an ISO-8601 date.")
    }
    let withFraction = ISO8601DateFormatter()
    withFraction.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    let normal = ISO8601DateFormatter()
    guard let date = withFraction.date(from: string) ?? normal.date(from: string) else {
        throw RpcFailure(code: "invalid_params", message: "Invalid ISO-8601 date: \(string)")
    }
    return date
}

private func isoString(_ date: Date) -> String {
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    return formatter.string(from: date)
}

private func sourceKind(_ source: EKSource) -> Any {
    switch source.sourceType {
    case .local: return ["kind": "local"]
    case .calDAV:
        if source.title.localizedCaseInsensitiveContains("icloud") { return ["kind": "i_cloud"] }
        return ["kind": "cal_dav"]
    case .exchange: return ["kind": "exchange"]
    case .birthdays: return ["kind": "birthdays"]
    default: return ["kind": "other", "name": source.title]
    }
}

private func color(_ calendar: EKCalendar) -> [String: Int]? {
    guard let components = calendar.cgColor.components, components.count >= 3 else { return nil }
    return [
        "red": Int((components[0] * 255).rounded()),
        "green": Int((components[1] * 255).rounded()),
        "blue": Int((components[2] * 255).rounded()),
    ]
}

private func calendarJSON(_ calendar: EKCalendar) -> [String: Any] {
    var result: [String: Any] = [
        "id": calendar.calendarIdentifier,
        "name": calendar.title,
        "writable": calendar.allowsContentModifications,
        "source": sourceKind(calendar.source),
    ]
    result["color"] = color(calendar) ?? NSNull()
    return result
}

private func availability(_ value: EKEventAvailability) -> String {
    switch value {
    case .free: return "free"
    case .tentative: return "tentative"
    case .unavailable: return "unavailable"
    default: return "busy"
    }
}

private func eventJSON(_ event: EKEvent) -> [String: Any] {
    var result: [String: Any] = [
        "id": event.eventIdentifier ?? event.calendarItemIdentifier,
        "calendar_id": event.calendar.calendarIdentifier,
        "title": event.title ?? "(Untitled)",
        "start": isoString(event.startDate),
        "end": isoString(event.endDate),
        "all_day": event.isAllDay,
        "availability": availability(event.availability),
    ]
    result["location"] = event.location ?? NSNull()
    result["notes"] = event.notes ?? NSNull()
    result["url"] = event.url?.absoluteString ?? NSNull()
    result["recurrence"] = event.hasRecurrenceRules ? ["description": event.recurrenceRules?.map(\.description).joined(separator: ", ") ?? "Recurring"] : NSNull()
    return result
}

private func hasPermission() -> Bool {
    let status = EKEventStore.authorizationStatus(for: .event)
    if #available(macOS 14.0, *) { return status == .fullAccess }
    return status == .authorized
}

private func permissionStatus() -> String {
    let status = EKEventStore.authorizationStatus(for: .event)
    if #available(macOS 14.0, *) {
        if status == .fullAccess { return "granted" }
    } else if status == .authorized {
        return "granted"
    }
    return status == .notDetermined ? "not_determined" : "denied"
}

private func requestPermission() throws -> Bool {
    if hasPermission() { return true }
    let semaphore = DispatchSemaphore(value: 0)
    var granted = false
    var requestError: Error?
    if #available(macOS 14.0, *) {
        store.requestFullAccessToEvents { value, error in
            granted = value
            requestError = error
            semaphore.signal()
        }
    } else {
        store.requestAccess(to: .event) { value, error in
            granted = value
            requestError = error
            semaphore.signal()
        }
    }
    semaphore.wait()
    if let requestError { throw requestError }
    return granted
}

private func requirePermission() throws {
    guard hasPermission() else {
        throw RpcFailure(code: "permission_denied", message: "Calendar access is not granted. Open System Settings → Privacy & Security → Calendars.")
    }
}

private func findEvent(_ identifier: String) throws -> EKEvent {
    try requirePermission()
    guard let event = store.event(withIdentifier: identifier) else {
        throw RpcFailure(code: "not_found", message: "The event no longer exists.")
    }
    return event
}

private func setOptionalText(_ dictionary: [String: Any], key: String, setter: (String?) -> Void) {
    guard dictionary.keys.contains(key) else { return }
    setter(dictionary[key] is NSNull ? nil : dictionary[key] as? String)
}

private func handle(method: String, params: [String: Any]) throws -> Any {
    switch method {
    case "ping":
        return ["version": "0.1.0"]
    case "permissions":
        return permissionStatus()
    case "request_permissions":
        return try requestPermission()
    case "calendars":
        try requirePermission()
        return store.calendars(for: .event).map(calendarJSON)
    case "events":
        try requirePermission()
        let from = try isoDate(params["from"])
        let to = try isoDate(params["to"])
        let predicate = store.predicateForEvents(withStart: from, end: to, calendars: nil)
        return store.events(matching: predicate).map(eventJSON)
    case "event":
        guard let id = params["event_id"] as? String else { throw RpcFailure(code: "invalid_params", message: "event_id is required.") }
        try requirePermission()
        guard let event = store.event(withIdentifier: id) else { return NSNull() }
        return eventJSON(event)
    case "create_event":
        try requirePermission()
        guard let calendarID = params["calendar_id"] as? String,
              let calendar = store.calendar(withIdentifier: calendarID),
              calendar.allowsContentModifications,
              let title = params["title"] as? String
        else { throw RpcFailure(code: "invalid_params", message: "A writable calendar and title are required.") }
        let event = EKEvent(eventStore: store)
        event.calendar = calendar
        event.title = title
        event.startDate = try isoDate(params["start"])
        event.endDate = try isoDate(params["end"])
        event.isAllDay = params["all_day"] as? Bool ?? false
        event.location = params["location"] as? String
        event.notes = params["notes"] as? String
        try store.save(event, span: .thisEvent, commit: true)
        return eventJSON(event)
    case "update_event":
        guard let id = params["event_id"] as? String,
              let patch = params["patch"] as? [String: Any]
        else { throw RpcFailure(code: "invalid_params", message: "event_id and patch are required.") }
        let event = try findEvent(id)
        guard event.calendar.allowsContentModifications else { throw RpcFailure(code: "read_only", message: "The selected calendar is read-only.") }
        if let title = patch["title"] as? String { event.title = title }
        if patch.keys.contains("start") { event.startDate = try isoDate(patch["start"]) }
        if patch.keys.contains("end") { event.endDate = try isoDate(patch["end"]) }
        if let allDay = patch["all_day"] as? Bool { event.isAllDay = allDay }
        setOptionalText(patch, key: "location") { event.location = $0 }
        setOptionalText(patch, key: "notes") { event.notes = $0 }
        let scope = params["scope"] as? String ?? "this_event"
        if scope == "all_events" {
            throw RpcFailure(code: "unsupported_scope", message: "EventKit cannot safely identify occurrences before this one. Choose this event or this and future events.")
        }
        try store.save(event, span: scope == "this_and_future" ? .futureEvents : .thisEvent, commit: true)
        return eventJSON(event)
    case "delete_event":
        guard let id = params["event_id"] as? String else { throw RpcFailure(code: "invalid_params", message: "event_id is required.") }
        let event = try findEvent(id)
        guard event.calendar.allowsContentModifications else { throw RpcFailure(code: "read_only", message: "The selected calendar is read-only.") }
        let scope = params["scope"] as? String ?? "this_event"
        if scope == "all_events" {
            throw RpcFailure(code: "unsupported_scope", message: "EventKit cannot safely identify occurrences before this one. Choose this event or this and future events.")
        }
        try store.remove(event, span: scope == "this_and_future" ? .futureEvents : .thisEvent, commit: true)
        return [:]
    case "search":
        try requirePermission()
        let query = (params["query"] as? String ?? "").lowercased()
        var from = Calendar.current.date(byAdding: .year, value: -1, to: Date())!
        var to = Calendar.current.date(byAdding: .year, value: 2, to: Date())!
        if let range = params["range"] as? [String: Any] {
            from = try isoDate(range["from"])
            to = try isoDate(range["to"])
        }
        let predicate = store.predicateForEvents(withStart: from, end: to, calendars: nil)
        return store.events(matching: predicate).filter { event in
            [event.title, event.location, event.notes].compactMap { $0?.lowercased() }.contains { $0.contains(query) }
        }.map(eventJSON)
    default:
        throw RpcFailure(code: "unknown_method", message: "Unknown method: \(method)")
    }
}

private func send(_ object: [String: Any]) {
    guard let data = try? JSONSerialization.data(withJSONObject: object),
          let line = String(data: data, encoding: .utf8)
    else { return }
    print(line)
    fflush(stdout)
}

while let line = readLine() {
    guard let data = line.data(using: .utf8),
          let request = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
          let id = request["id"] as? NSNumber,
          let method = request["method"] as? String
    else {
        send(["id": 0, "ok": false, "error": ["code": "invalid_request", "message": "Request must be a JSON object with id and method."]])
        continue
    }
    do {
        let result = try handle(method: method, params: request["params"] as? [String: Any] ?? [:])
        send(["id": id, "ok": true, "result": result])
    } catch let failure as RpcFailure {
        send(["id": id, "ok": false, "error": ["code": failure.code, "message": failure.message]])
    } catch {
        send(["id": id, "ok": false, "error": ["code": "eventkit_error", "message": error.localizedDescription]])
    }
}

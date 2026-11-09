import ExpCore
import Foundation
import Sentry

/// #LOGS-2: crash reports, app hangs, performance (app start, view
/// controllers, our API + Electric shape requests) and breadcrumbs/logs,
/// sent THROUGH the signed-in server's tunnel (`POST /api/monitoring`).
/// On only when that server's config says so AND Settings › Account ›
/// Privacy › Send diagnostics is on (default on, per device). Launch starts
/// from the cached answer so early crashes are caught; `refresh` re-reads the
/// server.
@MainActor
enum Observability {
    /// What the running SDK was started with; nil = not running.
    private static var running: Monitoring.Cached?
    private static var userId: String?

    /// App launch, before anything else can crash.
    static func launch(origins: [String]) {
        MonitoringState.shared.setOrigins(origins)
        apply(Monitoring.cached())
    }

    /// The signed-in instances changed (or the app came back): re-read the
    /// active server's answer and follow it.
    static func refresh(activeOrigin: String, origins: [String], userId: String?) async {
        MonitoringState.shared.setOrigins(origins)
        setUser(userId)
        do {
            let config = try await Monitoring.fetchConfig(origin: activeOrigin, session: configSession)
            let cached = Monitoring.Cached(origin: activeOrigin, config: config)
            Monitoring.store(cached)
            apply(cached)
        } catch {
            // Offline: the cached answer stays in force.
        }
    }

    /// The Send diagnostics toggle (already written to UserDefaults).
    static func diagnosticsChanged() {
        apply(Monitoring.cached())
    }

    static func setUser(_ id: String?) {
        userId = id
        guard running != nil else { return }
        // The pseudonymous user id only: no email, name or IP.
        SentrySDK.setUser(id.map { User(userId: $0) })
    }

    private static func apply(_ cached: Monitoring.Cached?) {
        let wanted = cached.flatMap { $0.config.isActive && Monitoring.sendDiagnostics() ? $0 : nil }
        guard wanted != running else { return }
        if running != nil {
            SentrySDK.close()
            running = nil
        }
        guard let wanted, let dsn = wanted.config.dsn, let tunnel = Monitoring.tunnelUrl(origin: wanted.origin) else {
            return
        }
        MonitoringState.shared.setTunnelUrl(tunnel)
        start(wanted, dsn: dsn)
        running = wanted
        setUser(userId)
    }

    private static func start(_ cached: Monitoring.Cached, dsn: String) {
        #if DEBUG
        let isDebug = true
        #else
        let isDebug = false
        #endif
        let config = cached.config
        let targets = Monitoring.targetPatterns(origins: MonitoringState.shared.origins + [cached.origin])
            .compactMap { try? NSRegularExpression(pattern: $0) }
        SentrySDK.start { options in
            options.dsn = dsn
            options.releaseName = Monitoring.release(info: Bundle.main.infoDictionary)
            options.environment = Monitoring.environment(config: config, origin: cached.origin, isDebug: isDebug)
            options.urlSession = tunnelSession
            options.sendDefaultPii = false
            options.sampleRate = 1
            options.tracesSampleRate = NSNumber(value: config.resolvedTracesSampleRate)
            options.enableAppHangTracking = true
            options.appHangTimeoutInterval = config.resolvedAppHangTimeout
            options.enableAutoPerformanceTracing = true
            options.enableUIViewControllerTracing = true
            options.enableNetworkTracking = true
            options.enableNetworkBreadcrumbs = true
            options.enableLogs = true
            options.attachScreenshot = false
            options.attachViewHierarchy = false
            options.tracePropagationTargets = targets
            options.failedRequestTargets = targets
            options.beforeSend = scrub(event:)
            options.beforeBreadcrumb = scrub(breadcrumb:)
            options.beforeSendSpan = scrub(span:)
            options.beforeSendLog = scrub(log:)
        }
    }

    // MARK: - Scrubbing (SDK callbacks, any thread)

    private nonisolated static func scrub(event: Event) -> Event? {
        if Monitoring.isTunnelRequest(event.request?.url) { return nil }
        if let request = event.request {
            request.headers = request.headers.map(Monitoring.Scrubber.scrub(headers:))
            request.cookies = nil
            request.url = request.url.map(Monitoring.Scrubber.scrubUrl)
            request.queryString = request.queryString.map(Monitoring.Scrubber.scrubQuery)
        }
        event.extra = event.extra.map(Monitoring.Scrubber.scrub)
        event.tags = event.tags.map(Monitoring.Scrubber.scrub(headers:))
        event.breadcrumbs = event.breadcrumbs?.compactMap(scrub(breadcrumb:))
        event.user?.email = nil
        event.user?.username = nil
        event.user?.name = nil
        event.user?.ipAddress = nil
        return event
    }

    private nonisolated static func scrub(breadcrumb: Breadcrumb) -> Breadcrumb? {
        if Monitoring.isTunnelRequest(breadcrumb.data?["url"] as? String) { return nil }
        for (key, value) in Monitoring.Scrubber.scrub(breadcrumb.data ?? [:]) {
            breadcrumb.setData(value: value, key: key)
        }
        breadcrumb.message = breadcrumb.message.map(Monitoring.Scrubber.scrubUrl)
        return breadcrumb
    }

    /// Network spans for our own servers only (API + Electric shapes).
    private nonisolated static func scrub(span: Span) -> Span? {
        guard span.operation == "http.client" else { return span }
        let url = span.data["url"] as? String
        guard Monitoring.isOwnRequest(url, origins: MonitoringState.shared.origins) else { return nil }
        if let url { span.setData(value: Monitoring.Scrubber.scrubUrl(url), key: "url") }
        if let query = span.data["http.query"] as? String {
            span.setData(value: Monitoring.Scrubber.scrubQuery(query), key: "http.query")
        }
        if let description = span.spanDescription {
            span.spanDescription = Monitoring.Scrubber.scrubUrl(description)
        }
        return span
    }

    private nonisolated static func scrub(log: SentryLog) -> SentryLog? {
        log.attributes = log.attributes.filter { !Monitoring.Scrubber.isSensitive($0.key) }
        return log
    }

    // MARK: - Sessions

    /// The config read: a plain ephemeral session, no cookies, no cache.
    private static let configSession = URLSession(configuration: .ephemeral)

    /// The SDK's transport: every envelope request is re-addressed to the
    /// server's tunnel by `MonitoringTunnelProtocol`.
    private static let tunnelSession: URLSession = {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [MonitoringTunnelProtocol.self]
        return URLSession(configuration: configuration)
    }()
}

/// Re-addresses the SDK's envelope POST (to the DSN's ingest host) to
/// `<server>/api/monitoring`, body and headers unchanged (gzip envelope).
final class MonitoringTunnelProtocol: URLProtocol, @unchecked Sendable {
    private static let forwardSession = URLSession(configuration: .ephemeral)
    private var forwardTask: URLSessionDataTask?

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        guard let tunnel = MonitoringState.shared.tunnelUrl else {
            client?.urlProtocol(self, didFailWithError: URLError(.cancelled))
            return
        }
        var forwarded = URLRequest(url: tunnel)
        forwarded.httpMethod = "POST"
        forwarded.timeoutInterval = request.timeoutInterval
        for (field, value) in request.allHTTPHeaderFields ?? [:] {
            forwarded.setValue(value, forHTTPHeaderField: field)
        }
        forwarded.setValue(AppConstants.clientVersionHeaderValue, forHTTPHeaderField: "x-client-version")
        forwarded.httpBody = request.httpBody ?? request.httpBodyStream.map(Self.read)
        forwardTask = Self.forwardSession.dataTask(with: forwarded) { [weak self] data, response, error in
            guard let self else { return }
            if let error {
                self.client?.urlProtocol(self, didFailWithError: error)
                return
            }
            if let response {
                self.client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            }
            if let data { self.client?.urlProtocol(self, didLoad: data) }
            self.client?.urlProtocolDidFinishLoading(self)
        }
        forwardTask?.resume()
    }

    override func stopLoading() {
        forwardTask?.cancel()
    }

    private static func read(_ stream: InputStream) -> Data {
        var data = Data()
        stream.open()
        defer { stream.close() }
        var buffer = [UInt8](repeating: 0, count: 16 * 1024)
        while stream.hasBytesAvailable {
            let count = stream.read(&buffer, maxLength: buffer.count)
            if count <= 0 { break }
            data.append(buffer, count: count)
        }
        return data
    }
}

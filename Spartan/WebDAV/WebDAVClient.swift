//
//  WebDAVClient.swift
//  Argo
//
//  A dependency-free WebDAV client built on URLSession + XMLParser.
//  Completion-handler based so it runs on tvOS 13 without the Swift
//  concurrency back-deployment libraries.
//

import Foundation

final class WebDAVClient: NSObject {

    let server: WebDAVServer
    private var session: URLSession!

    init(server: WebDAVServer) {
        self.server = server
        super.init()
        let config = URLSessionConfiguration.default
        config.timeoutIntervalForRequest = 30
        config.timeoutIntervalForResource = 3600
        config.requestCachePolicy = .reloadIgnoringLocalCacheData
        config.httpAdditionalHeaders = ["User-Agent": "Argo/1.0 (tvOS; WebDAV)"]
        self.session = URLSession(configuration: config, delegate: self, delegateQueue: nil)
    }

    deinit {
        session.finishTasksAndInvalidate()
    }

    // MARK: - URL building

    func url(forPath path: String) -> URL? {
        guard var comps = URLComponents(string: server.urlString) else { return nil }
        comps.path = path.hasPrefix("/") ? path : "/" + path
        comps.query = nil
        comps.fragment = nil
        return comps.url
    }

    private func request(_ method: String, path: String) -> URLRequest? {
        guard let url = url(forPath: path) else { return nil }
        var req = URLRequest(url: url)
        req.httpMethod = method
        if !server.username.isEmpty || !server.password.isEmpty {
            let raw = "\(server.username):\(server.password)"
            if let data = raw.data(using: .utf8) {
                req.setValue("Basic " + data.base64EncodedString(), forHTTPHeaderField: "Authorization")
            }
        }
        return req
    }

    private func finish<T>(_ completion: @escaping (Result<T, Error>) -> Void, _ result: Result<T, Error>) {
        DispatchQueue.main.async { completion(result) }
    }

    /// Maps a URLSession result onto a Result, checking the HTTP status code.
    private func validate(_ response: URLResponse?, _ error: Error?, acceptable: Set<Int>) -> Error? {
        if let error = error {
            return WebDAVError.transport(error.localizedDescription)
        }
        guard let http = response as? HTTPURLResponse else {
            return WebDAVError.emptyResponse
        }
        if acceptable.contains(http.statusCode) { return nil }
        if (200...299).contains(http.statusCode) { return nil }
        return WebDAVError.http(http.statusCode)
    }

    // MARK: - Listing

    /// PROPFIND with Depth: 1. Returns the children of `path`, self excluded.
    func list(path: String, completion: @escaping (Result<[WebDAVItem], Error>) -> Void) {
        guard var req = request("PROPFIND", path: path) else {
            finish(completion, .failure(WebDAVError.badURL)); return
        }
        req.setValue("1", forHTTPHeaderField: "Depth")
        req.setValue("application/xml; charset=utf-8", forHTTPHeaderField: "Content-Type")
        let body = """
        <?xml version="1.0" encoding="utf-8"?>
        <D:propfind xmlns:D="DAV:">
          <D:prop>
            <D:displayname/>
            <D:resourcetype/>
            <D:getcontentlength/>
            <D:getlastmodified/>
            <D:getcontenttype/>
          </D:prop>
        </D:propfind>
        """
        req.httpBody = body.data(using: .utf8)

        let task = session.dataTask(with: req) { [weak self] data, response, error in
            guard let self = self else { return }
            if let failure = self.validate(response, error, acceptable: [207, 200]) {
                self.finish(completion, .failure(failure)); return
            }
            guard let data = data, !data.isEmpty else {
                self.finish(completion, .failure(WebDAVError.emptyResponse)); return
            }
            let parser = WebDAVPropfindParser()
            guard let entries = parser.parse(data) else {
                self.finish(completion, .failure(WebDAVError.parseFailure)); return
            }

            let requested = WebDAVClient.normalize(path)
            var items: [WebDAVItem] = []
            for entry in entries {
                let entryPath = WebDAVClient.normalize(entry.path)
                if entryPath == requested { continue } // the collection itself
                items.append(entry)
            }
            items.sort { lhs, rhs in
                if lhs.isDirectory != rhs.isDirectory { return lhs.isDirectory }
                return lhs.name.localizedCaseInsensitiveCompare(rhs.name) == .orderedAscending
            }
            self.finish(completion, .success(items))
        }
        task.resume()
    }

    /// Cheap reachability / credential check.
    func testConnection(completion: @escaping (Result<Void, Error>) -> Void) {
        list(path: server.rootPath) { result in
            switch result {
            case .success: completion(.success(()))
            case .failure(let error): completion(.failure(error))
            }
        }
    }

    // MARK: - Transfers

    @discardableResult
    func download(item: WebDAVItem,
                  to localPath: String,
                  progress: @escaping (Double) -> Void,
                  completion: @escaping (Result<String, Error>) -> Void) -> URLSessionDownloadTask? {
        guard let req = request("GET", path: item.path) else {
            finish(completion, .failure(WebDAVError.badURL)); return nil
        }
        // Delegate-driven (not completion-handler) so that progress callbacks fire.
        let task = session.downloadTask(with: req)
        lock.lock()
        downloadDestinations[task.taskIdentifier] = localPath
        downloadProgress[task.taskIdentifier] = progress
        downloadCompletions[task.taskIdentifier] = completion
        lock.unlock()
        task.resume()
        return task
    }

    fileprivate func completeDownload(_ taskIdentifier: Int, _ result: Result<String, Error>) {
        lock.lock()
        let completion = downloadCompletions.removeValue(forKey: taskIdentifier)
        downloadDestinations.removeValue(forKey: taskIdentifier)
        downloadProgress.removeValue(forKey: taskIdentifier)
        lock.unlock()
        guard let completion = completion else { return }
        DispatchQueue.main.async { completion(result) }
    }

    func upload(localPath: String,
                toRemotePath remotePath: String,
                completion: @escaping (Result<Void, Error>) -> Void) {
        guard let req = request("PUT", path: remotePath) else {
            finish(completion, .failure(WebDAVError.badURL)); return
        }
        let task = session.uploadTask(with: req, fromFile: URL(fileURLWithPath: localPath)) { [weak self] _, response, error in
            guard let self = self else { return }
            if let failure = self.validate(response, error, acceptable: [200, 201, 204]) {
                self.finish(completion, .failure(failure)); return
            }
            self.finish(completion, .success(()))
        }
        task.resume()
    }

    // MARK: - Mutations

    func createDirectory(at path: String, completion: @escaping (Result<Void, Error>) -> Void) {
        simple("MKCOL", path: path, acceptable: [201], completion: completion)
    }

    func delete(path: String, completion: @escaping (Result<Void, Error>) -> Void) {
        simple("DELETE", path: path, acceptable: [200, 204, 207], completion: completion)
    }

    func move(from: String, to: String, completion: @escaping (Result<Void, Error>) -> Void) {
        guard var req = request("MOVE", path: from), let destination = url(forPath: to) else {
            finish(completion, .failure(WebDAVError.badURL)); return
        }
        req.setValue(destination.absoluteString, forHTTPHeaderField: "Destination")
        req.setValue("T", forHTTPHeaderField: "Overwrite")
        run(req, acceptable: [201, 204], completion: completion)
    }

    private func simple(_ method: String, path: String, acceptable: Set<Int>,
                        completion: @escaping (Result<Void, Error>) -> Void) {
        guard let req = request(method, path: path) else {
            finish(completion, .failure(WebDAVError.badURL)); return
        }
        run(req, acceptable: acceptable, completion: completion)
    }

    private func run(_ req: URLRequest, acceptable: Set<Int>,
                     completion: @escaping (Result<Void, Error>) -> Void) {
        let task = session.dataTask(with: req) { [weak self] _, response, error in
            guard let self = self else { return }
            if let failure = self.validate(response, error, acceptable: acceptable) {
                self.finish(completion, .failure(failure)); return
            }
            self.finish(completion, .success(()))
        }
        task.resume()
    }

    // MARK: - Path helpers

    static func normalize(_ path: String) -> String {
        var p = path
        while p.hasSuffix("/") && p.count > 1 { p.removeLast() }
        if !p.hasPrefix("/") { p = "/" + p }
        return p
    }

    static func parent(of path: String) -> String {
        let normalized = normalize(path)
        if normalized == "/" { return "/" }
        return (normalized as NSString).deletingLastPathComponent
    }

    static func child(_ parent: String, _ name: String) -> String {
        return (normalize(parent) as NSString).appendingPathComponent(name)
    }

    // MARK: - Transfer bookkeeping

    fileprivate let lock = NSLock()
    fileprivate var downloadDestinations: [Int: String] = [:]
    fileprivate var downloadProgress: [Int: (Double) -> Void] = [:]
    fileprivate var downloadCompletions: [Int: (Result<String, Error>) -> Void] = [:]
    fileprivate var downloadResults: [Int: Result<String, Error>] = [:]
}

extension WebDAVClient: URLSessionDataDelegate, URLSessionDownloadDelegate {

    func urlSession(_ session: URLSession,
                    didReceive challenge: URLAuthenticationChallenge,
                    completionHandler: @escaping (URLSession.AuthChallengeDisposition, URLCredential?) -> Void) {
        let method = challenge.protectionSpace.authenticationMethod

        if method == NSURLAuthenticationMethodServerTrust {
            if server.allowInsecureTLS, let trust = challenge.protectionSpace.serverTrust {
                completionHandler(.useCredential, URLCredential(trust: trust))
            } else {
                completionHandler(.performDefaultHandling, nil)
            }
            return
        }

        // Basic / Digest / NTLM: answer once, then give up so we surface a 401
        // instead of looping forever on bad credentials.
        if challenge.previousFailureCount > 0 {
            completionHandler(.cancelAuthenticationChallenge, nil)
            return
        }
        let credential = URLCredential(user: server.username,
                                       password: server.password,
                                       persistence: .forSession)
        completionHandler(.useCredential, credential)
    }

    func urlSession(_ session: URLSession,
                    downloadTask: URLSessionDownloadTask,
                    didWriteData bytesWritten: Int64,
                    totalBytesWritten: Int64,
                    totalBytesExpectedToWrite: Int64) {
        guard totalBytesExpectedToWrite > 0 else { return }
        lock.lock()
        let handler = downloadProgress[downloadTask.taskIdentifier]
        lock.unlock()
        guard let handler = handler else { return }
        let fraction = Double(totalBytesWritten) / Double(totalBytesExpectedToWrite)
        DispatchQueue.main.async { handler(fraction) }
    }

    func urlSession(_ session: URLSession,
                    downloadTask: URLSessionDownloadTask,
                    didFinishDownloadingTo location: URL) {
        let identifier = downloadTask.taskIdentifier
        lock.lock()
        let destination = downloadDestinations[identifier]
        lock.unlock()

        if let http = downloadTask.response as? HTTPURLResponse, !(200...299).contains(http.statusCode) {
            lock.lock(); downloadResults[identifier] = .failure(WebDAVError.http(http.statusCode)); lock.unlock()
            return
        }
        guard let destination = destination else { return }

        do {
            let fm = FileManager.default
            let destinationURL = URL(fileURLWithPath: destination)
            try fm.createDirectory(at: destinationURL.deletingLastPathComponent(),
                                   withIntermediateDirectories: true)
            if fm.fileExists(atPath: destination) {
                try fm.removeItem(at: destinationURL)
            }
            // The temporary file is deleted as soon as this method returns, so move it now.
            try fm.moveItem(at: location, to: destinationURL)
            lock.lock(); downloadResults[identifier] = .success(destination); lock.unlock()
        } catch {
            lock.lock()
            downloadResults[identifier] = .failure(WebDAVError.transport(error.localizedDescription))
            lock.unlock()
        }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        let identifier = task.taskIdentifier
        lock.lock()
        let stored = downloadResults.removeValue(forKey: identifier)
        let isDownload = downloadCompletions[identifier] != nil
        lock.unlock()
        guard isDownload else { return }

        if let error = error {
            completeDownload(identifier, .failure(WebDAVError.transport(error.localizedDescription)))
            return
        }
        if let stored = stored {
            completeDownload(identifier, stored)
        } else {
            completeDownload(identifier, .failure(WebDAVError.emptyResponse))
        }
    }
}

// MARK: - PROPFIND response parsing

private final class WebDAVPropfindParser: NSObject, XMLParserDelegate {

    private var items: [WebDAVItem] = []
    private var currentElement = ""
    private var buffer = ""

    private var href = ""
    private var displayName = ""
    private var contentLength: Int64 = 0
    private var contentType = ""
    private var lastModified: Date?
    private var isCollection = false
    private var insideResponse = false
    private var insideResourceType = false

    private static let rfc1123: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(secondsFromGMT: 0)
        formatter.dateFormat = "EEE, dd MMM yyyy HH:mm:ss zzz"
        return formatter
    }()

    private static let iso8601: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(secondsFromGMT: 0)
        formatter.dateFormat = "yyyy-MM-dd'T'HH:mm:ss'Z'"
        return formatter
    }()

    func parse(_ data: Data) -> [WebDAVItem]? {
        let parser = XMLParser(data: data)
        parser.shouldProcessNamespaces = true
        parser.delegate = self
        guard parser.parse() else { return nil }
        return items
    }

    func parser(_ parser: XMLParser, didStartElement elementName: String,
                namespaceURI: String?, qualifiedName qName: String?,
                attributes attributeDict: [String: String] = [:]) {
        currentElement = elementName
        buffer = ""
        switch elementName {
        case "response":
            insideResponse = true
            href = ""; displayName = ""; contentLength = 0; contentType = ""
            lastModified = nil; isCollection = false
        case "resourcetype":
            insideResourceType = true
        case "collection":
            if insideResourceType { isCollection = true }
        default:
            break
        }
    }

    func parser(_ parser: XMLParser, foundCharacters string: String) {
        buffer += string
    }

    func parser(_ parser: XMLParser, didEndElement elementName: String,
                namespaceURI: String?, qualifiedName qName: String?) {
        let value = buffer.trimmingCharacters(in: .whitespacesAndNewlines)
        switch elementName {
        case "href":
            if insideResponse && href.isEmpty { href = value }
        case "displayname":
            if displayName.isEmpty { displayName = value }
        case "getcontentlength":
            contentLength = Int64(value) ?? 0
        case "getcontenttype":
            contentType = value
        case "getlastmodified":
            lastModified = WebDAVPropfindParser.rfc1123.date(from: value)
                ?? WebDAVPropfindParser.iso8601.date(from: value)
        case "resourcetype":
            insideResourceType = false
        case "response":
            insideResponse = false
            if let item = makeItem() { items.append(item) }
        default:
            break
        }
        buffer = ""
    }

    private func makeItem() -> WebDAVItem? {
        guard !href.isEmpty else { return nil }

        // href may be an absolute URL or an absolute path, and is percent-encoded.
        var path: String
        if let comps = URLComponents(string: href), comps.host != nil {
            path = comps.path
        } else {
            path = href.removingPercentEncoding ?? href
        }
        if path.isEmpty { return nil }

        let collection = isCollection || path.hasSuffix("/")
        let normalized = WebDAVClient.normalize(path)
        var name = displayName
        if name.isEmpty {
            name = (normalized as NSString).lastPathComponent
        }
        if name.isEmpty || name == "/" { name = normalized }

        return WebDAVItem(path: normalized,
                          name: name,
                          isDirectory: collection,
                          size: contentLength,
                          modified: lastModified,
                          contentType: contentType)
    }
}

//
//  RemoteStream.swift
//  Argo
//
//  WebDAV 의 음악·동영상을 받지 않고 바로 재생하는 중계기.
//
//  AVPlayer 에 WebDAV 주소를 그대로 주면 세 가지가 막힌다.
//    · 자체 서명 인증서(시놀로지·헤놀로지 기본값)를 AVPlayer 는 받아 주지 않는다.
//    · Digest 인증, 아이디·비밀번호를 AVPlayer 에 넘길 공식 방법이 없다.
//    · 서버가 Content-Type 을 application/octet-stream 으로 주면 재생을 거부한다.
//  그래서 애플TV 안(127.0.0.1)에만 작은 HTTP 서버를 열고, AVPlayer 의 요청(Range 포함)을
//  WebDAV 연결과 같은 인증·인증서 설정으로 서버에 그대로 전달한다. 받은 바이트는
//  디스크에 쓰지 않고 바로 흘려보낸다 — 2시간짜리 영화도 고른 순간 재생된다.
//
//  보안: 루프백 주소에만 묶으므로 같은 네트워크의 다른 기기는 접속할 수 없다.
//  주소에는 추측할 수 없는 일회용 토큰이 들어가고, WebDAV 화면을 닫으면 모두 지운다.
//

import Foundation
import Network

final class ArgoRemoteStream: NSObject {

    static let shared = ArgoRemoteStream()

    private struct Route {
        let url: URL
        let server: WebDAVServer
        let mime: String?
    }

    private let queue = DispatchQueue(label: "argo.remote.stream")
    private var listener: NWListener?
    private var port: UInt16 = 0
    private var waiters: [(UInt16?) -> Void] = []
    private var routes: [String: Route] = [:]
    private var sessions: [String: URLSession] = [:]       // 서버 id → 세션(인증서 설정이 서버마다 다르다)
    private var relays: [Int: Relay] = [:]                  // 작업 id → 중계 상태
    private let opQueue: OperationQueue = {
        let q = OperationQueue()
        q.maxConcurrentOperationCount = 1
        return q
    }()

    private override init() {
        super.init()
        opQueue.underlyingQueue = queue
    }

    // MARK: - 바깥에서 부르는 것

    /// 그 파일을 재생할 주소. 서버가 처음이면 여는 동안 기다렸다가 준다. 결과는 메인 스레드로.
    func streamURL(remote: URL, server: WebDAVServer, name: String,
                   completion: @escaping (URL?) -> Void) {
        queue.async {
            self.ensureListening { port in
                guard let port = port else {
                    DispatchQueue.main.async { completion(nil) }
                    return
                }
                let token = UUID().uuidString.replacingOccurrences(of: "-", with: "")
                self.routes[token] = Route(url: remote, server: server,
                                           mime: ArgoRemoteStream.mimeType(forName: name))
                // 확장자를 주소 끝에 남겨 둔다 — AVPlayer 가 형식을 짐작할 때 본다.
                let safe = name.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? "media"
                let url = URL(string: "http://127.0.0.1:\(port)/\(token)/\(safe)")
                DispatchQueue.main.async { completion(url) }
            }
        }
    }

    /// WebDAV 화면을 닫을 때 부른다. 토큰을 지워 주소를 더 쓸 수 없게 한다.
    /// 재생 중인 연결은 끊지 않는다(화면을 닫은 뒤에도 영상은 계속 볼 수 있어야 한다).
    func forgetRoutes() {
        queue.async { self.routes.removeAll() }
    }

    /// 진행 중인 중계를 모두 끊는다. 사진 앨범을 열 때 부른다 — 영상 받기가 뒤에서 계속 돌며
    /// 메모리·네트워크를 잡고 있지 않게 한다. 열린 연결은 재생기가 다시 요청하면 새로 만든다.
    func cancelAll() {
        queue.async {
            let n = self.relays.count
            for (_, r) in self.relays { r.task?.cancel(); r.conn.cancel() }
            self.relays.removeAll()
            if n > 0 { ArgoDiag.log("stream", "중계 \(n)개 끊음") }
        }
    }

    static func mimeType(forName name: String) -> String? {
        switch (name as NSString).pathExtension.lowercased() {
        case "mp4":          return "video/mp4"
        case "m4v":          return "video/x-m4v"
        case "mov", "qt":    return "video/quicktime"
        case "3gp":          return "video/3gpp"
        case "ts":           return "video/mp2t"
        case "mp3":          return "audio/mpeg"
        case "m4a", "m4b", "alac": return "audio/mp4"
        case "aac":          return "audio/aac"
        case "wav":          return "audio/wav"
        case "aif", "aiff":  return "audio/aiff"
        case "caf":          return "audio/x-caf"
        case "flac":         return "audio/flac"
        case "ac3":          return "audio/ac3"
        default:             return nil
        }
    }

    // MARK: - 듣기

    private func ensureListening(_ done: @escaping (UInt16?) -> Void) {
        if listener != nil, port != 0 { done(port); return }
        waiters.append(done)
        guard listener == nil else { return }   // 이미 여는 중
        do {
            let params = NWParameters.tcp
            params.requiredLocalEndpoint = NWEndpoint.hostPort(host: .ipv4(.loopback), port: .any)
            params.allowLocalEndpointReuse = true
            let l = try NWListener(using: params)
            l.newConnectionHandler = { [weak self] c in self?.accept(c) }
            l.stateUpdateHandler = { [weak self] state in
                guard let self = self else { return }
                switch state {
                case .ready:
                    self.port = l.port?.rawValue ?? 0
                    ArgoDiag.log("stream", "중계기 열림 127.0.0.1:\(self.port)")
                    self.flushWaiters(self.port == 0 ? nil : self.port)
                case .failed(let e):
                    ArgoDiag.log("stream", "중계기 실패 \(e)")
                    l.cancel()
                    self.listener = nil
                    self.port = 0
                    self.flushWaiters(nil)
                case .cancelled:
                    self.listener = nil
                    self.port = 0
                default:
                    break
                }
            }
            listener = l
            l.start(queue: queue)
        } catch {
            ArgoDiag.log("stream", "중계기를 못 열었다 \(error)")
            flushWaiters(nil)
        }
    }

    private func flushWaiters(_ port: UInt16?) {
        let w = waiters
        waiters.removeAll()
        w.forEach { $0(port) }
    }

    // MARK: - 연결 하나

    private final class Relay {
        let conn: NWConnection
        var task: URLSessionDataTask?
        var pending = 0           // 보냈지만 아직 다 나가지 않은 바이트
        var suspended = false
        var finished = false
        var headOnly = false
        init(conn: NWConnection) { self.conn = conn }
    }

    /// 쌓아 둘 수 있는 양. 넘으면 서버에서 받기를 잠시 멈춘다(재생기가 느릴 때 메모리가 늘지 않게).
    private static let highWater = 8 * 1024 * 1024
    private static let lowWater = 2 * 1024 * 1024
    /// 요청 머리의 상한(악성 요청 대비 — 어차피 루프백만 받는다).
    private static let maxHeader = 16 * 1024

    private func accept(_ c: NWConnection) {
        c.start(queue: queue)
        readHeader(c, buffer: Data())
    }

    private func readHeader(_ c: NWConnection, buffer: Data) {
        c.receive(minimumIncompleteLength: 1, maximumLength: 8192) { [weak self] data, _, isComplete, error in
            guard let self = self else { c.cancel(); return }
            var buf = buffer
            if let data = data { buf.append(data) }
            if let end = buf.range(of: Data("\r\n\r\n".utf8)) {
                let head = String(decoding: buf[buf.startIndex..<end.lowerBound], as: UTF8.self)
                self.handle(c, head: head)
            } else if error != nil || isComplete || buf.count > ArgoRemoteStream.maxHeader {
                c.cancel()
            } else {
                self.readHeader(c, buffer: buf)
            }
        }
    }

    private func handle(_ c: NWConnection, head: String) {
        let lines = head.components(separatedBy: "\r\n")
        let parts = (lines.first ?? "").split(separator: " ")
        guard parts.count >= 2 else { return reply(c, status: 400) }
        let method = String(parts[0]).uppercased()
        guard method == "GET" || method == "HEAD" else { return reply(c, status: 405) }
        // "/토큰/이름" → 토큰
        let target = String(parts[1])
        let token = target.split(separator: "/").first.map(String.init) ?? ""
        guard let route = routes[token] else { return reply(c, status: 404) }

        var range: String?
        for line in lines.dropFirst() {
            guard let colon = line.firstIndex(of: ":") else { continue }
            let key = line[..<colon].trimmingCharacters(in: .whitespaces).lowercased()
            if key == "range" {
                range = line[line.index(after: colon)...].trimmingCharacters(in: .whitespaces)
            }
        }

        var req = URLRequest(url: route.url)
        req.httpMethod = method
        req.timeoutInterval = 60
        if let r = range { req.setValue(r, forHTTPHeaderField: "Range") }
        if !route.server.username.isEmpty || !route.server.password.isEmpty,
           let raw = "\(route.server.username):\(route.server.password)".data(using: .utf8) {
            req.setValue("Basic " + raw.base64EncodedString(), forHTTPHeaderField: "Authorization")
        }

        let task = session(for: route.server).dataTask(with: req)
        let relay = Relay(conn: c)
        relay.task = task
        relay.headOnly = (method == "HEAD")
        relays[task.taskIdentifier] = relay
        mimes[task.taskIdentifier] = route.mime

        // 재생기가 연결을 끊으면(앞으로 감기 등) 서버 쪽 받기도 멈춘다.
        c.stateUpdateHandler = { [weak self, weak task] state in
            switch state {
            case .failed, .cancelled:
                task?.cancel()
                if let id = task?.taskIdentifier { self?.drop(id) }
            default: break
            }
        }
        task.resume()
    }

    private var mimes: [Int: String?] = [:]

    private func session(for server: WebDAVServer) -> URLSession {
        let key = server.id.uuidString + (server.allowInsecureTLS ? "-i" : "")
        if let s = sessions[key] { return s }
        let config = URLSessionConfiguration.default
        config.timeoutIntervalForRequest = 60
        config.timeoutIntervalForResource = 24 * 3600
        config.requestCachePolicy = .reloadIgnoringLocalCacheData
        config.urlCache = nil
        config.httpAdditionalHeaders = ["User-Agent": "Argo/1.0 (tvOS; WebDAV stream)"]
        let s = URLSession(configuration: config, delegate: self, delegateQueue: opQueue)
        sessions[key] = s
        serversBySession[ObjectIdentifier(s)] = server
        return s
    }

    private var serversBySession: [ObjectIdentifier: WebDAVServer] = [:]

    /// 상태 줄 문구. 기기 언어로 번역된 문구(한글)를 상태 줄에 넣지 않으려고 직접 정한다.
    static func reason(_ status: Int) -> String {
        switch status {
        case 200: return "OK"
        case 206: return "Partial Content"
        case 400: return "Bad Request"
        case 401: return "Unauthorized"
        case 403: return "Forbidden"
        case 404: return "Not Found"
        case 405: return "Method Not Allowed"
        case 416: return "Range Not Satisfiable"
        default:  return status < 400 ? "OK" : "Error"
        }
    }

    private func reply(_ c: NWConnection, status: Int) {
        let text = "HTTP/1.1 \(status) \(ArgoRemoteStream.reason(status))\r\n"
            + "Content-Length: 0\r\nConnection: close\r\n\r\n"
        c.send(content: Data(text.utf8), completion: .contentProcessed { _ in c.cancel() })
    }

    private func drop(_ id: Int) {
        relays.removeValue(forKey: id)
        mimes.removeValue(forKey: id)
    }

    private func finish(_ relay: Relay, id: Int) {
        guard !relay.finished else { return }
        relay.finished = true
        drop(id)
        // 남은 것이 다 나간 뒤 닫는다.
        relay.conn.send(content: nil, contentContext: .finalMessage, isComplete: true,
                        completion: .contentProcessed { _ in relay.conn.cancel() })
    }
}

// MARK: - 서버 쪽 받기

extension ArgoRemoteStream: URLSessionDataDelegate {

    func urlSession(_ session: URLSession,
                    didReceive challenge: URLAuthenticationChallenge,
                    completionHandler: @escaping (URLSession.AuthChallengeDisposition, URLCredential?) -> Void) {
        guard let server = serversBySession[ObjectIdentifier(session)] else {
            completionHandler(.performDefaultHandling, nil); return
        }
        if challenge.protectionSpace.authenticationMethod == NSURLAuthenticationMethodServerTrust {
            if server.allowInsecureTLS, let trust = challenge.protectionSpace.serverTrust {
                completionHandler(.useCredential, URLCredential(trust: trust))
            } else {
                completionHandler(.performDefaultHandling, nil)
            }
            return
        }
        if challenge.previousFailureCount > 0 {
            completionHandler(.cancelAuthenticationChallenge, nil)
            return
        }
        completionHandler(.useCredential, URLCredential(user: server.username, password: server.password,
                                                        persistence: .forSession))
    }

    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask,
                    didReceive response: URLResponse,
                    completionHandler: @escaping (URLSession.ResponseDisposition) -> Void) {
        let id = dataTask.taskIdentifier
        guard let relay = relays[id], let http = response as? HTTPURLResponse else {
            completionHandler(.cancel); return
        }
        var lines = ["HTTP/1.1 \(http.statusCode) \(ArgoRemoteStream.reason(http.statusCode))"]
        // 서버가 준 형식이 쓸모없으면 확장자로 정한 형식을 준다.
        let serverType = (http.value(forHTTPHeaderField: "Content-Type") ?? "").lowercased()
        let fallback = mimes[id] ?? nil
        if let f = fallback, serverType.isEmpty || serverType.hasPrefix("application/octet-stream")
            || serverType.hasPrefix("binary/") || serverType.hasPrefix("application/x-") {
            lines.append("Content-Type: \(f)")
        } else if !serverType.isEmpty {
            lines.append("Content-Type: \(serverType)")
        }
        for key in ["Content-Length", "Content-Range", "Last-Modified", "ETag"] {
            if let v = http.value(forHTTPHeaderField: key) { lines.append("\(key): \(v)") }
        }
        lines.append("Accept-Ranges: bytes")
        lines.append("Connection: close")
        let head = Data((lines.joined(separator: "\r\n") + "\r\n\r\n").utf8)
        relay.conn.send(content: head, completion: .contentProcessed { _ in })
        if relay.headOnly || !(200...299).contains(http.statusCode) {
            if !(200...299).contains(http.statusCode) {
                ArgoDiag.log("stream", "서버 응답 \(http.statusCode)")
            }
            completionHandler(.cancel)
            finish(relay, id: id)
            return
        }
        completionHandler(.allow)
    }

    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) {
        let id = dataTask.taskIdentifier
        guard let relay = relays[id], !relay.finished else { dataTask.cancel(); return }
        relay.pending += data.count
        if relay.pending > ArgoRemoteStream.highWater, !relay.suspended {
            relay.suspended = true
            dataTask.suspend()
        }
        relay.conn.send(content: data, completion: .contentProcessed { [weak self, weak dataTask] error in
            guard let self = self else { return }
            relay.pending -= data.count
            if error != nil {
                dataTask?.cancel()
                self.drop(id)
                relay.conn.cancel()
                return
            }
            if relay.suspended, relay.pending < ArgoRemoteStream.lowWater {
                relay.suspended = false
                dataTask?.resume()
            }
        })
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        let id = task.taskIdentifier
        guard let relay = relays[id] else { return }
        if let error = error as NSError?, error.code != NSURLErrorCancelled {
            ArgoDiag.log("stream", "받기 실패 \(error.localizedDescription)")
            drop(id)
            relay.conn.cancel()
            return
        }
        finish(relay, id: id)
    }
}

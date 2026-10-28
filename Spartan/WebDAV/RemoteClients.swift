//
//  RemoteClients.swift
//  Argo
//
//  원격 연결 공통 틀 + FTP.
//
//  WebDAV 화면(목록·받기·보기·설치)을 그대로 쓰고, 연결 종류만 늘린다.
//  화면은 RemoteFileClient 만 알고, 어느 종류인지는 모른다.
//
//  - WebDAV: 기존 WebDAVClient.
//  - FTP: Network.framework 로 직접 말한다. URLSession 은 FTP 를 더는
//    지원하지 않는다. 일반 FTP(암호화 없음)만 한다.
//

import Foundation
import Network

// MARK: - 공통

protocol RemoteFileClient: AnyObject {
    var server: WebDAVServer { get }
    /// 처음 보여 줄 경로.
    var startPath: String { get }
    func list(path: String, completion: @escaping (Result<[WebDAVItem], Error>) -> Void)
    func testConnection(completion: @escaping (Result<Void, Error>) -> Void)
    func fetch(item: WebDAVItem, to localPath: String,
               progress: @escaping (Double) -> Void,
               completion: @escaping (Result<String, Error>) -> Void)
    /// 설치기가 직접 받아 갈 수 있는 주소. 없으면 nil.
    func directURL(for item: WebDAVItem) -> URL?
}

enum RemoteClients {
    static func make(_ server: WebDAVServer) -> RemoteFileClient {
        switch server.kind {
        case .webdav: return WebDAVClient(server: server)
        case .ftp:    return FTPClient(server: server)
        }
    }
}

extension WebDAVClient: RemoteFileClient {
    var startPath: String { server.rootPath }

    func fetch(item: WebDAVItem, to localPath: String,
               progress: @escaping (Double) -> Void,
               completion: @escaping (Result<String, Error>) -> Void) {
        download(item: item, to: localPath, progress: progress, completion: completion)
    }

    func directURL(for item: WebDAVItem) -> URL? { url(forPath: item.path) }
}

enum RemoteError: LocalizedError {
    case message(String)
    var errorDescription: String? {
        switch self { case .message(let m): return m }
    }
}

/// 받는 자리에 같은 이름이 있으면 비켜 쓴다. 덮어쓰지 않는다.
private func freshLocalPath(_ path: String) -> String {
    guard FileManager.default.fileExists(atPath: path) else { return path }
    let ns = path as NSString
    let dir = ns.deletingLastPathComponent
    let ext = ns.pathExtension
    let base = (ns.lastPathComponent as NSString).deletingPathExtension
    for n in 2..<1000 {
        let name = ext.isEmpty ? "\(base) \(n)" : "\(base) \(n).\(ext)"
        let p = (dir as NSString).appendingPathComponent(name)
        if !FileManager.default.fileExists(atPath: p) { return p }
    }
    return path
}

// MARK: - FTP

/// 일반 FTP. 작업마다 연결을 새로 열고 닫는다 — 오래 붙잡아 둔 연결이
/// 서버 쪽에서 끊겨 다음 작업이 이상하게 실패하는 일을 없앤다.
final class FTPClient: RemoteFileClient {
    let server: WebDAVServer
    init(server: WebDAVServer) { self.server = server }

    var startPath: String { server.rootPath }

    private var host: String { URLComponents(string: server.urlString)?.host ?? "" }
    private var port: UInt16 { UInt16(URLComponents(string: server.urlString)?.port ?? 21) }

    func list(path: String, completion: @escaping (Result<[WebDAVItem], Error>) -> Void) {
        let session = FTPSession(host: host, port: port)
        session.login(user: server.username, password: server.password) { r in
            if case .failure(let e) = r { session.close(); return completion(.failure(e)) }
            // MLSD 가 되면 그것을, 안 되면 LIST 를 쓴다. MLSD 는 형식이 정해져 있다.
            session.transferText(command: "MLSD " + path) { r in
                switch r {
                case .success(let text):
                    session.close()
                    completion(.success(FTPParse.mlsd(text, in: path)))
                case .failure:
                    session.transferText(command: "LIST " + path) { r in
                        session.close()
                        switch r {
                        case .success(let text): completion(.success(FTPParse.list(text, in: path)))
                        case .failure(let e): completion(.failure(e))
                        }
                    }
                }
            }
        }
    }

    func testConnection(completion: @escaping (Result<Void, Error>) -> Void) {
        let session = FTPSession(host: host, port: port)
        session.login(user: server.username, password: server.password) { r in
            session.close()
            completion(r)
        }
    }

    func fetch(item: WebDAVItem, to localPath: String,
               progress: @escaping (Double) -> Void,
               completion: @escaping (Result<String, Error>) -> Void) {
        let session = FTPSession(host: host, port: port)
        let dest = freshLocalPath(localPath)
        session.login(user: server.username, password: server.password) { r in
            if case .failure(let e) = r { session.close(); return completion(.failure(e)) }
            session.transferFile(command: "RETR " + item.path, to: dest, expected: item.size,
                                 progress: progress) { r in
                session.close()
                switch r {
                case .success: completion(.success(dest))
                case .failure(let e):
                    try? FileManager.default.removeItem(atPath: dest)
                    completion(.failure(e))
                }
            }
        }
    }

    func directURL(for item: WebDAVItem) -> URL? { nil }
}

/// 제어 연결 하나. 명령을 보내고 응답(세 자리 번호 + 글)을 받는다.
private final class FTPSession {
    private let host: String
    private let port: UInt16
    private let queue = DispatchQueue(label: "argo.ftp")
    private var control: NWConnection?
    private var buffer = Data()
    private var keepAlive: FTPSession?

    init(host: String, port: UInt16) {
        self.host = host
        self.port = port
        keepAlive = self
    }

    func close() {
        control?.cancel()
        control = nil
        keepAlive = nil
    }

    private func main(_ f: @escaping () -> Void) { DispatchQueue.main.async(execute: f) }

    // MARK: 연결·로그인

    func login(user: String, password: String, completion: @escaping (Result<Void, Error>) -> Void) {
        guard !host.isEmpty, let p = NWEndpoint.Port(rawValue: port) else {
            return main { completion(.failure(RemoteError.message(L("FTP 주소가 올바르지 않습니다")))) }
        }
        let c = NWConnection(host: NWEndpoint.Host(host), port: p, using: .tcp)
        control = c
        var started = false
        c.stateUpdateHandler = { [weak self] state in
            guard let self else { return }
            switch state {
            case .ready where !started:
                started = true
                self.readReply { r in
                    guard case .success(let (code, text)) = r, code == 220 else {
                        return self.main { completion(.failure(Self.fail(r, L("서버가 인사하지 않았습니다")))) }
                    }
                    _ = text
                    let name = user.isEmpty ? "anonymous" : user
                    self.command("USER " + name) { r in
                        guard case .success(let (code, _)) = r else {
                            return self.main { completion(.failure(Self.fail(r, ""))) }
                        }
                        let afterPass: (Result<(Int, String), Error>) -> Void = { r in
                            guard case .success(let (code, msg)) = r, code == 230 || code == 202 else {
                                return self.main { completion(.failure(Self.fail(r, L("로그인하지 못했습니다")))) }
                            }
                            _ = msg
                            // 한글 이름이 깨지지 않게 UTF-8 을 청한다. 거절해도 계속한다.
                            self.command("OPTS UTF8 ON") { _ in
                                self.command("TYPE I") { _ in
                                    self.main { completion(.success(())) }
                                }
                            }
                        }
                        if code == 230 {
                            afterPass(.success((230, "")))
                        } else if code == 331 || code == 332 {
                            self.command("PASS " + (user.isEmpty && password.isEmpty ? "argo@" : password), afterPass)
                        } else {
                            self.main { completion(.failure(Self.fail(r, L("로그인하지 못했습니다")))) }
                        }
                    }
                }
            case .failed(let e):
                if !started { started = true; self.main { completion(.failure(e)) } }
            case .waiting(let e):
                if !started { started = true; c.cancel(); self.main { completion(.failure(e)) } }
            default:
                break
            }
        }
        c.start(queue: queue)
    }

    private static func fail(_ r: Result<(Int, String), Error>, _ fallback: String) -> Error {
        switch r {
        case .failure(let e): return e
        case .success(let (code, text)):
            let t = text.trimmingCharacters(in: .whitespacesAndNewlines)
            return RemoteError.message(fallback.isEmpty ? "\(code) \(t)" : "\(fallback) (\(code) \(t))")
        }
    }

    // MARK: 명령·응답

    func command(_ line: String, _ completion: @escaping (Result<(Int, String), Error>) -> Void) {
        guard let c = control else { return completion(.failure(RemoteError.message(L("연결이 끊겼습니다")))) }
        c.send(content: (line + "\r\n").data(using: .utf8), completion: .contentProcessed { [weak self] e in
            if let e { return completion(.failure(e)) }
            self?.readReply(completion)
        })
    }

    /// 응답 하나. "123-" 로 시작하면 "123 " 줄이 나올 때까지 여러 줄이다.
    private func readReply(_ completion: @escaping (Result<(Int, String), Error>) -> Void) {
        if let reply = takeReply() { return completion(.success(reply)) }
        guard let c = control else { return completion(.failure(RemoteError.message(L("연결이 끊겼습니다")))) }
        c.receive(minimumIncompleteLength: 1, maximumLength: 64 * 1024) { [weak self] data, _, done, e in
            guard let self else { return }
            if let data { self.buffer.append(data) }
            if let reply = self.takeReply() { return completion(.success(reply)) }
            if let e { return completion(.failure(e)) }
            if done { return completion(.failure(RemoteError.message(L("서버가 연결을 끊었습니다")))) }
            self.readReply(completion)
        }
    }

    private func takeReply() -> (Int, String)? {
        guard let text = String(data: buffer, encoding: .utf8) ?? String(data: buffer, encoding: .isoLatin1) else { return nil }
        let lines = text.components(separatedBy: "\r\n")
        guard lines.count > 1, let first = lines.first, first.count >= 3,
              let code = Int(first.prefix(3)) else { return nil }
        let multi = first.count > 3 && first[first.index(first.startIndex, offsetBy: 3)] == "-"
        var endLine = 0
        if multi {
            guard let i = lines.indices.dropFirst().first(where: {
                lines[$0].hasPrefix("\(code) ") && $0 < lines.count - 1
            }) else { return nil }
            endLine = i
        }
        let used = lines[0...endLine].joined(separator: "\r\n") + "\r\n"
        let consumed = used.data(using: .utf8)?.count ?? used.utf8.count
        buffer.removeFirst(min(consumed, buffer.count))
        let message = lines[0...endLine].map { String($0.dropFirst(min(4, $0.count))) }.joined(separator: "\n")
        return (code, message)
    }

    // MARK: 자료 연결

    /// EPSV, 안 되면 PASV 로 자료 연결의 포트를 얻는다. 주소는 늘 제어
    /// 연결의 호스트를 쓴다 — 공유기 뒤의 서버는 PASV 로 안쪽 주소를 알려 와
    /// 그대로 따라가면 닿지 않는다.
    private func openData(_ completion: @escaping (Result<NWConnection, Error>) -> Void) {
        command("EPSV") { [weak self] r in
            guard let self else { return }
            if case .success(let (code, text)) = r, code == 229,
               let open = text.firstIndex(of: "("), let close = text.lastIndex(of: ")") {
                let inner = text[text.index(after: open)..<close]
                let digits = inner.split(separator: "|").last.flatMap { UInt16($0) }
                if let port = digits { return self.connectData(port, completion) }
            }
            self.command("PASV") { r in
                guard case .success(let (code, text)) = r, code == 227 else {
                    return completion(.failure(Self.fail(r, L("자료 연결을 열지 못했습니다"))))
                }
                let nums = text.split(whereSeparator: { !$0.isNumber }).compactMap { Int($0) }
                guard nums.count >= 6 else {
                    return completion(.failure(RemoteError.message(L("자료 연결을 열지 못했습니다"))))
                }
                let p1 = nums[nums.count - 2], p2 = nums[nums.count - 1]
                self.connectData(UInt16(p1 * 256 + p2), completion)
            }
        }
    }

    private func connectData(_ port: UInt16, _ completion: @escaping (Result<NWConnection, Error>) -> Void) {
        guard let p = NWEndpoint.Port(rawValue: port) else {
            return completion(.failure(RemoteError.message(L("자료 연결을 열지 못했습니다"))))
        }
        let d = NWConnection(host: NWEndpoint.Host(host), port: p, using: .tcp)
        var settled = false
        d.stateUpdateHandler = { state in
            switch state {
            case .ready where !settled: settled = true; completion(.success(d))
            case .failed(let e) where !settled: settled = true; completion(.failure(e))
            case .waiting(let e) where !settled: settled = true; d.cancel(); completion(.failure(e))
            default: break
            }
        }
        d.start(queue: queue)
    }

    /// 목록처럼 글로 오는 자료.
    func transferText(command line: String, _ completion: @escaping (Result<String, Error>) -> Void) {
        var collected = Data()
        transfer(command: line, sink: { collected.append($0); return true }) { r in
            switch r {
            case .success:
                let s = String(data: collected, encoding: .utf8)
                    ?? String(data: collected, encoding: ArgoTextCodec.cp949)
                    ?? String(data: collected, encoding: .isoLatin1) ?? ""
                self.main { completion(.success(s)) }
            case .failure(let e):
                self.main { completion(.failure(e)) }
            }
        }
    }

    /// 파일로 받는 자료.
    func transferFile(command line: String, to path: String, expected: Int64,
                      progress: @escaping (Double) -> Void,
                      _ completion: @escaping (Result<Void, Error>) -> Void) {
        guard FileManager.default.createFile(atPath: path, contents: nil),
              let out = FileHandle(forWritingAtPath: path) else {
            return main { completion(.failure(RemoteError.message(L("받을 파일을 만들지 못했습니다")))) }
        }
        var got: Int64 = 0
        var last = Date.distantPast
        transfer(command: line, sink: { data in
            out.write(data)
            got += Int64(data.count)
            if expected > 0, Date().timeIntervalSince(last) > 0.15 {
                last = Date()
                let f = Double(got) / Double(expected)
                DispatchQueue.main.async { progress(min(1, f)) }
            }
            return true
        }) { r in
            out.closeFile()
            self.main {
                switch r {
                case .success: progress(1); completion(.success(()))
                case .failure(let e): completion(.failure(e))
                }
            }
        }
    }

    /// 자료 연결을 열고, 명령을 보내고, 끝까지 받은 뒤 226 을 기다린다.
    private func transfer(command line: String, sink: @escaping (Data) -> Bool,
                          _ completion: @escaping (Result<Void, Error>) -> Void) {
        openData { [weak self] r in
            guard let self else { return }
            guard case .success(let data) = r else {
                if case .failure(let e) = r { completion(.failure(e)) }
                return
            }
            self.command(line) { r in
                guard case .success(let (code, _)) = r, code == 150 || code == 125 else {
                    data.cancel()
                    return completion(.failure(Self.fail(r, "")))
                }
                self.drain(data, sink: sink) { e in
                    data.cancel()
                    if let e { return completion(.failure(e)) }
                    self.readReply { r in
                        guard case .success(let (code, _)) = r, code == 226 || code == 250 else {
                            return completion(.failure(Self.fail(r, L("전송이 끝나지 않았습니다"))))
                        }
                        completion(.success(()))
                    }
                }
            }
        }
    }

    private func drain(_ c: NWConnection, sink: @escaping (Data) -> Bool, done: @escaping (Error?) -> Void) {
        c.receive(minimumIncompleteLength: 1, maximumLength: 256 * 1024) { [weak self] data, _, complete, e in
            if let data, !data.isEmpty { _ = sink(data) }
            if complete { return done(nil) }
            if let e {
                // 서버가 다 보낸 뒤 바로 끊으면 오류로 올 때가 있다. 받은 것이 있으면 끝으로 본다.
                if case .posix(let code) = e, code == .ECONNRESET || code == .ENOTCONN { return done(nil) }
                return done(e)
            }
            self?.drain(c, sink: sink, done: done)
        }
    }
}

/// FTP 목록 읽기.
enum FTPParse {
    static func mlsd(_ text: String, in dir: String) -> [WebDAVItem] {
        var out: [WebDAVItem] = []
        for raw in text.components(separatedBy: .newlines) where !raw.isEmpty {
            guard let space = raw.firstIndex(of: " ") else { continue }
            let facts = raw[..<space].split(separator: ";")
            let name = String(raw[raw.index(after: space)...])
            var type = "", size: Int64 = 0, modified: Date?
            for f in facts {
                let kv = f.split(separator: "=", maxSplits: 1)
                guard kv.count == 2 else { continue }
                switch kv[0].lowercased() {
                case "type": type = kv[1].lowercased()
                case "size": size = Int64(kv[1]) ?? 0
                case "modify": modified = mdtm(String(kv[1]))
                default: break
                }
            }
            if type == "cdir" || type == "pdir" || name == "." || name == ".." { continue }
            let isDir = type == "dir"
            out.append(WebDAVItem(path: WebDAVClient.child(dir, name) + (isDir ? "/" : ""),
                                  name: name, isDirectory: isDir, size: size,
                                  modified: modified, contentType: ""))
        }
        return sorted(out)
    }

    /// 유닉스식 LIST: drwxr-xr-x 2 user group 4096 Jan 01 12:00 name
    static func list(_ text: String, in dir: String) -> [WebDAVItem] {
        var out: [WebDAVItem] = []
        for raw in text.components(separatedBy: .newlines) where !raw.isEmpty {
            let parts = raw.split(separator: " ", omittingEmptySubsequences: true)
            guard parts.count >= 9, let first = parts.first?.first, "d-l".contains(first) else { continue }
            // 이름에 빈칸이 있을 수 있어, 여덟 칸 뒤의 원래 글을 그대로 쓴다.
            var rest = Substring(raw)
            for _ in 0..<8 {
                rest = rest.drop(while: { $0 == " " })
                rest = rest.drop(while: { $0 != " " })
            }
            var name = rest.drop(while: { $0 == " " })
            if first == "l", let arrow = name.range(of: " -> ") { name = name[..<arrow.lowerBound] }
            let n = String(name)
            if n == "." || n == ".." || n.isEmpty { continue }
            let isDir = first == "d"
            out.append(WebDAVItem(path: WebDAVClient.child(dir, n) + (isDir ? "/" : ""),
                                  name: n, isDirectory: isDir, size: Int64(parts[4]) ?? 0,
                                  modified: nil, contentType: ""))
        }
        return sorted(out)
    }

    private static func mdtm(_ s: String) -> Date? {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.timeZone = TimeZone(identifier: "UTC")
        f.dateFormat = "yyyyMMddHHmmss"
        return f.date(from: String(s.prefix(14)))
    }

    private static func sorted(_ items: [WebDAVItem]) -> [WebDAVItem] {
        items.sorted {
            if $0.isDirectory != $1.isDirectory { return $0.isDirectory }
            return $0.name.localizedStandardCompare($1.name) == .orderedAscending
        }
    }
}

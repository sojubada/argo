//
//  WebDAVModels.swift
//  Argo
//
//  WebDAV support: data models and shared helpers.
//

import Foundation

/// 연결 종류. 이름은 예전 그대로 WebDAVServer 지만 FTP 도 담는다 —
/// 저장해 둔 연결을 옮겨 적지 않고 쓰려고 이름을 바꾸지 않았다.
enum RemoteKind: String, Codable, CaseIterable {
    case webdav, ftp

    var label: String {
        switch self {
        case .webdav: return "WebDAV"
        case .ftp:    return "FTP"
        }
    }

    var symbol: String {
        switch self {
        case .webdav: return "globe"
        case .ftp:    return "server.rack"
        }
    }
}

/// A saved remote connection.
struct WebDAVServer: Codable, Identifiable, Hashable {
    var id: UUID = UUID()
    var kind: RemoteKind = .webdav
    var name: String = ""
    /// Full base URL, e.g. https://nas.local:5006/dav
    var urlString: String = ""
    var username: String = ""
    var password: String = ""
    /// Accept self-signed / untrusted certificates (common on home NAS boxes).
    var allowInsecureTLS: Bool = false

    var displayName: String {
        if !name.isEmpty { return name }
        if let host = URLComponents(string: urlString)?.host { return host }
        return urlString
    }

    /// Path component of the base URL, always starting with "/".
    var rootPath: String {
        guard let comps = URLComponents(string: urlString) else { return "/" }
        var path = comps.path
        if path.isEmpty { path = "/" }
        if !path.hasPrefix("/") { path = "/" + path }
        return path
    }

    var isValid: Bool {
        switch kind {
        case .ftp:
            guard let comps = URLComponents(string: urlString), comps.scheme?.lowercased() == "ftp" else { return false }
            return comps.host?.isEmpty == false
        case .webdav:
            guard let comps = URLComponents(string: urlString), let scheme = comps.scheme?.lowercased() else { return false }
            return (scheme == "http" || scheme == "https") && (comps.host?.isEmpty == false)
        }
    }

    init() {}

    private enum CodingKeys: String, CodingKey {
        case id, kind, name, urlString, username, password, allowInsecureTLS
    }

    /// 예전에 저장한 연결에는 kind 가 없다. 없으면 WebDAV 로 읽는다 —
    /// 그대로 두면 디코딩이 실패해 저장한 연결이 전부 사라진다.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decodeIfPresent(UUID.self, forKey: .id) ?? UUID()
        kind = (try? c.decodeIfPresent(RemoteKind.self, forKey: .kind)) ?? .webdav
        name = try c.decodeIfPresent(String.self, forKey: .name) ?? ""
        urlString = try c.decodeIfPresent(String.self, forKey: .urlString) ?? ""
        username = try c.decodeIfPresent(String.self, forKey: .username) ?? ""
        password = try c.decodeIfPresent(String.self, forKey: .password) ?? ""
        allowInsecureTLS = try c.decodeIfPresent(Bool.self, forKey: .allowInsecureTLS) ?? false
    }
}

/// One entry returned by a PROPFIND.
struct WebDAVItem: Identifiable, Hashable {
    var id: String { path }
    /// Decoded absolute path on the server, e.g. "/dav/Movies/clip.mkv"
    var path: String
    var name: String
    var isDirectory: Bool
    var size: Int64
    var modified: Date?
    var contentType: String

    var isIPA: Bool {
        let ext = (name as NSString).pathExtension.lowercased()
        return ext == "ipa" || ext == "tipa"
    }

    var sizeDescription: String {
        if isDirectory { return "" }
        let formatter = ByteCountFormatter()
        formatter.countStyle = .file
        return formatter.string(fromByteCount: size)
    }

    var modifiedDescription: String {
        guard let modified = modified else { return "" }
        let formatter = DateFormatter()
        formatter.dateFormat = UserDefaults.settings.string(forKey: "dateFormat") ?? "MM-dd-yyyy HH:mm"
        return formatter.string(from: modified)
    }
}

enum WebDAVError: LocalizedError {
    case badURL
    case http(Int)
    case transport(String)
    case emptyResponse
    case parseFailure

    var errorDescription: String? {
        switch self {
        case .badURL:
            return "Invalid server URL"
        case .http(let code):
            switch code {
            case 401: return "Authentication failed (401)"
            case 403: return "Access denied (403)"
            case 404: return "Not found (404)"
            case 405: return "Server rejected the request (405) - is this a WebDAV endpoint?"
            case 507: return "Server is out of storage (507)"
            default:  return "Server returned HTTP \(code)"
            }
        case .transport(let message):
            return message
        case .emptyResponse:
            return "Server returned an empty response"
        case .parseFailure:
            return "Could not parse the server response"
        }
    }
}

/// Where downloaded files are staged before being viewed or installed.
enum WebDAVCache {
    static let preferredDirectory = "/private/var/mobile/Documents/Argo"

    static var directory: String {
        let fm = FileManager.default
        if fm.fileExists(atPath: preferredDirectory) { return preferredDirectory }
        do {
            try fm.createDirectory(atPath: preferredDirectory, withIntermediateDirectories: true)
            return preferredDirectory
        } catch {
            let fallback = (fm.urls(for: .documentDirectory, in: .userDomainMask).first
                            ?? URL(fileURLWithPath: NSTemporaryDirectory()))
                .appendingPathComponent("Argo")
            try? fm.createDirectory(at: fallback, withIntermediateDirectories: true)
            return fallback.path
        }
    }

    static func localPath(for item: WebDAVItem) -> String {
        return (directory as NSString).appendingPathComponent(item.name)
    }

    static func clear() {
        let fm = FileManager.default
        guard let contents = try? fm.contentsOfDirectory(atPath: directory) else { return }
        for file in contents {
            try? fm.removeItem(atPath: (directory as NSString).appendingPathComponent(file))
        }
    }

    static var usedBytes: Int64 {
        let fm = FileManager.default
        guard let contents = try? fm.contentsOfDirectory(atPath: directory) else { return 0 }
        var total: Int64 = 0
        for file in contents {
            let path = (directory as NSString).appendingPathComponent(file)
            if let attrs = try? fm.attributesOfItem(atPath: path), let size = attrs[.size] as? Int64 {
                total += size
            }
        }
        return total
    }
}

extension UserDefaults {
    static var webdav: UserDefaults {
        return UserDefaults(suiteName: "com.sojubada.argo.webdav") ?? UserDefaults.standard
    }
}

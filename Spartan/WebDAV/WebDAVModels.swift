//
//  WebDAVModels.swift
//  Argo
//
//  WebDAV support: data models and shared helpers.
//

import Foundation

/// A saved WebDAV connection.
struct WebDAVServer: Codable, Identifiable, Hashable {
    var id: UUID = UUID()
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
        guard let comps = URLComponents(string: urlString), let scheme = comps.scheme?.lowercased() else { return false }
        return (scheme == "http" || scheme == "https") && (comps.host?.isEmpty == false)
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

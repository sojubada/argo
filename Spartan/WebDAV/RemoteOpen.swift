//
//  RemoteOpen.swift
//  Argo
//
//  원격(WebDAV·FTP) 파일을 로컬 파일처럼 여는 데 필요한 것들.
//
//  · 음악·동영상(WebDAV): 받지 않고 ArgoRemoteStream 으로 바로 재생한다.
//  · 사진: 사진 앨범으로 열고, 넘길 때마다 그 장과 이웃 장만 받는다.
//  · 문서·그 밖의 파일: 임시 보관함(tmp)에 받아 로컬 파일과 같은 뷰어로 연다.
//    받는 곳이 tmp 라 '미디어 폴더' 가 쌓이지 않고, 같은 파일을 다시 열면 다시 받지 않는다.
//

import Foundation

enum ArgoRemoteKind {
    case video, audio, image, unsupportedMedia, other

    static let videoExtensions: Set<String> = ["mp4", "m4v", "mov", "qt", "3gp", "ts"]

    static func of(_ name: String) -> ArgoRemoteKind {
        let ext = (name as NSString).pathExtension.lowercased()
        if FileInfo.unsupportedMediaExtensions.contains(ext) { return .unsupportedMedia }
        if videoExtensions.contains(ext) { return .video }
        if MusicLookup.audioExtensions.contains(ext) { return .audio }
        if ArgoPhotoAlbumController.imageExtensions.contains(ext) { return .image }
        return .other
    }
}

enum ArgoRemoteCache {

    /// 임시 보관함 상한. 넘으면 오래 안 쓴 파일부터 지운다.
    static let maxBytes: Int64 = 2 * 1024 * 1024 * 1024

    static var root: String {
        (NSTemporaryDirectory() as NSString).appendingPathComponent("ArgoRemote")
    }

    /// 원격 폴더 하나 = 임시 폴더 하나. 이름이 같은 파일이 다른 폴더에 있어도 섞이지 않는다.
    static func folder(server: WebDAVServer, remoteFolder: String) -> String {
        let key = server.id.uuidString + "|" + WebDAVClient.normalize(remoteFolder)
        let dir = (root as NSString).appendingPathComponent(String(format: "%016llx", fnv1a(key)))
        try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        return dir
    }

    static func localPath(server: WebDAVServer, item: WebDAVItem) -> String {
        let dir = folder(server: server, remoteFolder: WebDAVClient.parent(of: item.path))
        // 서버가 준 이름에 / 가 섞여 있어도 보관함 밖으로 나가지 않게.
        let safe = item.name.replacingOccurrences(of: "/", with: "_")
        return (dir as NSString).appendingPathComponent(safe.isEmpty ? "file" : safe)
    }

    /// 이미 받은 것이 있고 크기가 같으면 다시 받지 않는다.
    static func isFresh(_ path: String, item: WebDAVItem) -> Bool {
        guard let attrs = try? FileManager.default.attributesOfItem(atPath: path),
              let size = attrs[.size] as? Int64 else { return false }
        if item.size > 0 { return size == item.size }
        return size > 0
    }

    /// 받기 전에 부른다. 덜 받다 만 파일을 지우고(FTP 는 같은 이름이 있으면 비켜 쓴다),
    /// 보관함이 상한을 넘었으면 오래된 것부터 지운다.
    static func prepare(_ path: String) {
        try? FileManager.default.removeItem(atPath: path)
        trim()
    }

    static func touch(_ path: String) {
        try? FileManager.default.setAttributes([.modificationDate: Date()], ofItemAtPath: path)
    }

    static func trim() {
        let fm = FileManager.default
        guard let e = fm.enumerator(atPath: root) else { return }
        var files: [(String, Int64, Date)] = []
        var total: Int64 = 0
        while let rel = e.nextObject() as? String {
            let p = (root as NSString).appendingPathComponent(rel)
            guard let a = try? fm.attributesOfItem(atPath: p),
                  (a[.type] as? FileAttributeType) == .typeRegular else { continue }
            let s = (a[.size] as? Int64) ?? 0
            total += s
            files.append((p, s, (a[.modificationDate] as? Date) ?? .distantPast))
        }
        guard total > maxBytes else { return }
        for f in files.sorted(by: { $0.2 < $1.2 }) {
            try? fm.removeItem(atPath: f.0)
            total -= f.1
            if total <= maxBytes * 3 / 4 { break }
        }
    }

    static func clearAll() {
        try? FileManager.default.removeItem(atPath: root)
    }

    /// 백그라운드에서 부르는 동기 받기(사진 앨범이 넘길 때 쓴다). 받은 경로나 nil.
    static func fetchSync(client: RemoteFileClient, item: WebDAVItem, timeout: TimeInterval = 180) -> String? {
        let path = localPath(server: client.server, item: item)
        if isFresh(path, item: item) { touch(path); return path }
        prepare(path)
        let done = DispatchSemaphore(value: 0)
        var saved: String?
        DispatchQueue.main.async {
            client.fetch(item: item, to: path, progress: { _ in }, completion: { result in
                if case .success(let p) = result { saved = p }
                done.signal()
            })
        }
        if done.wait(timeout: .now() + timeout) == .timedOut { return nil }
        return saved
    }

    private static func fnv1a(_ s: String) -> UInt64 {
        var h: UInt64 = 0xcbf29ce484222325
        for b in s.utf8 {
            h ^= UInt64(b)
            h = h &* 0x100000001b3
        }
        return h
    }
}

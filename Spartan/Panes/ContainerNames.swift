//
//  ContainerNames.swift
//  Argo
//
//  앱 컨테이너 폴더(UUID 이름)에 앱 이름을 붙인다 — Filza 처럼.
//
//  /var/mobile/Containers/Data/Application/<UUID>     앱의 자료
//  /var/containers/Bundle/Application/<UUID>          앱 본체(.app)
//  /var/mobile/Containers/Shared/AppGroup/<UUID>      앱 그룹
//  /var/mobile/Containers/Data/PluginKitPlugin/<UUID> 확장
//
//  자료 쪽 폴더에는 `.com.apple.mobile_container_manager.metadata.plist` 가 있어
//  주인의 번들 ID 를 적어 둔다. 그 ID 로 설치된 앱의 Info.plist 에서 이름을 읽는다.
//  한 번 읽은 것은 기억해 둔다 — 목록을 그릴 때마다 디스크를 치지 않게.
//

import Foundation
import ApplicationsWrapper

struct ContainerLabel {
    /// 보일 이름 (앱 이름, 없으면 번들 ID)
    let name: String
    /// 번들 ID 나 그룹 ID
    let identifier: String
}

enum ContainerNames {
    private static let lock = NSLock()
    private static var cache: [String: ContainerLabel?] = [:]
    private static var appNames: [String: String]?

    private static let parents: Set<String> = [
        "/var/mobile/Containers/Data/Application",
        "/var/mobile/Containers/Shared/AppGroup",
        "/var/mobile/Containers/Data/PluginKitPlugin",
        "/var/containers/Bundle/Application",
        "/var/mobile/Containers/Bundle/Application",
    ]

    /// UUID 꼴 이름인가 (8-4-4-4-12)
    private static func looksLikeUUID(_ s: String) -> Bool {
        let parts = s.split(separator: "-")
        guard parts.count == 5 else { return false }
        let lens = parts.map { $0.count }
        guard lens == [8, 4, 4, 4, 12] else { return false }
        return s.allSatisfy { $0.isHexDigit || $0 == "-" }
    }

    private static func normalize(_ path: String) -> String {
        var p = path
        while p.count > 1 && p.hasSuffix("/") { p.removeLast() }
        if p.hasPrefix("/private/") { p = String(p.dropFirst("/private".count)) }
        return p
    }

    /// 이 폴더가 컨테이너 폴더면 붙일 이름. 아니면 nil.
    static func label(for fullPath: String) -> ContainerLabel? {
        let path = normalize(fullPath)
        let parent = (path as NSString).deletingLastPathComponent
        let leaf = (path as NSString).lastPathComponent
        guard parents.contains(parent), looksLikeUUID(leaf) else { return nil }

        lock.lock()
        if let hit = cache[path] {
            lock.unlock()
            return hit
        }
        lock.unlock()

        let made = compute(path: path, parent: parent)
        lock.lock()
        cache[path] = made
        lock.unlock()
        return made
    }

    /// 목록을 새로 읽을 때(앱을 깔거나 지운 뒤) 비운다.
    static func reset() {
        lock.lock()
        cache.removeAll()
        appNames = nil
        lock.unlock()
    }

    // MARK: - 계산

    private static func compute(path: String, parent: String) -> ContainerLabel? {
        if parent.hasSuffix("/Bundle/Application") {
            // 앱 본체: 안의 .app 을 직접 읽는다.
            guard let app = (try? FileManager.default.contentsOfDirectory(atPath: path))?
                .first(where: { $0.hasSuffix(".app") }) else { return nil }
            let appPath = (path as NSString).appendingPathComponent(app)
            let info = NSDictionary(contentsOfFile: (appPath as NSString).appendingPathComponent("Info.plist"))
            let id = info?["CFBundleIdentifier"] as? String ?? app
            let name = displayName(appPath: appPath, info: info) ?? (app as NSString).deletingPathExtension
            return ContainerLabel(name: name, identifier: id)
        }

        let meta = NSDictionary(contentsOfFile: (path as NSString)
            .appendingPathComponent(".com.apple.mobile_container_manager.metadata.plist"))
        guard let id = meta?["MCMMetadataIdentifier"] as? String, !id.isEmpty else { return nil }
        let names = installedAppNames()

        if parent.hasSuffix("/AppGroup") {
            // group.com.회사.앱 → 그 앱의 이름을 찾아 본다.
            let bare = id.hasPrefix("group.") ? String(id.dropFirst("group.".count)) : id
            if let owner = bestOwner(of: bare, in: names) {
                return ContainerLabel(name: LF("{0} (그룹)", owner), identifier: id)
            }
            return ContainerLabel(name: id, identifier: id)
        }
        if parent.hasSuffix("/PluginKitPlugin") {
            if let owner = bestOwner(of: id, in: names) {
                return ContainerLabel(name: LF("{0} (확장)", owner), identifier: id)
            }
            return ContainerLabel(name: id, identifier: id)
        }
        return ContainerLabel(name: names[id] ?? id, identifier: id)
    }

    /// 번들 ID 가 가장 길게 겹치는 설치된 앱의 이름.
    private static func bestOwner(of id: String, in names: [String: String]) -> String? {
        if let exact = names[id] { return exact }
        var best: (Int, String)?
        for (bundle, name) in names where id.hasPrefix(bundle + ".") || id.hasPrefix(bundle) {
            if best == nil || bundle.count > best!.0 { best = (bundle.count, name) }
        }
        return best?.1
    }

    /// 설치된 앱: 번들 ID → 보일 이름
    private static func installedAppNames() -> [String: String] {
        lock.lock()
        if let names = appNames {
            lock.unlock()
            return names
        }
        lock.unlock()

        var out: [String: String] = [:]
        for proxy in ApplicationsManager.shared.allApps {
            let maybeID: String? = proxy.applicationIdentifier()
            guard let id = maybeID else { continue }
            let appPath = proxy.bundleURL().path
            let info = NSDictionary(contentsOfFile: (appPath as NSString).appendingPathComponent("Info.plist"))
            if let name = displayName(appPath: appPath, info: info) {
                out[id] = name
            }
        }
        // LaunchServices 가 모르는 TrollStore 앱도 본체 폴더에서 줍는다.
        for root in ["/var/containers/Bundle/Application", "/private/var/containers/Bundle/Application"] {
            guard let dirs = try? FileManager.default.contentsOfDirectory(atPath: root) else { continue }
            for dir in dirs {
                let container = (root as NSString).appendingPathComponent(dir)
                guard let app = (try? FileManager.default.contentsOfDirectory(atPath: container))?
                    .first(where: { $0.hasSuffix(".app") }) else { continue }
                let appPath = (container as NSString).appendingPathComponent(app)
                let info = NSDictionary(contentsOfFile: (appPath as NSString).appendingPathComponent("Info.plist"))
                guard let id = info?["CFBundleIdentifier"] as? String, out[id] == nil else { continue }
                out[id] = displayName(appPath: appPath, info: info) ?? (app as NSString).deletingPathExtension
            }
            break
        }
        lock.lock()
        appNames = out
        lock.unlock()
        return out
    }

    /// 앱 화면에 보이는 이름. 앱 언어(한국어/영어)에 맞춘 번역이 있으면 그것.
    private static func displayName(appPath: String, info: NSDictionary?) -> String? {
        let lang = ArgoLocale.current
        let candidates = lang == "ko" ? ["ko", "ko-KR", "Korean"] : ["en", "Base", "English"]
        for l in candidates {
            let strings = (appPath as NSString).appendingPathComponent("\(l).lproj/InfoPlist.strings")
            if let d = NSDictionary(contentsOfFile: strings) {
                if let n = (d["CFBundleDisplayName"] ?? d["CFBundleName"]) as? String, !n.isEmpty { return n }
            }
        }
        if let n = (info?["CFBundleDisplayName"] ?? info?["CFBundleName"]) as? String, !n.isEmpty { return n }
        return nil
    }
}

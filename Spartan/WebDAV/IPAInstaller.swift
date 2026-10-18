//
//  IPAInstaller.swift
//  Argo
//
//  Hands a local .ipa/.tipa to TrollStore for installation.
//
//  Two routes are supported:
//   1. Spawn TrollStore's own root helper directly (preferred - no UI handoff,
//      works for files we already downloaded to the cache).
//   2. Fall back to TrollStore's URL scheme, which makes TrollStore fetch the
//      IPA itself. Requires "URL Scheme" to be enabled in TrollStore settings.
//

import Foundation
import UIKit
import ApplicationsWrapper

enum IPAInstallRoute {
    case rootHelper(String)   // absolute path to trollstorehelper
    case urlScheme
    case unavailable
}

enum IPAInstallResult {
    case success(String)          // log output
    case failure(String)          // message
    case handedOff                // TrollStore took over via URL scheme
}

struct IPAInstaller {

    static let trollStoreBundleIDs = ["com.opa334.TrollStore", "com.opa334.TrollStoreLite"]

    // MARK: - Discovery

    /// Looks for TrollStore's bundled root helper on disk.
    static func locateRootHelper() -> String? {
        let fm = FileManager.default

        // 1. Ask LaunchServices where TrollStore lives.
        let installed = ApplicationsManager.shared.allApps
        for bundleID in trollStoreBundleIDs {
            guard let proxy = installed.first(where: { $0.applicationIdentifier() == bundleID }) else { continue }
            let helper = proxy.bundleURL().appendingPathComponent("trollstorehelper").path
            if fm.fileExists(atPath: helper) { return helper }
        }

        // 2. Fall back to scanning the usual install locations.
        let roots = ["/var/containers/Bundle/Application",
                     "/private/var/containers/Bundle/Application",
                     "/Applications"]
        for root in roots {
            guard let entries = try? fm.contentsOfDirectory(atPath: root) else { continue }
            for entry in entries {
                let container = (root as NSString).appendingPathComponent(entry)
                // /Applications/TrollStore.app/trollstorehelper
                let direct = (container as NSString).appendingPathComponent("trollstorehelper")
                if fm.fileExists(atPath: direct) { return direct }
                // .../<uuid>/TrollStore.app/trollstorehelper
                guard let apps = try? fm.contentsOfDirectory(atPath: container) else { continue }
                for app in apps where app.hasSuffix(".app") {
                    let candidate = ((container as NSString).appendingPathComponent(app) as NSString)
                        .appendingPathComponent("trollstorehelper")
                    if fm.fileExists(atPath: candidate) { return candidate }
                }
            }
        }
        return nil
    }

    static func route() -> IPAInstallRoute {
        if let helper = locateRootHelper() { return .rootHelper(helper) }
        if let url = URL(string: "apple-magnifier://install?url=about:blank"),
           UIApplication.shared.canOpenURL(url) {
            return .urlScheme
        }
        return .unavailable
    }

    static var routeDescription: String {
        switch route() {
        case .rootHelper(let path):
            return "TrollStore helper: \((path as NSString).deletingLastPathComponent)"
        case .urlScheme:
            return "TrollStore URL scheme"
        case .unavailable:
            return "TrollStore not found"
        }
    }

    // MARK: - Installing

    /// Installs a local IPA. `remoteFallbackURL` is used when only the URL
    /// scheme is available, in which case TrollStore downloads the file itself.
    static func install(localPath: String,
                        remoteFallbackURL: URL? = nil,
                        completion: @escaping (IPAInstallResult) -> Void) {

        guard FileManager.default.fileExists(atPath: localPath) else {
            completion(.failure("File not found: \(localPath)"))
            return
        }

        switch route() {
        case .rootHelper(let helper):
            DispatchQueue.global(qos: .userInitiated).async {
                var stdOut = ""
                var stdErr = ""
                let status = spawnAsRoot(command: helper,
                                         args: ["install", "custom", localPath],
                                         stdOut: &stdOut,
                                         stdErr: &stdErr)
                let log = [stdOut, stdErr]
                    .filter { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
                    .joined(separator: "\n")
                DispatchQueue.main.async {
                    if status == 0 {
                        completion(.success(log.isEmpty ? "Installed." : log))
                    } else {
                        completion(.failure("trollstorehelper exited with \(status)\n\(log)"))
                    }
                }
            }

        case .urlScheme:
            guard let remote = remoteFallbackURL,
                  let escaped = remote.absoluteString.addingPercentEncoding(withAllowedCharacters: .alphanumerics),
                  let url = URL(string: "apple-magnifier://install?url=\(escaped)") else {
                completion(.failure("TrollStore's root helper was not found, and there is no remote URL to hand over."))
                return
            }
            UIApplication.shared.open(url, options: [:]) { opened in
                completion(opened ? .handedOff : .failure("TrollStore refused the install request. Check that the URL Scheme is enabled in TrollStore settings."))
            }

        case .unavailable:
            completion(.failure("TrollStore was not found on this device. Install TrollStore first, or copy the IPA somewhere you can install it from."))
        }
    }

    // MARK: - Root spawn

    /// Thin wrapper around TrollStore's own spawnRoot() (External/TSUtil.m),
    /// which is already part of this target and bridged into Swift. It spawns
    /// with a root persona, which needs com.apple.private.persona-mgmt - the
    /// entitlement the build already grants.
    @discardableResult
    static func spawnAsRoot(command: String,
                            args: [String],
                            stdOut: inout String,
                            stdErr: inout String) -> Int32 {
        var out: NSString?
        var err: NSString?
        let status = spawnRoot(command, args, &out, &err)
        stdOut = (out as String?) ?? ""
        stdErr = (err as String?) ?? ""
        return Int32(status)
    }
}

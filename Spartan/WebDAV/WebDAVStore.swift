//
//  WebDAVStore.swift
//  Argo
//
//  Persistence for saved WebDAV connections.
//

import Foundation
import Combine

final class WebDAVStore: ObservableObject {

    static let shared = WebDAVStore()

    private static let key = "servers"

    @Published var servers: [WebDAVServer] = [] {
        didSet { save() }
    }

    private init() {
        load()
    }

    private func load() {
        guard let data = UserDefaults.webdav.data(forKey: WebDAVStore.key) else { return }
        if let decoded = try? JSONDecoder().decode([WebDAVServer].self, from: data) {
            // Assigning here would re-enter save(); that is harmless but wasteful,
            // so write the backing value directly.
            servers = decoded
        }
    }

    private func save() {
        guard let data = try? JSONEncoder().encode(servers) else { return }
        UserDefaults.webdav.set(data, forKey: WebDAVStore.key)
        UserDefaults.webdav.synchronize()
    }

    func upsert(_ server: WebDAVServer) {
        if let index = servers.firstIndex(where: { $0.id == server.id }) {
            servers[index] = server
        } else {
            servers.append(server)
        }
    }

    func remove(_ server: WebDAVServer) {
        servers.removeAll { $0.id == server.id }
    }
}

//
//  FavoritesStore.swift
//  Argo
//

import Foundation
import Combine

struct FavoriteItem: Codable, Identifiable, Hashable {
    var id: UUID
    var name: String
    var path: String           // 항상 "/"로 끝나도록 정규화
    var systemImage: String    // SF Symbol 이름
}

final class FavoritesStore: ObservableObject {
    static let shared = FavoritesStore()

    private static let itemsKey = "favoriteItems"
    private static let migratedKey = "favoritesMigratedV1"
    private static let seededDefaultsKey = "favoritesSeededDefaultsV1"

    static let availableIcons: [String] = [
        "folder",
        "doc.text",
        "film",
        "music.note",
        "photo",
        "arrow.down.circle",
        "star",
        "gearshape",
        "externaldrive",
        "trash"
    ]

    @Published var items: [FavoriteItem] = [] {
        didSet {
            save()
        }
    }

    private var isLoading = false

    private init() {
        loadOrMigrate()
    }

    static func normalizePath(_ rawPath: String) -> String {
        let trimmed = rawPath.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return "/" }
        if trimmed.hasSuffix("/") {
            return trimmed
        }
        return trimmed + "/"
    }

    private func loadOrMigrate() {
        isLoading = true
        defer { isLoading = false }

        let defaults = UserDefaults.favorites

        // 1. 이미 새 단일 키("favoriteItems") 데이터가 존재하는 경우
        if let data = defaults.data(forKey: Self.itemsKey),
           let decoded = try? JSONDecoder().decode([FavoriteItem].self, from: data) {
            items = decoded.map { item in
                var copy = item
                copy.path = Self.normalizePath(item.path)
                return copy
            }
            defaults.set(true, forKey: Self.migratedKey)
            defaults.set(true, forKey: Self.seededDefaultsKey)
            return
        }

        // 2. 구버전 두 배열(favoritesDisplayName / favoritesFilePath) 마이그레이션
        if !defaults.bool(forKey: Self.migratedKey) {
            let oldNames = defaults.stringArray(forKey: "favoritesDisplayName")
            let oldPaths = defaults.stringArray(forKey: "favoritesFilePath")

            if let names = oldNames, let paths = oldPaths {
                let count = min(names.count, paths.count)
                var migrated: [FavoriteItem] = []

                for i in 0..<count {
                    let normPath = Self.normalizePath(paths[i])
                    let icon = Self.defaultIcon(forName: names[i], path: normPath)
                    migrated.append(FavoriteItem(
                        id: UUID(),
                        name: names[i],
                        path: normPath,
                        systemImage: icon
                    ))
                }

                // 기존 기본 4개만 있던 사용자에게 Argo Downloads도 자연스럽게 보강
                let argoPath = "/private/var/mobile/Documents/Argo/"
                if !migrated.contains(where: { $0.path == argoPath }) {
                    let insertIndex = migrated.isEmpty ? 0 : 1
                    migrated.insert(FavoriteItem(
                        id: UUID(),
                        name: "Argo Downloads",
                        path: argoPath,
                        systemImage: "arrow.down.circle"
                    ), at: insertIndex)
                }

                items = migrated
                defaults.set(true, forKey: Self.migratedKey)
                defaults.set(true, forKey: Self.seededDefaultsKey)
                persist()
                return
            }
        }

        // 3. 최초 실행 시 기본 항목 1회 심기 (이후 사용자가 모두 지워도 다시 생기지 않음)
        if !defaults.bool(forKey: Self.seededDefaultsKey) {
            items = Self.defaultItems()
            defaults.set(true, forKey: Self.migratedKey)
            defaults.set(true, forKey: Self.seededDefaultsKey)
            persist()
        } else {
            items = []
        }
    }

    private func save() {
        // Skipped while loading so the didSet on `items` does not write back
        // what we just read. Deliberate writes during load call persist().
        guard !isLoading else { return }
        persist()
    }

    /// Writes unconditionally. loadOrMigrate() uses this because `isLoading`
    /// is still true at that point (the defer only clears it on function exit).
    private func persist() {
        guard let data = try? JSONEncoder().encode(items) else { return }
        UserDefaults.favorites.set(data, forKey: Self.itemsKey)
        UserDefaults.favorites.synchronize()
    }

    private static func defaultItems() -> [FavoriteItem] {
        return [
            FavoriteItem(id: UUID(), name: "Documents", path: "/private/var/mobile/Documents/", systemImage: "doc.text"),
            FavoriteItem(id: UUID(), name: "Argo Downloads", path: "/private/var/mobile/Documents/Argo/", systemImage: "arrow.down.circle"),
            FavoriteItem(id: UUID(), name: "Applications", path: "/Applications/", systemImage: "square.grid.2x2"),
            FavoriteItem(id: UUID(), name: "User Apps", path: "/private/var/containers/Bundle/Application/", systemImage: "person.crop.circle"),
            FavoriteItem(id: UUID(), name: "Trash", path: "/private/var/mobile/Media/.Trash/", systemImage: "trash")
        ]
    }

    private static func defaultIcon(forName name: String, path: String) -> String {
        switch path {
        case "/private/var/mobile/Documents/":
            return "doc.text"
        case "/private/var/mobile/Documents/Argo/":
            return "arrow.down.circle"
        case "/Applications/":
            return "square.grid.2x2"
        case "/private/var/containers/Bundle/Application/":
            return "person.crop.circle"
        case "/private/var/mobile/Media/.Trash/":
            return "trash"
        default:
            break
        }

        switch name {
        case "Documents":
            return "doc.text"
        case "Argo Downloads":
            return "arrow.down.circle"
        case "Applications":
            return "square.grid.2x2"
        case "UserApplications", "User Apps":
            return "person.crop.circle"
        case "Trash":
            return "trash"
        default:
            return "folder"
        }
    }

    // MARK: - Public API

    func add(name: String, path: String, systemImage: String) {
        let normalizedPath = Self.normalizePath(path)
        let trimmedName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        let fallbackName = URL(fileURLWithPath: normalizedPath).lastPathComponent
        let finalName = trimmedName.isEmpty ? (fallbackName.isEmpty ? normalizedPath : fallbackName) : trimmedName
        let finalIcon = systemImage.isEmpty ? "folder" : systemImage

        let newItem = FavoriteItem(
            id: UUID(),
            name: finalName,
            path: normalizedPath,
            systemImage: finalIcon
        )
        items.append(newItem)
    }

    func contains(path: String) -> Bool {
        let normalizedPath = Self.normalizePath(path)
        return items.contains { $0.path == normalizedPath }
    }

    func remove(_ item: FavoriteItem) {
        items.removeAll { $0.id == item.id }
    }

    func rename(_ item: FavoriteItem, to newName: String) {
        let trimmed = newName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty,
              let index = items.firstIndex(where: { $0.id == item.id }) else {
            return
        }
        items[index].name = trimmed
    }

    func moveUp(_ item: FavoriteItem) {
        guard let index = items.firstIndex(where: { $0.id == item.id }),
              index > 0 else {
            return
        }
        items.swapAt(index, index - 1)
    }

    func moveDown(_ item: FavoriteItem) {
        guard let index = items.firstIndex(where: { $0.id == item.id }),
              index < items.count - 1 else {
            return
        }
        items.swapAt(index, index + 1)
    }

    func resetToDefaults() {
        let defaults = UserDefaults.favorites
        defaults.set(true, forKey: Self.migratedKey)
        defaults.set(true, forKey: Self.seededDefaultsKey)
        items = Self.defaultItems()
    }
}
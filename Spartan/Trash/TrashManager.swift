//
//  TrashManager.swift
//  Argo
//

import Foundation
import Combine

struct TrashEntry: Codable, Identifiable, Hashable {
    let id: String              // UUID 문자열
    let originalPath: String    // 원본 전체 경로
    let originalName: String
    let deletedAt: Date
    let isDirectory: Bool
    let size: Int64

    var storedPath: String {
        if originalPath.isEmpty {
            // 레거시 항목: .Trash/ 바로 아래에 존재하는 파일 또는 폴더
            return "\(TrashManager.trashRootPath)/\(originalName)"
        }
        return "\(TrashManager.itemsRootPath)/\(id)/\(originalName)"
    }

    var isLegacy: Bool {
        return originalPath.isEmpty
    }

    var deletedAtDescription: String {
        let formatter = DateFormatter()
        let format = UserDefaults.settings.string(forKey: "dateFormat") ?? "MM-dd-yyyy HH:mm"
        formatter.dateFormat = format.isEmpty ? "MM-dd-yyyy HH:mm" : format
        return formatter.string(from: deletedAt)
    }
}

enum TrashRestoreResult {
    case restored(String)          // 복원된 최종 경로
    case renamed(String)           // 이름 충돌로 바꿔서 복원한 경로
    case failed(String)            // 사유
}

private struct TrashIndexFile: Codable {
    var version: Int = 1
    var entries: [TrashEntry]
}

final class TrashManager: ObservableObject {
    static let shared = TrashManager()

    static let trashRootPath = "/private/var/mobile/Media/.Trash"
    static let indexFilePath = "/private/var/mobile/Media/.Trash/argo-trash-index.json"
    static let itemsRootPath = "/private/var/mobile/Media/.Trash/items"

    @Published private(set) var entries: [TrashEntry] = []

    var totalSize: Int64 {
        return entries.reduce(0) { $0 + $1.size }
    }

    private init() {
        ensureDirectories()
        reload()
    }

    private func ensureDirectories() {
        if !fileManager.fileExists(atPath: Self.trashRootPath) {
            RootHelperActs.mkdir(Self.trashRootPath)
        }
        if !fileManager.fileExists(atPath: Self.itemsRootPath) {
            RootHelperActs.mkdir(Self.itemsRootPath)
        }
    }

    func reload() {
        ensureDirectories()
        let indexed = loadIndexedEntries()
        let legacy = scanLegacyEntries()
        let combined = indexed + legacy
        entries = combined.sorted { $0.deletedAt > $1.deletedAt }
    }

    private func loadIndexedEntries() -> [TrashEntry] {
        guard fileManager.fileExists(atPath: Self.indexFilePath),
              let data = try? Data(contentsOf: URL(fileURLWithPath: Self.indexFilePath)) else {
            return []
        }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .secondsSince1970
        guard let indexFile = try? decoder.decode(TrashIndexFile.self, from: data) else {
            return []
        }
        // 실제 디스크에 남아 있는 항목만 유지
        return indexFile.entries.filter { fileManager.fileExists(atPath: $0.storedPath) }
    }

    private func saveIndexedEntries(_ list: [TrashEntry]) {
        let nonLegacy = list.filter { !$0.isLegacy }
        let indexFile = TrashIndexFile(version: 1, entries: nonLegacy)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted]
        encoder.dateEncodingStrategy = .secondsSince1970
        guard let data = try? encoder.encode(indexFile) else { return }
        do {
            try data.write(to: URL(fileURLWithPath: tempPath))
            RootHelperActs.mvtemp(Self.indexFilePath)
        } catch {
            print("TrashManager: Failed to write index file: \(error)")
        }
    }

    private func scanLegacyEntries() -> [TrashEntry] {
        guard let names = try? fileManager.contentsOfDirectory(atPath: Self.trashRootPath) else {
            return []
        }
        var result: [TrashEntry] = []
        for name in names {
            if name == "argo-trash-index.json" || name == "items" || name == "." || name == ".." {
                continue
            }
            let fullPath = "\(Self.trashRootPath)/\(name)"
            var isDir: ObjCBool = false
            guard fileManager.fileExists(atPath: fullPath, isDirectory: &isDir) else { continue }
            let attrs = try? fileManager.attributesOfItem(atPath: fullPath)
            let modDate = (attrs?[.modificationDate] as? Date) ?? Date(timeIntervalSince1970: 0)
            let itemSize = computeSize(atPath: fullPath, isDirectory: isDir.boolValue)
            let entry = TrashEntry(
                id: "legacy-\(name)",
                originalPath: "",
                originalName: name,
                deletedAt: modDate,
                isDirectory: isDir.boolValue,
                size: itemSize
            )
            result.append(entry)
        }
        return result
    }

    @discardableResult
    func moveToTrash(path: String) -> Bool {
        let normalized = normalizePath(path)
        guard !normalized.isEmpty else { return false }

        // 휴지통 자체나 휴지통 내부 항목을 다시 버리려 하면 거부
        if normalized == Self.trashRootPath || normalized.hasPrefix(Self.trashRootPath + "/") {
            return false
        }

        var isDir: ObjCBool = false
        guard fileManager.fileExists(atPath: normalized, isDirectory: &isDir) else {
            return false
        }

        ensureDirectories()

        let uuid = UUID().uuidString
        let originalName = (normalized as NSString).lastPathComponent
        guard !originalName.isEmpty, originalName != "/" else { return false }

        let itemSize = computeSize(atPath: normalized, isDirectory: isDir.boolValue)
        let uuidDir = "\(Self.itemsRootPath)/\(uuid)"
        let destPath = "\(uuidDir)/\(originalName)"

        RootHelperActs.mkdir(uuidDir)
        RootHelperActs.mv(normalized, destPath)

        // 이동 성공 여부 확인 (실패 시 유령 항목 방지)
        guard fileManager.fileExists(atPath: destPath) else {
            RootHelperActs.rm(uuidDir)
            return false
        }

        var currentIndexed = loadIndexedEntries()
        let newEntry = TrashEntry(
            id: uuid,
            originalPath: normalized,
            originalName: originalName,
            deletedAt: Date(),
            isDirectory: isDir.boolValue,
            size: itemSize
        )
        currentIndexed.append(newEntry)
        saveIndexedEntries(currentIndexed)
        reload()
        return true
    }

    func moveToTrash(paths: [String]) -> (moved: Int, failed: Int) {
        var movedCount = 0
        var failedCount = 0
        for p in paths {
            if moveToTrash(path: p) {
                movedCount += 1
            } else {
                failedCount += 1
            }
        }
        return (movedCount, failedCount)
    }

    func restore(_ entry: TrashEntry) -> TrashRestoreResult {
        if entry.isLegacy || entry.originalPath.isEmpty {
            return .failed("Unknown original location.")
        }

        let sourcePath = entry.storedPath
        guard fileManager.fileExists(atPath: sourcePath) else {
            var currentIndexed = loadIndexedEntries()
            currentIndexed.removeAll { $0.id == entry.id }
            saveIndexedEntries(currentIndexed)
            reload()
            return .failed("Item no longer exists in Trash.")
        }

        let targetOriginal = normalizePath(entry.originalPath)
        let parentDir = (targetOriginal as NSString).deletingLastPathComponent
        ensureDirectoryRecursive(parentDir)

        var finalPath = targetOriginal
        var didRename = false

        if fileManager.fileExists(atPath: finalPath) {
            finalPath = makeNonConflictingPath(for: targetOriginal, isDirectory: entry.isDirectory)
            didRename = true
        }

        RootHelperActs.mv(sourcePath, finalPath)

        guard fileManager.fileExists(atPath: finalPath) else {
            reload()
            return .failed("Failed to move item to \(finalPath)")
        }

        let uuidDir = "\(Self.itemsRootPath)/\(entry.id)"
        RootHelperActs.rm(uuidDir)

        var currentIndexed = loadIndexedEntries()
        currentIndexed.removeAll { $0.id == entry.id }
        saveIndexedEntries(currentIndexed)
        reload()

        if didRename {
            return .renamed(finalPath)
        } else {
            return .restored(finalPath)
        }
    }

    func deleteForever(_ entry: TrashEntry) -> Bool {
        if entry.isLegacy {
            let target = entry.storedPath
            RootHelperActs.rm(target)
            let ok = !fileManager.fileExists(atPath: target)
            reload()
            return ok
        } else {
            let uuidDir = "\(Self.itemsRootPath)/\(entry.id)"
            RootHelperActs.rm(uuidDir)
            let ok = !fileManager.fileExists(atPath: uuidDir)
            var currentIndexed = loadIndexedEntries()
            currentIndexed.removeAll { $0.id == entry.id }
            saveIndexedEntries(currentIndexed)
            reload()
            return ok
        }
    }

    func emptyAll() -> Bool {
        RootHelperActs.rm(Self.trashRootPath)
        RootHelperActs.mkdir(Self.trashRootPath)
        RootHelperActs.mkdir(Self.itemsRootPath)
        saveIndexedEntries([])
        reload()
        return entries.isEmpty
    }

    // MARK: - Helpers

    private func normalizePath(_ path: String) -> String {
        var result = path.trimmingCharacters(in: .whitespacesAndNewlines)
        while result.count > 1 && result.hasSuffix("/") {
            result.removeLast()
        }
        return result
    }

    private func ensureDirectoryRecursive(_ dirPath: String) {
        let normalized = normalizePath(dirPath)
        guard !normalized.isEmpty, normalized != "/" else { return }
        if fileManager.fileExists(atPath: normalized) { return }

        let components = normalized.split(separator: "/").map(String.init)
        var current = ""
        for comp in components {
            current += "/" + comp
            if !fileManager.fileExists(atPath: current) {
                RootHelperActs.mkdir(current)
            }
        }
    }

    private func makeNonConflictingPath(for originalPath: String, isDirectory: Bool) -> String {
        let nsPath = originalPath as NSString
        let parent = nsPath.deletingLastPathComponent
        let fileName = nsPath.lastPathComponent

        let baseName: String
        let ext: String

        if isDirectory {
            baseName = fileName
            ext = ""
        } else {
            let nsFile = fileName as NSString
            ext = nsFile.pathExtension
            baseName = ext.isEmpty ? fileName : nsFile.deletingPathExtension
        }

        var counter = 1
        while true {
            let candidateName: String
            if ext.isEmpty {
                candidateName = "\(baseName) (\(counter))"
            } else {
                candidateName = "\(baseName) (\(counter)).\(ext)"
            }
            let candidatePath = (parent as NSString).appendingPathComponent(candidateName)
            if !fileManager.fileExists(atPath: candidatePath) {
                return candidatePath
            }
            counter += 1
        }
    }

    private func computeSize(atPath path: String, isDirectory: Bool) -> Int64 {
        if !isDirectory {
            let attrs = try? fileManager.attributesOfItem(atPath: path)
            if let num = attrs?[.size] as? NSNumber {
                return num.int64Value
            }
            if let s = attrs?[.size] as? Int64 {
                return s
            }
            return 0
        }

        var total: Int64 = 0
        if let enumerator = fileManager.enumerator(atPath: path) {
            for case let subPath as String in enumerator {
                let fullSubPath = (path as NSString).appendingPathComponent(subPath)
                let attrs = try? fileManager.attributesOfItem(atPath: fullSubPath)
                if let type = attrs?[.type] as? FileAttributeType, type == .typeRegular {
                    if let num = attrs?[.size] as? NSNumber {
                        total += num.int64Value
                    } else if let s = attrs?[.size] as? Int64 {
                        total += s
                    }
                }
            }
        }
        return total
    }
}
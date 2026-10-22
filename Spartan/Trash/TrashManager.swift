//
//  TrashManager.swift
//  Argo
//

import Foundation
import Combine

struct TrashEntry: Codable, Identifiable, Hashable {
    let id: String
    let originalPath: String
    let originalName: String
    let deletedAt: Date
    let isDirectory: Bool
    let size: Int64

    var storedPath: String {
        if originalPath.isEmpty {
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
    case restored(String)
    case renamed(String)
    case failed(String)
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

    // MARK: - Symlink Inspection Helper (S6)

    private func isSymbolicLink(atPath path: String) -> Bool {
        guard let attrs = try? fileManager.attributesOfItem(atPath: path),
              let type = attrs[.type] as? FileAttributeType else {
            return false
        }
        return type == .typeSymbolicLink
    }

    /// 디렉터리 삭제 전 내부의 심볼릭 링크만 먼저 끊어내어 재귀 삭제가 링크 대상을 따라가는 것을 원천 차단

    // MARK: - Dual File Operations (FileGuard + FileManager First + RootHelperActs Fallback)

    private func robustMkdir(_ path: String) {
        let norm = normalizePath(path)
        guard !norm.isEmpty else { return }
        if fileManager.fileExists(atPath: norm) { return }
        guard case .success = FileGuard.authorize(action: "mkdir", paths: [norm]) else { return }

        do {
            try fileManager.createDirectory(atPath: norm, withIntermediateDirectories: true, attributes: nil)
        } catch {
            RootHelperActs.mkdir(norm)
        }
    }

    private func robustRemove(_ path: String) {
        let norm = normalizePath(path)
        guard !norm.isEmpty else { return }
        guard fileManager.fileExists(atPath: norm) || isSymbolicLink(atPath: norm) else { return }
        guard case .success = FileGuard.authorize(action: "rm", paths: [norm]) else { return }

        // 폴더 내부 심볼릭 링크를 미리 끊는 방어가 여기 있었다. 지웠다.
        // removeItem 은 아래로 훑으며 링크를 '끊고' 따라가지 않는다. 막을 위험이
        // 없었고, 대신 아래 removeItem 이 실패하면 폴더는 남은 채 안의 링크만
        // 영구히 사라졌다 — robustMove 가 목적지를 비우려 이 함수를 부르므로,
        // 옮기기가 실패하면 건드릴 생각도 없던 폴더의 링크만 날아갔다.
        do {
            try fileManager.removeItem(atPath: norm)
        } catch {
            RootHelperActs.rm(norm)
        }
        if fileManager.fileExists(atPath: norm) {
            RootHelperActs.rm(norm)
        }
    }

    private func robustMove(from src: String, to dst: String) -> Bool {
        let normSrc = normalizePath(src)
        let normDst = normalizePath(dst)
        guard !normSrc.isEmpty, !normDst.isEmpty else { return false }
        guard fileManager.fileExists(atPath: normSrc) || isSymbolicLink(atPath: normSrc) else { return false }
        guard case .success = FileGuard.authorize(action: "mv", paths: [normSrc, normDst]) else {
            return false
        }

        if fileManager.fileExists(atPath: normDst) || isSymbolicLink(atPath: normDst) {
            robustRemove(normDst)
        }

        // 1차 시도: FileManager 직접 이동
        do {
            try fileManager.moveItem(atPath: normSrc, toPath: normDst)
            if fileManager.fileExists(atPath: normDst) || isSymbolicLink(atPath: normDst) {
                return true
            }
        } catch {
            // 2차 시도: RootHelperActs.mv
            RootHelperActs.mv(normSrc, normDst)
            if fileManager.fileExists(atPath: normDst) || isSymbolicLink(atPath: normDst) {
                return true
            }
        }

        // 3차 시도: 복사 후 원본 삭제 (단, 심볼릭 링크가 아닐 때만)
        if !isSymbolicLink(atPath: normSrc) {
            do {
                try fileManager.copyItem(atPath: normSrc, toPath: normDst)
                if fileManager.fileExists(atPath: normDst) {
                    robustRemove(normSrc)
                    return !fileManager.fileExists(atPath: normSrc)
                }
            } catch {
                RootHelperActs.cp(normSrc, normDst)
                if fileManager.fileExists(atPath: normDst) {
                    robustRemove(normSrc)
                    return !fileManager.fileExists(atPath: normSrc)
                }
            }
        }

        return fileManager.fileExists(atPath: normDst)
    }

    private func ensureDirectories() {
        robustMkdir(Self.trashRootPath)
        robustMkdir(Self.itemsRootPath)
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
        return indexFile.entries.filter { fileManager.fileExists(atPath: $0.storedPath) || isSymbolicLink(atPath: $0.storedPath) }
    }

    private func saveIndexedEntries(_ list: [TrashEntry]) {
        let nonLegacy = list.filter { !$0.isLegacy }
        let indexFile = TrashIndexFile(version: 1, entries: nonLegacy)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted]
        encoder.dateEncodingStrategy = .secondsSince1970
        guard let data = try? encoder.encode(indexFile) else { return }

        do {
            try data.write(to: URL(fileURLWithPath: Self.indexFilePath))
            return
        } catch {
            let sandboxTemp = (NSTemporaryDirectory() as NSString).appendingPathComponent("argo-trash-index.tmp")
            defer { try? fileManager.removeItem(atPath: sandboxTemp) }
            if let _ = try? data.write(to: URL(fileURLWithPath: sandboxTemp)) {
                robustRemove(Self.indexFilePath)
                _ = robustMove(from: sandboxTemp, to: Self.indexFilePath)
                if fileManager.fileExists(atPath: Self.indexFilePath) {
                    return
                }
            }
            if let _ = try? data.write(to: URL(fileURLWithPath: tempPath)) {
                robustRemove(Self.indexFilePath)
                RootHelperActs.mvtemp(Self.indexFilePath)
            }
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
            let isSym = isSymbolicLink(atPath: fullPath)
            let effectiveIsDir = isSym ? false : isDir.boolValue
            let attrs = try? fileManager.attributesOfItem(atPath: fullPath)
            let modDate = (attrs?[.modificationDate] as? Date) ?? Date(timeIntervalSince1970: 0)
            let itemSize = computeSize(atPath: fullPath, isDirectory: effectiveIsDir)
            let entry = TrashEntry(
                id: "legacy-\(name)",
                originalPath: "",
                originalName: name,
                deletedAt: modDate,
                isDirectory: effectiveIsDir,
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

        if normalized == Self.trashRootPath || normalized.hasPrefix(Self.trashRootPath + "/") {
            return false
        }

        // S5: 휴지통 이동 전 FileGuard 승인 검사 (금지 구역 및 안전 모드 차단 + UI 알림)
        guard case .success = FileGuard.authorize(action: "trash", paths: [normalized]) else {
            return false
        }

        let isSym = isSymbolicLink(atPath: normalized)
        var isDir: ObjCBool = false
        guard isSym || fileManager.fileExists(atPath: normalized, isDirectory: &isDir) else {
            return false
        }
        let effectiveIsDir = isSym ? false : isDir.boolValue

        ensureDirectories()

        let uuid = UUID().uuidString
        let originalName = (normalized as NSString).lastPathComponent
        guard !originalName.isEmpty, originalName != "/" else { return false }

        let itemSize = computeSize(atPath: normalized, isDirectory: effectiveIsDir)
        let uuidDir = "\(Self.itemsRootPath)/\(uuid)"
        let destPath = "\(uuidDir)/\(originalName)"

        robustMkdir(uuidDir)

        guard robustMove(from: normalized, to: destPath) else {
            robustRemove(uuidDir)
            return false
        }

        var currentIndexed = loadIndexedEntries()
        let newEntry = TrashEntry(
            id: uuid,
            originalPath: normalized,
            originalName: originalName,
            deletedAt: Date(),
            isDirectory: effectiveIsDir,
            size: itemSize
        )
        currentIndexed.append(newEntry)
        saveIndexedEntries(currentIndexed)
        OperationLog.record(action: "TRASH", path: normalized, destination: destPath)
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
            return .failed("원래 위치를 알 수 없습니다.")
        }

        let sourcePath = entry.storedPath
        guard fileManager.fileExists(atPath: sourcePath) || isSymbolicLink(atPath: sourcePath) else {
            var currentIndexed = loadIndexedEntries()
            currentIndexed.removeAll { $0.id == entry.id }
            saveIndexedEntries(currentIndexed)
            reload()
            return .failed("휴지통에 해당 항목이 없습니다.")
        }

        let targetOriginal = normalizePath(entry.originalPath)

        switch FileGuard.authorize(action: "restore", paths: [targetOriginal]) {
        case .failure(let err):
            return .failed(err.localizedDescription)
        case .success:
            break
        }

        let parentDir = (targetOriginal as NSString).deletingLastPathComponent
        ensureDirectoryRecursive(parentDir)

        var finalPath = targetOriginal
        var didRename = false

        if fileManager.fileExists(atPath: finalPath) || isSymbolicLink(atPath: finalPath) {
            finalPath = makeNonConflictingPath(for: targetOriginal, isDirectory: entry.isDirectory)
            didRename = true
        }

        guard robustMove(from: sourcePath, to: finalPath) else {
            return .failed("복원 이동 실패: \(finalPath)")
        }

        let uuidDir = "\(Self.itemsRootPath)/\(entry.id)"
        robustRemove(uuidDir)

        var currentIndexed = loadIndexedEntries()
        currentIndexed.removeAll { $0.id == entry.id }
        saveIndexedEntries(currentIndexed)
        OperationLog.record(action: "RESTORE", path: sourcePath, destination: finalPath)
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
            robustRemove(target)
            let ok = !fileManager.fileExists(atPath: target) && !isSymbolicLink(atPath: target)
            if ok {
                OperationLog.record(action: "DELETE_FOREVER", path: target)
            }
            reload()
            return ok
        } else {
            let uuidDir = "\(Self.itemsRootPath)/\(entry.id)"
            robustRemove(uuidDir)
            let ok = !fileManager.fileExists(atPath: uuidDir)
            var currentIndexed = loadIndexedEntries()
            currentIndexed.removeAll { $0.id == entry.id }
            saveIndexedEntries(currentIndexed)
            if ok {
                OperationLog.record(action: "DELETE_FOREVER", path: entry.originalPath)
            }
            reload()
            return ok
        }
    }

    @discardableResult
    func deletePathDirectly(_ path: String) -> Bool {
        let norm = normalizePath(path)
        guard !norm.isEmpty else { return false }
        guard case .success = FileGuard.authorize(action: "delete", paths: [norm]) else {
            return false
        }
        robustRemove(norm)
        let ok = !fileManager.fileExists(atPath: norm) && !isSymbolicLink(atPath: norm)
        if ok {
            OperationLog.record(action: "DELETE", path: norm)
        }
        return ok
    }

    func emptyAll() -> Bool {
        robustRemove(Self.trashRootPath)
        robustMkdir(Self.trashRootPath)
        robustMkdir(Self.itemsRootPath)
        saveIndexedEntries([])
        OperationLog.record(action: "EMPTY_TRASH", path: Self.trashRootPath)
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
                robustMkdir(current)
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
            if !fileManager.fileExists(atPath: candidatePath) && !isSymbolicLink(atPath: candidatePath) {
                return candidatePath
            }
            counter += 1
        }
    }

    private func computeSize(atPath path: String, isDirectory: Bool) -> Int64 {
        if isSymbolicLink(atPath: path) {
            return 0
        }
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
        let rootURL = URL(fileURLWithPath: path)
        // S6: 용량 계산 시에도 심볼릭 링크를 따라가지 않도록 처리
        if let enumerator = fileManager.enumerator(
            at: rootURL,
            includingPropertiesForKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey],
            options: [.skipsPackageDescendants]
        ) {
            for case let fileURL as URL in enumerator {
                if let vals = try? fileURL.resourceValues(forKeys: [.isSymbolicLinkKey, .isRegularFileKey, .fileSizeKey]) {
                    if vals.isSymbolicLink == true {
                        enumerator.skipDescendants()
                        continue
                    }
                    if vals.isRegularFile == true, let s = vals.fileSize {
                        total += Int64(s)
                    }
                }
            }
        }
        return total
    }
}
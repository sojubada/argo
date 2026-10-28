//
//  PaneTransfer.swift
//  Argo
//
//  칸 사이 복사·이동의 진행률과 되돌리기.
//
//  큰 파일을 복사하는 동안 화면이 멈추면 안 된다. 그래서 작업은 뒷줄에서 돌고,
//  진행률만 메인으로 올린다.
//
//  쓰기는 전부 FileGuard를 지난다. 관문을 우회하면 안전장치가 무의미해진다.
//

import Foundation
import SwiftUI

/// 되돌릴 수 있는 한 건. 이동과 이름 바꾸기만 되돌릴 수 있다.
/// 복사는 되돌릴 것이 '지우기'인데, 지우기를 자동으로 하면 위험해서 넣지 않는다.
///
/// 한 번에 옮긴 여러 개는 한 건으로 묶는다. 다섯 개를 옮기고 ⌘Z 를 누르면
/// 다섯 개가 함께 돌아와야 한다 — 전에는 마지막 하나만 돌아왔다.
struct UndoableAction {
    enum Kind {
        case move
        case rename
    }

    struct Item {
        /// 되돌리려면 to 에서 from 으로 다시 옮기면 된다.
        let from: String
        let to: String
    }

    let kind: Kind
    var items: [Item]

    init(kind: Kind, items: [Item]) {
        self.kind = kind
        self.items = items
    }

    init(kind: Kind, from: String, to: String) {
        self.init(kind: kind, items: [Item(from: from, to: to)])
    }

    var describe: String {
        let name = items.count == 1
            ? "'" + (items[0].to as NSString).lastPathComponent + "'"
            : LF("{0}개", items.count)
        switch kind {
        case .move:   return LF("{0} 이동을 되돌립니다", name)
        case .rename: return LF("{0} 이름 변경을 되돌립니다", name)
        }
    }
}

@available(tvOS 13.0, *)
final class PaneTransfer: ObservableObject {

    @Published var isRunning = false
    /// 0.0 ~ 1.0.
    ///
    /// 복사는 바이트 기준이다. 전에는 항목 수 기준이라 큰 파일 하나를
    /// 복사하면 끝날 때까지 0 에 멈춰 있었다. 그래서 복사는 FileManager 에
    /// 통째로 맡기지 않고 직접 조각내어 쓴다 — 조각마다 진행률을 올리고
    /// 취소를 본다.
    ///
    /// 이동은 같은 볼륨 안에서는 이름만 바뀌어 순식간이라 항목 수 기준이다.
    @Published var progress: Double = 0
    @Published var currentName: String = ""
    @Published var completed = 0
    @Published var total = 0
    /// 복사일 때만 채운다. 0 이면 화면에 바이트를 보여 주지 않는다.
    @Published var bytesDone: Int64 = 0
    @Published var bytesTotal: Int64 = 0
    /// 취소를 누른 뒤 지금 조각이 끝나기를 기다리는 중.
    @Published var isCancelling = false

    /// 되돌리기 기록. 마지막 하나만 쓰지만 몇 개를 쌓아 둔다.
    @Published private(set) var undoStack: [UndoableAction] = []

    /// 뒷줄과 메인이 같이 본다. 잠금 없이 Bool 하나를 읽고 쓰면
    /// 대개는 되지만 보장이 없다.
    private let cancelLock = NSLock()
    private var cancelFlag = false
    private var cancelRequested: Bool {
        get { cancelLock.lock(); defer { cancelLock.unlock() }; return cancelFlag }
        set { cancelLock.lock(); cancelFlag = newValue; cancelLock.unlock() }
    }

    /// 조각 크기. 크면 취소가 늦게 먹고, 작으면 느려진다.
    private static let chunk = 4 * 1024 * 1024

    var canUndo: Bool { !undoStack.isEmpty }
    var nextUndo: UndoableAction? { undoStack.last }

    func cancel() {
        cancelRequested = true
        isCancelling = true
    }

    /// 여러 항목을 목적지로 복사하거나 옮긴다.
    /// 끝나면 onFinish 가 메인에서 불린다 — 양쪽 칸을 다시 읽는 데 쓴다.
    /// 세 번째 값은 취소로 멈췄는지다.
    func run(sources: [String],
             destination: String,
             move: Bool,
             onBlocked: @escaping (String) -> Void,
             onFinish: @escaping (Int, [String], Bool) -> Void) {

        guard !isRunning else {
            onBlocked(L("앞의 작업이 아직 끝나지 않았습니다."))
            return
        }

        let targets = sources.map { destination + (($0 as NSString).lastPathComponent) }

        // 폴더를 그 자신의 안으로 보내지 않는다.
        //
        // 두 칸을 쓰면 흔히 생긴다 — 왼쪽에서 A 를 고르고 오른쪽이 A 안의 하위
        // 폴더에 있을 때. 복사는 자기 자신을 끝없이 안으로 퍼 담으려 들고,
        // 이동은 제 발밑으로 들어가려다 실패한다.
        func bare(_ p: String) -> String {
            var t = (p as NSString).standardizingPath
            if t.hasPrefix("/var/") { t = "/private" + t }
            return t
        }
        let dest = bare(destination)
        if let inside = sources.first(where: { dest == bare($0) || dest.hasPrefix(bare($0) + "/") }) {
            onBlocked(LF("‘{0}’ 을(를) 그 자신의 안으로 ", (inside as NSString).lastPathComponent)
                      + (move ? L("옮길") : L("복사할")) + L(" 수 없습니다."))
            return
        }

        // 관문을 먼저 지난다. 한 건이라도 막히면 전부 멈춘다 —
        // 절반만 옮겨진 상태가 가장 수습하기 어렵다.
        //
        // 옮기기는 원본 자리에서 보면 지우기다. 목적지만 물으면 보호된 폴더에서
        // 파일을 빼내는 것이 그대로 통과하므로, 원본도 같이 묻는다.
        let asked = move ? targets + sources : targets
        switch FileGuard.authorize(action: move ? "move" : "copy", paths: asked) {
        case .failure(let err):
            onBlocked(err.localizedDescription)
            return
        case .success:
            break
        }

        cancelRequested = false
        isCancelling = false
        isRunning = true
        progress = 0
        completed = 0
        total = sources.count
        bytesDone = 0
        bytesTotal = 0

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self = self else { return }
            var failed: [String] = []
            var moved: [UndoableAction.Item] = []
            var done = 0

            // 복사할 총량을 먼저 센다. 큰 폴더면 잠깐 걸리지만, 이것이 없으면
            // 진행률이 무엇의 몇 분의 몇인지 알 수 없다.
            var totalBytes: Int64 = 0
            if !move {
                DispatchQueue.main.async { self.currentName = L("크기를 세는 중…") }
                for source in sources {
                    if self.cancelRequested { break }
                    totalBytes += PaneTransfer.treeSize(source)
                }
                let t = totalBytes
                DispatchQueue.main.async { self.bytesTotal = t }
            }

            var copied: Int64 = 0
            var lastPush = Date.distantPast
            let push: (Int64) -> Void = { delta in
                copied += delta
                let now = Date()
                guard now.timeIntervalSince(lastPush) > 0.1 else { return }
                lastPush = now
                let c = copied
                DispatchQueue.main.async {
                    self.bytesDone = c
                    if totalBytes > 0 {
                        self.progress = min(1, Double(c) / Double(totalBytes))
                    }
                }
            }

            for (i, source) in sources.enumerated() {
                if self.cancelRequested { break }

                let name = (source as NSString).lastPathComponent
                DispatchQueue.main.async { self.currentName = name }

                // 덮어쓰기는 하지 않는다. 이름을 비켜 쓴다 — 조용한 자료 손실을 막는다.
                var target = destination + name
                if fileManager.fileExists(atPath: target) || PaneTransfer.isLink(target) {
                    target = PaneTransfer.nonClashingPath(target)
                }

                let ok: Bool
                if move {
                    ok = self.moveOne(source: source, target: target)
                } else {
                    ok = self.copyOne(source: source, target: target, onBytes: push)
                }

                if ok {
                    done += 1
                    // 실제로 놓인 자리를 적는다. 이름을 비켜 썼으면 그 이름이다 —
                    // 원래 이름을 적어 두면 되돌리기가 엉뚱한 파일을 찾는다.
                    if move { moved.append(.init(from: PaneTransfer.trimSlash(source), to: target)) }
                } else if !self.cancelRequested {
                    failed.append(name)
                }

                DispatchQueue.main.async {
                    self.completed = i + 1
                    if move || totalBytes == 0 {
                        self.progress = Double(i + 1) / Double(max(1, sources.count))
                    }
                }
            }

            let wasCancelled = self.cancelRequested
            DispatchQueue.main.async {
                if !moved.isEmpty {
                    self.undoStack.append(UndoableAction(kind: .move, items: moved))
                }
                self.isRunning = false
                self.isCancelling = false
                self.currentName = ""
                onFinish(done, failed, wasCancelled)
            }
        }
    }

    // MARK: - 한 건

    private func moveOne(source: String, target: String) -> Bool {
        do {
            try fileManager.moveItem(atPath: source, toPath: target)
            OperationLog.record(action: "MOVE (pane)", path: source, destination: target)
            return true
        } catch {
            print("[PaneTransfer] \(source) -> \(target) 이동 실패: \(error.localizedDescription)")
            RootHelperActs.mv(source, target)
            return fileManager.fileExists(atPath: target) || PaneTransfer.isLink(target)
        }
    }

    /// 조각내어 복사한다. 도중에 취소하면 쓰다 만 것을 지운다 —
    /// 반쯤 쓰인 파일이 온전한 파일처럼 남는 것이 가장 나쁘다.
    ///
    /// 조각 복사가 취소 말고 다른 이유로 실패하면, 쓰다 만 것을 지우고
    /// FileManager 로 한 번 더 해 본다. 그래도 안 되면 헬퍼로 한다.
    private func copyOne(source: String, target: String, onBytes: (Int64) -> Void) -> Bool {
        do {
            try copyTree(source: source, target: target, onBytes: onBytes)
            OperationLog.record(action: "COPY (pane)", path: source, destination: target)
            return true
        } catch {
            PaneTransfer.removePartial(target)
            if cancelRequested { return false }
            print("[PaneTransfer] 조각 복사 실패, 통째 복사로: \(source) — \(error.localizedDescription)")
            do {
                try fileManager.copyItem(atPath: source, toPath: target)
                OperationLog.record(action: "COPY (pane)", path: source, destination: target)
                return true
            } catch {
                RootHelperActs.cp(source, target)
                return fileManager.fileExists(atPath: target)
            }
        }
    }

    private struct Cancelled: Error {}

    private func copyTree(source: String, target: String, onBytes: (Int64) -> Void) throws {
        if cancelRequested { throw Cancelled() }

        let attrs = try fileManager.attributesOfItem(atPath: source)
        let type = (attrs[.type] as? FileAttributeType) ?? .typeUnknown

        switch type {
        case .typeSymbolicLink:
            // 링크는 링크로 옮겨 적는다. 따라가 복사하면 가리키던 것이
            // 통째로 딸려 오고, 링크가 돌면 끝나지 않는다.
            let dest = try fileManager.destinationOfSymbolicLink(atPath: source)
            try fileManager.createSymbolicLink(atPath: target, withDestinationPath: dest)

        case .typeDirectory:
            try fileManager.createDirectory(atPath: target, withIntermediateDirectories: false)
            for child in try fileManager.contentsOfDirectory(atPath: source) {
                try copyTree(source: (source as NSString).appendingPathComponent(child),
                             target: (target as NSString).appendingPathComponent(child),
                             onBytes: onBytes)
            }
            PaneTransfer.carryAttributes(attrs, to: target)

        case .typeRegular:
            try copyFile(source: source, target: target, onBytes: onBytes)
            PaneTransfer.carryAttributes(attrs, to: target)

        default:
            // 장치·소켓 같은 특수 파일. 조각낼 것이 없다.
            try fileManager.copyItem(atPath: source, toPath: target)
        }
    }

    private func copyFile(source: String, target: String, onBytes: (Int64) -> Void) throws {
        guard fileManager.createFile(atPath: target, contents: nil) else {
            throw CocoaError(.fileWriteUnknown)
        }
        let input = try FileHandle(forReadingFrom: URL(fileURLWithPath: source))
        defer { try? input.close() }
        let output = try FileHandle(forWritingTo: URL(fileURLWithPath: target))
        defer { try? output.close() }

        while true {
            if cancelRequested { throw Cancelled() }
            var count = 0
            try autoreleasepool {
                guard let data = try input.read(upToCount: PaneTransfer.chunk), !data.isEmpty else { return }
                try output.write(contentsOf: data)
                count = data.count
            }
            if count == 0 { break }
            onBytes(Int64(count))
        }
        try output.synchronize()
    }

    /// 권한과 수정 시각을 원본과 맞춘다. 못 맞춰도 복사는 성공이다.
    private static func carryAttributes(_ attrs: [FileAttributeKey: Any], to path: String) {
        var keep: [FileAttributeKey: Any] = [:]
        if let p = attrs[.posixPermissions] { keep[.posixPermissions] = p }
        if let d = attrs[.modificationDate] { keep[.modificationDate] = d }
        try? fileManager.setAttributes(keep, ofItemAtPath: path)
    }

    /// 쓰다 만 것을 지운다. 링크도 지울 수 있게 존재 여부를 lstat 으로 본다.
    private static func removePartial(_ path: String) {
        if fileManager.fileExists(atPath: path) || isLink(path) {
            try? fileManager.removeItem(atPath: path)
        }
    }

    static func trimSlash(_ p: String) -> String {
        var t = p
        while t.count > 1 && t.hasSuffix("/") { t.removeLast() }
        return t
    }

    /// 깨진 링크는 fileExists 가 false 를 준다. 자리가 비었는지 볼 때는 이것도 봐야 한다.
    static func isLink(_ path: String) -> Bool {
        (try? fileManager.attributesOfItem(atPath: path)[.type] as? FileAttributeType) == .typeSymbolicLink
    }

    /// 복사할 총량. 링크는 따라가지 않는다.
    static func treeSize(_ path: String) -> Int64 {
        guard let attrs = try? fileManager.attributesOfItem(atPath: path) else { return 0 }
        let type = attrs[.type] as? FileAttributeType
        if type == .typeRegular { return (attrs[.size] as? NSNumber)?.int64Value ?? 0 }
        guard type == .typeDirectory else { return 0 }

        var sum: Int64 = 0
        let walker = fileManager.enumerator(at: URL(fileURLWithPath: path),
                                            includingPropertiesForKeys: [.fileSizeKey, .isRegularFileKey],
                                            options: [], errorHandler: { _, _ in true })
        while let url = walker?.nextObject() as? URL {
            autoreleasepool {
                if let v = try? url.resourceValues(forKeys: [.fileSizeKey, .isRegularFileKey]),
                   v.isRegularFile == true {
                    sum += Int64(v.fileSize ?? 0)
                }
            }
        }
        return sum
    }

    /// 'name.txt' 가 있으면 'name 2.txt' 를 만든다.
    static func nonClashingPath(_ path: String) -> String {
        var clean = path
        let wasFolder = clean.hasSuffix("/")
        while clean.count > 1 && clean.hasSuffix("/") { clean.removeLast() }

        let ns = clean as NSString
        let dir = ns.deletingLastPathComponent
        let ext = ns.pathExtension
        let base = (ns.lastPathComponent as NSString).deletingPathExtension

        var n = 2
        while n < 1000 {
            let name = ext.isEmpty ? "\(base) \(n)" : "\(base) \(n).\(ext)"
            let candidate = (dir as NSString).appendingPathComponent(name)
            if !fileManager.fileExists(atPath: candidate) && !isLink(candidate) {
                return wasFolder ? candidate + "/" : candidate
            }
            n += 1
        }
        return path
    }

    // MARK: - 되돌리기

    func recordRename(from: String, to: String) {
        undoStack.append(UndoableAction(kind: .rename, from: from, to: to))
    }

    /// 이동 창(MoveFileView)처럼 이 클래스 밖에서 옮긴 것도 되돌릴 수 있게 적는다.
    func recordMoves(_ pairs: [(from: String, to: String)]) {
        guard !pairs.isEmpty else { return }
        undoStack.append(UndoableAction(kind: .move,
                                        items: pairs.map { .init(from: $0.from, to: $0.to) }))
    }

    /// 마지막 한 건을 되돌린다. 되돌리기 자체도 관문을 지난다.
    ///
    /// 여러 개를 묶은 건이면 거꾸로 하나씩 되돌린다. 일부만 되돌아가면
    /// 남은 것을 기록에 다시 얹는다 — 한 번 더 누르면 나머지를 해 본다.
    func undoLast(onBlocked: @escaping (String) -> Void,
                  onFinish: @escaping (Bool) -> Void) {
        guard !isRunning else {
            onBlocked(L("복사·이동이 끝난 뒤에 되돌리십시오."))
            onFinish(false)
            return
        }
        guard let action = undoStack.popLast() else {
            onFinish(false)
            return
        }

        let origins = action.items.map { $0.from }
        switch FileGuard.authorize(action: "undo", paths: origins + action.items.map { $0.to }) {
        case .failure(let err):
            undoStack.append(action)   // 못 되돌렸으니 기록을 되살린다
            onBlocked(err.localizedDescription)
            onFinish(false)
            return
        case .success:
            break
        }

        var left: [UndoableAction.Item] = []
        for item in action.items.reversed() {
            // 원래 자리가 이미 차 있으면 건드리지 않는다. 덮어쓰면 그 사이에
            // 생긴 다른 파일이 사라진다.
            if fileManager.fileExists(atPath: item.from) || PaneTransfer.isLink(item.from) {
                left.insert(item, at: 0)
                continue
            }
            if !moveOne(source: item.to, target: item.from) {
                left.insert(item, at: 0)
            }
        }
        if !left.isEmpty {
            undoStack.append(UndoableAction(kind: action.kind, items: left))
        }
        onFinish(left.isEmpty)
    }
}

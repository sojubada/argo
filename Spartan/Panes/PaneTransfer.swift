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
struct UndoableAction {
    enum Kind {
        case move
        case rename
    }

    let kind: Kind
    /// 되돌리려면 to 에서 from 으로 다시 옮기면 된다.
    let from: String
    let to: String

    var describe: String {
        let name = (to as NSString).lastPathComponent
        switch kind {
        case .move:   return "'\(name)' 이동을 되돌립니다"
        case .rename: return "'\(name)' 이름 변경을 되돌립니다"
        }
    }
}

@available(tvOS 13.0, *)
final class PaneTransfer: ObservableObject {

    @Published var isRunning = false
    /// 0.0 ~ 1.0. 항목 수 기준이다. 바이트 기준이 더 정확하지만
    /// FileManager.copyItem 이 진행률을 주지 않아 항목 단위가 전부다.
    @Published var progress: Double = 0
    @Published var currentName: String = ""
    @Published var completed = 0
    @Published var total = 0

    /// 되돌리기 기록. 마지막 하나만 쓰지만 몇 개를 쌓아 둔다.
    @Published private(set) var undoStack: [UndoableAction] = []

    private var cancelRequested = false

    var canUndo: Bool { !undoStack.isEmpty }
    var nextUndo: UndoableAction? { undoStack.last }

    func cancel() {
        cancelRequested = true
    }

    /// 여러 항목을 목적지로 복사하거나 옮긴다.
    /// 끝나면 onFinish 가 메인에서 불린다 — 양쪽 칸을 다시 읽는 데 쓴다.
    func run(sources: [String],
             destination: String,
             move: Bool,
             onBlocked: @escaping (String) -> Void,
             onFinish: @escaping (Int, [String]) -> Void) {

        let targets = sources.map { destination + (($0 as NSString).lastPathComponent) }

        // 관문을 먼저 지난다. 한 건이라도 막히면 전부 멈춘다 —
        // 절반만 옮겨진 상태가 가장 수습하기 어렵다.
        switch FileGuard.authorize(action: move ? "move" : "copy", paths: targets) {
        case .failure(let err):
            onBlocked(err.localizedDescription)
            return
        case .success:
            break
        }

        cancelRequested = false
        isRunning = true
        progress = 0
        completed = 0
        total = sources.count

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            var failed: [String] = []
            var done = 0

            for (i, source) in sources.enumerated() {
                guard let self = self else { return }
                if self.cancelRequested { break }

                let name = (source as NSString).lastPathComponent
                let target = destination + name

                DispatchQueue.main.async {
                    self.currentName = name
                }

                let ok = self.transfer(source: source, target: target, move: move)
                if ok {
                    done += 1
                    if move {
                        DispatchQueue.main.async {
                            self.undoStack.append(UndoableAction(kind: .move,
                                                                 from: source,
                                                                 to: target))
                        }
                    }
                } else {
                    failed.append(name)
                }

                DispatchQueue.main.async {
                    self.completed = i + 1
                    self.progress = Double(i + 1) / Double(max(1, sources.count))
                }
            }

            DispatchQueue.main.async {
                self?.isRunning = false
                self?.currentName = ""
                onFinish(done, failed)
            }
        }
    }

    /// 한 건. FileManager 로 하고, 실패하면 헬퍼로 한 번 더 시도한다.
    /// 헬퍼는 실제로 돌지 않는 환경이 많아 FileManager 가 사실상 전부다.
    private func transfer(source: String, target: String, move: Bool) -> Bool {
        if fileManager.fileExists(atPath: target) {
            // 덮어쓰기는 하지 않는다. 이름을 비켜 쓴다 — 조용한 자료 손실을 막는다.
            let unique = Self.nonClashingPath(target)
            return perform(source: source, target: unique, move: move)
        }
        return perform(source: source, target: target, move: move)
    }

    private func perform(source: String, target: String, move: Bool) -> Bool {
        do {
            if move {
                try fileManager.moveItem(atPath: source, toPath: target)
            } else {
                try fileManager.copyItem(atPath: source, toPath: target)
            }
            OperationLog.record(action: move ? "MOVE (pane)" : "COPY (pane)",
                                path: source,
                                destination: target)
            return true
        } catch {
            print("[PaneTransfer] \(source) -> \(target) 실패: \(error.localizedDescription)")
            if move {
                RootHelperActs.mv(source, target)
            } else {
                RootHelperActs.cp(source, target)
            }
            return fileManager.fileExists(atPath: target)
        }
    }

    /// 'name.txt' 가 있으면 'name 2.txt' 를 만든다.
    static func nonClashingPath(_ path: String) -> String {
        let ns = path as NSString
        let dir = ns.deletingLastPathComponent
        let ext = ns.pathExtension
        let base = (ns.lastPathComponent as NSString).deletingPathExtension

        var n = 2
        while n < 1000 {
            let name = ext.isEmpty ? "\(base) \(n)" : "\(base) \(n).\(ext)"
            let candidate = (dir as NSString).appendingPathComponent(name)
            if !fileManager.fileExists(atPath: candidate) { return candidate }
            n += 1
        }
        return path
    }

    // MARK: - 되돌리기

    func recordRename(from: String, to: String) {
        undoStack.append(UndoableAction(kind: .rename, from: from, to: to))
    }

    /// 마지막 한 건을 되돌린다. 되돌리기 자체도 관문을 지난다.
    func undoLast(onBlocked: @escaping (String) -> Void,
                  onFinish: @escaping (Bool) -> Void) {
        guard let action = undoStack.popLast() else {
            onFinish(false)
            return
        }

        switch FileGuard.authorize(action: "undo", paths: [action.from]) {
        case .failure(let err):
            undoStack.append(action)   // 못 되돌렸으니 기록을 되살린다
            onBlocked(err.localizedDescription)
            onFinish(false)
            return
        case .success:
            break
        }

        let ok = perform(source: action.to, target: action.from, move: true)
        if !ok { undoStack.append(action) }
        onFinish(ok)
    }
}

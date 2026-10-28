//
//  ExplorerTools.swift
//  Argo
//
//  탐색기 본연의 작은 도구들.
//
//  - 경로로 이동 (⌘L): 위쪽 경로 칸을 뗀 뒤로 경로를 직접 칠 곳이 없었다.
//  - 걸러보기 (⌘⇧F): 칸 머리에 표시할 자리만 있고 값을 넣는 곳이 없었다.
//  - 폴더 용량: 정보 창이 파일 크기만 보여 주었다.
//
//  셋 다 읽기만 한다. 디스크에 쓰지 않으므로 FileGuard 를 거칠 일이 없다.
//

import SwiftUI

// MARK: - 경로 풀기

enum PathResolver {

    /// 친 글을 실제 폴더로 바꾼다.
    ///
    /// - "~" 는 /var/mobile 로 편다.
    /// - 앞의 "/" 가 없으면 지금 폴더 기준으로 본다.
    /// - 대소문자가 틀린 조각은 실제 이름으로 고쳐 준다. tvOS 파일 시스템은
    ///   대소문자를 가린다 — "/var/mobile/media" 는 없는 폴더다. 손으로 치다
    ///   보면 늘 틀리는 자리라, 맞는 이름이 하나뿐이면 그것으로 간다.
    ///
    /// 파일을 가리키면 그 파일이 든 폴더와 파일 이름을 돌려준다.
    static func resolve(_ typed: String, relativeTo base: String)
        -> Result<(folder: String, file: String?), PathResolveError> {

        var text = typed.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return .failure(.empty) }

        if text == "~" { text = "/var/mobile/" }
        else if text.hasPrefix("~/") { text = "/var/mobile/" + String(text.dropFirst(2)) }
        if !text.hasPrefix("/") {
            text = (base.hasSuffix("/") ? base : base + "/") + text
        }
        let standard = (text as NSString).standardizingPath

        // 조각마다 실제 이름을 찾아 내려간다.
        var current = "/"
        let parts = standard.split(separator: "/").map(String.init)
        for (i, part) in parts.enumerated() {
            let candidate = (current as NSString).appendingPathComponent(part)
            if fileManager.fileExists(atPath: candidate) {
                current = candidate
                continue
            }
            // 대소문자만 다른 이름이 정확히 하나면 그것으로 간다.
            let names = (try? fileManager.contentsOfDirectory(atPath: current)) ?? []
            let same = names.filter { $0.lowercased() == part.lowercased() }
            if same.count == 1 {
                current = (current as NSString).appendingPathComponent(same[0])
                continue
            }
            let missing = "/" + parts[0...i].joined(separator: "/")
            return .failure(.missing(missing))
        }

        var isDir: ObjCBool = false
        guard fileManager.fileExists(atPath: current, isDirectory: &isDir) else {
            return .failure(.missing(current))
        }
        if isDir.boolValue {
            return .success((folder: current == "/" ? "/" : current + "/", file: nil))
        }
        let folder = (current as NSString).deletingLastPathComponent
        return .success((folder: folder == "/" ? "/" : folder + "/",
                         file: (current as NSString).lastPathComponent))
    }
}

enum PathResolveError: Error {
    case empty
    case missing(String)

    var message: String {
        switch self {
        case .empty:            return "경로를 입력하십시오."
        case .missing(let p):   return "없는 경로입니다: " + p
        }
    }
}

// MARK: - 경로로 이동

struct GoToPathView: View {
    /// 지금 칸의 폴더. 칸에 처음 채워 두고, 상대 경로의 기준이 된다.
    let startDirectory: String
    @Binding var isPresented: Bool
    /// 폴더로 간다. 파일을 쳤으면 두 번째 값에 파일 이름이 온다.
    let onGo: (String, String?) -> Void

    @State private var text = ""
    @State private var error: String?

    var body: some View {
        VStack(spacing: 24) {
            Text("경로로 이동")
                .font(.title2)

            ArgoTextField("/var/mobile/Media/", text: $text, onCommit: go)
                .disableAutocorrection(true)
                .textInputAutocapitalization(.never)

            Text("~ 는 /var/mobile 입니다. / 없이 치면 지금 폴더 안에서 찾습니다. 대소문자는 틀려도 맞춰 갑니다.")
                .font(.caption)
                .foregroundColor(.secondary)
                .multilineTextAlignment(.center)

            if let error {
                Text(error)
                    .foregroundColor(.red)
                    .multilineTextAlignment(.center)
            }

            HStack(spacing: 40) {
                Button("이동", action: go)
                Button("취소") { isPresented = false }
            }
        }
        .padding(40)
        .onAppear {
            text = startDirectory
            ArgoTextEntry.begin()
        }
        .onDisappear { ArgoTextEntry.end() }
    }

    private func go() {
        switch PathResolver.resolve(text, relativeTo: startDirectory) {
        case .failure(let e):
            error = e.message
        case .success(let r):
            error = nil
            isPresented = false
            onGo(r.folder, r.file)
        }
    }
}

// MARK: - 걸러보기

struct FilterListView: View {
    let initial: String
    @Binding var isPresented: Bool
    /// 빈 글이면 걸러보기를 끈다.
    let onApply: (String) -> Void

    @State private var text = ""

    var body: some View {
        VStack(spacing: 24) {
            Text("걸러보기")
                .font(.title2)

            ArgoTextField("이름 일부, .mp4, *.log …", text: $text, onCommit: apply)
                .disableAutocorrection(true)
                .textInputAutocapitalization(.never)

            Text("빈칸이나 쉼표로 여러 개를 주면 어느 하나라도 맞는 항목을 보여 줍니다. 다른 폴더로 가면 풀립니다.")
                .font(.caption)
                .foregroundColor(.secondary)
                .multilineTextAlignment(.center)

            HStack(spacing: 40) {
                Button("적용", action: apply)
                Button("풀기") {
                    isPresented = false
                    onApply("")
                }
                Button("취소") { isPresented = false }
            }
        }
        .padding(40)
        .onAppear {
            text = initial
            ArgoTextEntry.begin()
        }
        .onDisappear { ArgoTextEntry.end() }
    }

    private func apply() {
        isPresented = false
        onApply(text.trimmingCharacters(in: .whitespacesAndNewlines))
    }
}

// MARK: - 폴더 용량

/// 정보 창에 붙는 한 줄. 누를 때만 센다 — 큰 폴더는 오래 걸려서,
/// 창을 열 때마다 세면 정보 창 자체가 무거워진다.
struct FolderSizeRow: View {
    let path: String

    @StateObject private var counter = FolderSizeCounter()

    var body: some View {
        VStack(spacing: 8) {
            switch counter.state {
            case .idle:
                Button("폴더 용량 계산") { counter.start(path) }
            case .running:
                HStack(spacing: 12) {
                    ProgressView()
                    Text("세는 중 — " + counter.summary)
                }
                Button("멈추기") { counter.stop() }
            case .done:
                Text("폴더 용량: " + counter.summary)
                if counter.unreadable > 0 {
                    Text("읽지 못한 곳 \(counter.unreadable)개는 빠졌습니다")
                        .font(.caption)
                        .foregroundColor(.orange)
                }
            case .stopped:
                Text("멈춤 — 여기까지 " + counter.summary)
                Button("다시 계산") { counter.start(path) }
            }
        }
        .onDisappear { counter.stop() }
    }
}

final class FolderSizeCounter: ObservableObject {
    enum Phase { case idle, running, done, stopped }

    @Published var state: Phase = .idle
    @Published var bytes: Int64 = 0
    @Published var files = 0
    @Published var folders = 0
    @Published var unreadable = 0

    /// 셈 한 번마다 새 깃발을 쓴다. 다시 세기를 누르면 앞의 셈은 제 깃발을
    /// 보고 멈추고, 늦게 끝나더라도 새 셈의 숫자를 덮어쓰지 못한다.
    private final class Flag {
        private let lock = NSLock()
        private var value = false
        var isSet: Bool { lock.lock(); defer { lock.unlock() }; return value }
        func set() { lock.lock(); value = true; lock.unlock() }
    }
    private var flag = Flag()

    var summary: String {
        let size = ByteCountFormatter.string(fromByteCount: bytes, countStyle: .file)
        return "\(size) (파일 \(files)개, 폴더 \(folders)개)"
    }

    func start(_ path: String) {
        flag.set()
        let mine = Flag()
        flag = mine
        bytes = 0; files = 0; folders = 0; unreadable = 0
        state = .running

        DispatchQueue.global(qos: .utility).async { [weak self] in
            guard let self else { return }
            var b: Int64 = 0, f = 0, d = 0, bad = 0
            var lastPush = Date()
            let keys: [URLResourceKey] = [.isDirectoryKey, .isSymbolicLinkKey, .fileSizeKey]
            // 링크는 따라가지 않는다. 따라가면 같은 것을 두 번 세거나
            // 시스템 전체를 돌게 된다.
            let walker = fileManager.enumerator(
                at: URL(fileURLWithPath: path),
                includingPropertiesForKeys: keys,
                options: [],
                errorHandler: { _, _ in bad += 1; return true })

            while let url = walker?.nextObject() as? URL {
                if mine.isSet { break }
                autoreleasepool {
                    let v = try? url.resourceValues(forKeys: Set(keys))
                    if v?.isSymbolicLink == true {
                        f += 1
                    } else if v?.isDirectory == true {
                        d += 1
                    } else {
                        f += 1
                        b += Int64(v?.fileSize ?? 0)
                    }
                }
                if Date().timeIntervalSince(lastPush) > 0.25 {
                    lastPush = Date()
                    let (bb, ff, dd) = (b, f, d)
                    DispatchQueue.main.async {
                        guard self.flag === mine else { return }
                        self.bytes = bb; self.files = ff; self.folders = dd
                    }
                }
            }

            let stopped = mine.isSet
            DispatchQueue.main.async {
                guard self.flag === mine else { return }
                self.bytes = b; self.files = f; self.folders = d; self.unreadable = bad
                self.state = stopped ? .stopped : .done
            }
        }
    }

    func stop() {
        flag.set()
    }
}

//
//  FilePaneModel.swift
//  Argo
//
//  칸 하나의 상태 전부. 칸이 둘이 되려면 이 안에 다 들어 있어야 한다.
//  ContentView는 이것을 두 개 들고, 활성 칸 쪽으로만 명령을 보낸다.
//

import Foundation
import SwiftUI

enum PaneSide {
    case left
    case right
}

/// 어느 칸이 활성인가. 클래스에 둔다 — @State 였을 때 문제가 있었다.
///
/// 항목을 누르면 활성 칸을 먼저 바꾸고 곧바로 defaultAction 을 부르는데,
/// defaultAction 은 directory·masterFiles 계산 속성을 통해 '지금 활성 칸'을
/// 다시 읽는다. @State 는 액션 클로저 안에서 쓴 값을 같은 클로저의 다음
/// 읽기에서 돌려준다는 보장이 없어, 오른쪽 칸을 눌렀는데 왼쪽 칸이 열리고
/// 보고 있는 칸은 그대로였다 — 폴더에 못 들어가는 것처럼 보인 원인이다.
///
/// 클래스 참조라면 쓰고 읽는 것이 같은 저장소를 가리켜 즉시 보인다.
@available(tvOS 13.0, *)
final class PaneSelection: ObservableObject {
    @Published var active: PaneSide = .left
}

enum SortKey: String, CaseIterable {
    case name
    case size
    case date
    case kind

    var label: String {
        switch self {
        case .name: return "이름"
        case .size: return "크기"
        case .date: return "날짜"
        case .kind: return "종류"
        }
    }

    var symbol: String {
        switch self {
        case .name: return "textformat.abc"
        case .size: return "arrow.up.arrow.down"
        case .date: return "clock"
        case .kind: return "doc.on.doc"
        }
    }
}

@available(tvOS 13.0, *)
final class FilePaneModel: ObservableObject {

    // MARK: - 상태

    @Published var directory: String = "/var/mobile/Media/"
    /// 화면에 그려지는 목록. 걸러보기·정렬이 적용된 결과다.
    @Published var masterFiles: [SpartanFile] = []

    @Published var multiSelect = false
    @Published var multiSelectFiles: [String] = []
    @Published var allWereSelected = false

    /// 포커스가 올라가 있는 줄. 스페이스로 고를 대상이다.
    ///
    /// tvOS 는 포커스를 포커스 엔진이 들고 있어서, 어느 줄이 눌릴 차례인지
    /// 물어볼 데가 없습니다. 칸이 직접 받아 적어 둡니다.
    @Published var focusedIndex: Int?

    /// 이 칸에서 마지막으로 포커스가 올라갔던 줄의 **경로**.
    ///
    /// focusedIndex 는 포커스가 목록을 떠나면 nil 이 된다. 길게 눌러 메뉴가
    /// 뜨거나 툴바로 올라가면 포커스가 거기로 옮겨 가므로, '지금 무엇을
    /// 대상으로 하나' 를 그것만으로 정하면 메뉴의 삭제·복사가 대상을 잃는다.
    /// 그래서 마지막 줄을 따로 들고 있는다.
    ///
    /// 번호가 아니라 경로로 든다. 폴더를 옮기면 번호는 새 목록의 엉뚱한 줄을
    /// 가리키지만, 경로는 새 목록에 없으니 저절로 무효가 된다.
    @Published var lastFocusedPath: String?

    /// 마지막으로 포커스됐던 줄. 지금 목록에 실제로 있을 때만.
    var lastFocusedIndex: Int? {
        guard let path = lastFocusedPath else { return nil }
        return masterFiles.firstIndex { $0.fullPath == path }
    }

    @Published var currentDirRisk: PathRisk = .safe

    // 2-D: 정렬·걸러보기·숨김
    @Published var sort: SortKey = .name
    @Published var ascending = true
    @Published var showHidden = true
    @Published var filter: String = ""

    // 배너는 칸마다 따로 뜬다. 어느 칸이 시스템 폴더인지 보여야 하기 때문이다.
    @Published var showBanner = false
    @Published var bannerRisk: PathRisk = .safe
    @Published var bannerMsg: String = ""

    /// ⌘[ ⌘] 용 기록. 폴더를 바꿀 때만 쌓인다.
    @Published private(set) var backStack: [String] = []
    @Published private(set) var forwardStack: [String] = []

    /// 걸러보기·정렬 전의 원본. 정렬만 바꿀 때 디스크를 다시 읽지 않으려고 들고 있는다.
    private var rawFiles: [SpartanFile] = []

    init(directory: String = "/var/mobile/Media/") {
        self.directory = directory
        self.showHidden = UserDefaults.settings.object(forKey: "showHiddenFiles") as? Bool ?? true
    }

    // MARK: - 목록 읽기

    /// 디스크를 다시 읽는다. 원래 ContentView.updateFiles()와 같은 일을 한다.
    func updateFiles() {
        rawFiles = readDirectory()
        applySortAndFilter()
        normalizeDirectoryPath()
        evaluateRisk()
        // 옛 cacheFolder 를 2초 뒤에 따로 부르던 것을 뗐다.
        //
        // 이 모델이 캐시를 이미 직접 쓴다(writeCacheIfWritable·pruneCache).
        // cacheFolder 는 같은 일을 /var/mobile/Media/temp 라는 **공용 파일
        // 하나**에 쓴 뒤 옮기는 식이었다. 두 칸이 동시에 새로 읽으면 한 칸의
        // 목록이 다른 칸 폴더의 캐시로 들어갈 수 있었다. 지금까지는 그 옮기기가
        // 늘 실패해서 무해했을 뿐이다 — 작업 기록의 'MOVE (root)' 들이 그것이다.
    }

    /// 정렬·걸러보기만 다시 적용한다. 디스크를 읽지 않는다.
    func applySortAndFilter() {
        var out = rawFiles

        if !showHidden {
            out = out.filter { !$0.name.hasPrefix(".") }
        }

        let needle = filter.trimmingCharacters(in: .whitespaces).lowercased()
        if !needle.isEmpty {
            out = out.filter { $0.name.lowercased().contains(needle) }
        }

        out.sort { a, b in
            // 폴더가 언제나 먼저 온다. 정렬 기준과 무관하다.
            let aDir = a.name.hasSuffix("/")
            let bDir = b.name.hasSuffix("/")
            if aDir != bDir { return aDir }

            let result: Bool
            switch sort {
            case .name:
                result = a.name.localizedStandardCompare(b.name) == .orderedAscending
            case .size:
                // 디스크를 치지 않는다. 폴더를 읽을 때 채워 둔 값이다.
                result = (a.size ?? 0) < (b.size ?? 0)
            case .date:
                result = (a.modified ?? .distantPast) < (b.modified ?? .distantPast)
            case .kind:
                let ae = (a.name as NSString).pathExtension.lowercased()
                let be = (b.name as NSString).pathExtension.lowercased()
                if ae == be {
                    result = a.name.localizedStandardCompare(b.name) == .orderedAscending
                } else {
                    result = ae < be
                }
            }
            return ascending ? result : !result
        }

        masterFiles = out
        // 목록이 바뀌면 선택 배열의 길이가 안 맞는다. 원본 코드와 같은 이유로 비운다.
        resetMultiSelectArrays()
    }

    /// 원래 updateFiles + oldUpdate 가 하던 일. 캐시가 있으면 쓰고 없으면 훑는다.
    private func readDirectory() -> [SpartanFile] {
        guard let metaName = metadataName else { return scanDirectory() }

        guard fileManager.fileExists(atPath: directory + metaName),
              let encoded = fileManager.contents(atPath: directory + metaName) else {
            return scanDirectory()
        }

        let decoder = JSONDecoder()
        var decoded: [SpartanFile] = []
        var out: [SpartanFile] = []
        do {
            decoded = try decoder.decode([SpartanFile].self, from: encoded)
            let names = try currentNames()
            let decodedNames = decoded.map { $0.name }
            for name in names {
                if let j = decodedNames.firstIndex(of: name) {
                    // 전체 경로는 캐시에서 가져오지 않고 지금 폴더로 다시 짓는다.
                    //
                    // 캐시 파일은 폴더 안에 있어 폴더와 함께 옮겨지고 이름도 같이
                    // 바뀐다. 그 안의 fullPath 는 옛 자리를 가리킨 채로 남는다.
                    // 그대로 쓰면 이름을 바꾼 폴더에서 파일을 열거나 지울 때
                    // 옛 자리 — 다른 폴더일 수도 있다 — 의 같은 이름 파일을
                    // 건드리게 된다.
                    var entry = decoded[j]
                    entry.fullPath = directory + name
                    out.append(fillMissingAttributes(entry))
                } else {
                    out.append(makeFile(name))
                }
            }
        } catch {
            print("[FilePane] 캐시를 읽지 못해 직접 훑습니다: \(error)")
            return scanDirectory()
        }

        pruneCache(decoded: decoded, live: out)
        return out
    }

    private func scanDirectory() -> [SpartanFile] {
        var out: [SpartanFile] = []
        do {
            for name in try currentNames() {
                out.append(makeFile(name))
            }
            writeCacheIfWritable(out)
        } catch {
            print("[FilePane] \(directory) 를 읽지 못했습니다: \(error.localizedDescription)")
        }
        return out
    }

    /// 캐시는 쓸 수 있는 곳에만 쓴다.
    ///
    /// 전에는 폴더를 읽을 때마다 무조건 썼다. 주의·금지 폴더에서는 관문이
    /// 막았고, 그 차단이 알림으로 올라와 폴더를 둘러보는 내내 경고가 떴다.
    /// 캐시는 편의일 뿐이라 못 쓰면 조용히 넘어가는 것이 맞다.
    private func writeCacheIfWritable(_ files: [SpartanFile]) {
        guard let metaName = metadataName else { return }
        guard PathSafety.isWriteAllowed(directory) else { return }
        let target = directory + metaName
        do {
            let encoded = try JSONEncoder().encode(files)
            try encoded.write(to: URL(fileURLWithPath: target), options: .atomic)
        } catch {
            // 캐시를 못 썼을 뿐이다. 목록은 이미 읽었다.
            print("[FilePane] 캐시를 쓰지 못했습니다: \(error.localizedDescription)")
        }
    }

    private func currentNames() throws -> [String] {
        let contents = try fileManager.contentsOfDirectory(atPath: directory)
        var files = contents.map { file -> String in
            var isDirectory: ObjCBool = false
            fileManager.fileExists(atPath: directory + file, isDirectory: &isDirectory)
            return isDirectory.boolValue ? "\(file)/" : file
        }
        if let metaName = metadataName, let i = files.firstIndex(of: metaName) {
            files.remove(at: i)   // 캐시 파일 자체는 숨긴다
        }
        return files
    }

    private func makeFile(_ name: String) -> SpartanFile {
        let full = directory + name
        let attrs = try? fileManager.attributesOfItem(atPath: full)
        return SpartanFile(name: name,
                           fullPath: full,
                           isSelected: false,
                           fileType: FileInfo.yandereDevFileType(file: full),
                           isLoadingFile: false,
                           size: attrs?[.size] as? Int64,
                           modified: attrs?[.modificationDate] as? Date)
    }

    /// 캐시에서 온 항목은 크기·날짜가 없을 수 있다 (예전 캐시).
    /// 그때만 한 번 읽어 채운다. 다음 캐시 갱신에 함께 저장된다.
    private func fillMissingAttributes(_ f: SpartanFile) -> SpartanFile {
        guard f.size == nil || f.modified == nil else { return f }
        var out = f
        if let attrs = try? fileManager.attributesOfItem(atPath: f.fullPath) {
            out.size = attrs[.size] as? Int64
            out.modified = attrs[.modificationDate] as? Date
        }
        return out
    }

    /// 사라진 파일을 캐시에서 덜어낸다. 안 하면 캐시가 끝없이 자란다.
    private func pruneCache(decoded: [SpartanFile], live: [SpartanFile]) {
        guard let metaName = metadataName else { return }
        let dir = directory
        guard PathSafety.isWriteAllowed(dir) else { return }
        DispatchQueue.global().asyncAfter(deadline: .now() + 0.5) {
            // 원본은 순회 중에 remove(at:)를 해서 인덱스가 밀렸다. filter로 바꿨다.
            let kept = decoded.filter { live.contains($0) }
            do {
                let encoded = try JSONEncoder().encode(kept)
                try encoded.write(to: URL(fileURLWithPath: dir + metaName), options: .atomic)
            } catch {
                print("[FilePane] 캐시를 갱신하지 못했습니다: \(error.localizedDescription)")
            }
        }
    }

    private func normalizeDirectoryPath() {
        if UserDefaults.settings.bool(forKey: "autoComplete"),
           !directory.hasSuffix("/"),
           FileInfo.isDirectory(filePath: directory) {
            directory += "/"
        }
        if directory == "//" { directory = "/" }
    }

    /// 1.2초 경고 배너. 시스템에 영향을 주는 폴더에 들어갔을 때만 뜬다.
    private func evaluateRisk() {
        currentDirRisk = PathSafety.risk(of: directory)
        guard currentDirRisk != .safe else { return }
        bannerRisk = currentDirRisk
        bannerMsg = currentDirRisk == .caution
            ? "주의 — 시스템에 영향을 줄 수 있습니다"
            : "시스템 폴더 — 쓰기가 차단됩니다"
        withAnimation { showBanner = true }
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { [weak self] in
            withAnimation { self?.showBanner = false }
        }
    }

    // MARK: - 이동

    /// 폴더를 바꾼다. 되돌아갈 기록을 남기는 유일한 통로다.
    func navigate(to newPath: String, recordHistory: Bool = true) {
        let from = directory
        guard newPath != from else { return }
        if recordHistory {
            backStack.append(from)
            forwardStack.removeAll()
        }
        directory = newPath
        multiSelect = false
        filter = ""
        updateFiles()
    }

    func enter(_ name: String) {
        navigate(to: directory + name)
    }

    func goUp() {
        guard directory != "/" else { return }
        var components = directory.split(separator: "/")
        if components.count > 1 {
            components.removeLast()
            var up = "/" + components.joined(separator: "/") + "/"
            if up == "//" { up = "/" }
            navigate(to: up)
        } else {
            navigate(to: "/")
        }
    }

    func goBack() {
        guard let previous = backStack.popLast() else { return }
        forwardStack.append(directory)
        directory = previous
        multiSelect = false
        updateFiles()
    }

    func goForward() {
        guard let next = forwardStack.popLast() else { return }
        backStack.append(directory)
        directory = next
        multiSelect = false
        updateFiles()
    }

    var canGoBack: Bool { !backStack.isEmpty }
    var canGoForward: Bool { !forwardStack.isEmpty }

    /// 키보드로 글자를 치면 그 글자로 시작하는 첫 항목을 찾는다.
    /// 파일 300개 폴더에서 이게 가장 큰 이득이다.
    func indexOfFirstMatch(prefix: String) -> Int? {
        guard !prefix.isEmpty else { return nil }
        let needle = prefix.lowercased()
        return masterFiles.firstIndex { $0.name.lowercased().hasPrefix(needle) }
    }

    // MARK: - 선택

    func resetMultiSelectArrays() {
        multiSelectFiles = []
        allWereSelected = false
        for i in masterFiles.indices where masterFiles[i].isSelected {
            masterFiles[i].isSelected = false
        }
    }

    func setAllSelected(_ selected: Bool) {
        for i in masterFiles.indices {
            masterFiles[i].isSelected = selected
        }
        multiSelectFiles = selected ? masterFiles.map { $0.name } : []
        allWereSelected = selected
    }

    func toggleSelection(at index: Int) {
        guard masterFiles.indices.contains(index) else { return }
        masterFiles[index].isSelected.toggle()
        let name = masterFiles[index].name
        if masterFiles[index].isSelected {
            if !multiSelectFiles.contains(name) { multiSelectFiles.append(name) }
        } else {
            multiSelectFiles.removeAll { $0 == name }
        }
    }

    /// 포커스된 줄을 고르거나 풉니다. 스페이스가 부르는 자리입니다.
    ///
    /// 고르기 모드를 따로 켜게 하지 않습니다 — 처음 스페이스를 누르면
    /// 그것이 곧 '고르겠다'는 뜻이므로 모드를 같이 켜고 동그라미를 띄웁니다.
    /// 마지막 하나를 풀면 모드도 같이 내려, 쓰지 않는 동그라미가 줄마다
    /// 남지 않게 합니다.
    @discardableResult
    func toggleSelectionAtFocus() -> Bool {
        guard let index = focusedIndex, masterFiles.indices.contains(index) else { return false }
        if !multiSelect { multiSelect = true }
        toggleSelection(at: index)
        if multiSelectFiles.isEmpty { multiSelect = false }
        return true
    }

    /// 선택된 항목의 전체 경로. 선택이 없으면 빈 배열이다.
    var selectedPaths: [String] {
        masterFiles.filter { $0.isSelected }.map { $0.fullPath }
    }

    var selectedNames: [String] {
        masterFiles.filter { $0.isSelected }.map { $0.name }
    }
}

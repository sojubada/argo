# 검증 D — 두 칸 · 키보드 · 클립보드 · 전송 · 압축

커밋 `6869fc9`. **전부 빌드만 통과했고 기기에서 확인되지 않았습니다.**

---

## 답해 주셔야 할 질문

### Q1. 계산 속성 수법이 안전합니까?  ★

`ContentView` 는 1,880줄이고 `directory` 를 134곳, `masterFiles` 를 146곳에서
씁니다. 칸이 둘이 되려면 그 280곳이 **어느 칸** 이야기인지 알아야 합니다.

전부 고치는 대신 **이름을 그대로 두고 계산 속성으로** 바꿨습니다.

```swift
var directory: String {
    get { pane.directory }
    nonmutating set { pane.directory = newValue }
}
```

**이 수법 때문에 이미 한 번 당했습니다.** 처음에는 `activeSide` 가
`@State` 였습니다. 항목을 누르면 활성 칸을 먼저 바꾸고 곧바로
`defaultAction` 을 부르는데, 그 함수가 계산 속성을 통해 **활성 칸을 다시
읽습니다.** `@State` 는 액션 클로저 안에서 쓴 값이 **같은 클로저의 다음
읽기에 보인다는 보장이 없어**, 오른쪽 칸을 눌러도 왼쪽 칸이 열렸습니다.

지금은 `PaneSelection` 이라는 클래스에 담았습니다 — 쓰고 읽는 것이
같은 저장소를 가리켜 즉시 보입니다.

**질문:**
- 이 구조에 **남은 구멍**이 있습니까? 특히 비동기 콜백에서 `pane` 을
  늦게 읽는 곳
- `nonmutating set` 이 구조체에서 성립하는 근거가 "`pane` 이 클래스
  참조라 구조체를 변형하지 않는다" 입니다. 맞습니까?
- SwiftUI 가 두 `@StateObject` 의 변화를 제대로 받습니까? 활성 칸만
  다시 그려지고 반대 칸이 멈추는 일은 없습니까?

제미나이께서 **전면 리팩토링**을 지시하셨고 제가 반대했습니다. 근거는
"비동기 경로를 전수로 훑었더니 전부 인스턴스와 경로를 지역 변수로 고정하고
있더라" 였습니다. **그 뒤 같은 클로저 안의 `@State` 문제로 틀렸습니다.**
제 반대의 근거가 절반만 맞았던 셈이니, 지금 다시 판단해 주십시오.

### Q2. 클립보드가 맞습니까?

`⌘C`/`⌘X` 로 담고 `⌘V` 로 지금 폴더에 붙입니다. 담기면 툴바에 붙여넣기
버튼이 나타나 리모컨만으로도 됩니다.

- 잘라내기 후 붙여넣기가 실패하면 원본이 남습니까? (`PaneTransfer` 가
  관문에서 막히면 아무것도 안 하고, 성공해야만 클립보드를 비웁니다)
- 같은 폴더에 붙여넣으면 어떻게 됩니까? (`name 2.txt` 로 비켜 씁니다)
- 선택이 없을 때 `newViewFilePath`(마지막으로 만진 파일) 하나로 떨어지는데,
  **사용자가 의도하지 않은 파일이 복사될** 위험이 있습니까?

### Q3. 전송의 취소와 진행률이 솔직합니까?

- 진행률이 **항목 수** 기준입니다. `FileManager.copyItem` 이 진행 중
  아무것도 알려주지 않아 바이트 기준이 불가능합니다. 10GB 파일 하나면
  0%에서 100%로 점프합니다
- 취소를 **항목 사이**에서만 봅니다. 한 항목 복사 중에는 안 듣습니다

이 한계를 그대로 두는 것이 맞습니까, 아니면 `read`/`write` 로 직접 옮겨
바이트 진행률을 만들어야 합니까?

### Q4. 압축의 빈 공간 검사가 맞습니까?

해제 쪽은 공짜였습니다 — 관문이 Zip Slip 을 보려고 이미 모든 헤더를 훑으니
지나가면서 `archive_entry_size` 를 더하기만 했습니다. 압축 쪽은 원본 합계를
셉니다. 둘 다 여유 10% 를 둡니다.

- `archive_entry_size` 가 0 이거나 음수인 항목이 있는 아카이브에서
  합계가 틀립니까?
- 압축은 보통 줄어드는데 원본 크기만큼을 요구하는 것이 과합니까?

### Q5. 단축키 맵핑에 충돌이 없습니까?

`ArgoCommand.binding(for:)` 한 곳에 정의가 있고 `⌘/` 도움말이 같은 함수를
읽습니다. **같은 조합이 둘이면 앞의 것이 이깁니다.** 중복을 찾아 주십시오.

그리고 `textEntryActive` — 글자를 받는 칸이 뜨면 단축키를 막습니다.
경로 칸과 시트 5개에 걸었습니다. **빠진 입력 칸이 있습니까?**

---

## 1. 칸 모델

### `Spartan/Panes/FilePaneModel.swift`  (392줄)

```swift
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

        if PathSafety.isWriteAllowed(directory) {
            DispatchQueue.global().asyncAfter(deadline: .now() + 2) { [directory] in
                cacheFolder(directory)
            }
        }
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
                    out.append(fillMissingAttributes(decoded[j]))
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

    /// 선택된 항목의 전체 경로. 선택이 없으면 빈 배열이다.
    var selectedPaths: [String] {
        masterFiles.filter { $0.isSelected }.map { $0.fullPath }
    }

    var selectedNames: [String] {
        masterFiles.filter { $0.isSelected }.map { $0.name }
    }
}
```

**볼 곳**

- `navigate(to:)` 가 기록을 남기는 **유일한 통로**입니다. `directory` 를
  직접 쓰는 곳이 있으면 `⌘[` 기록이 어긋납니다
- `applySortAndFilter` 는 디스크를 다시 읽지 않습니다 — 대신 정렬 후
  **파일시스템 변화를 못 봅니다**
- `writeCacheIfWritable` — 캐시는 쓸 수 있는 곳에만 씁니다.
  전에는 무조건 쓰려다 주의 폴더에서 차단 알림이 떴고, **그 알림이
  `showAlert` 깃발을 세워 폴더 진입을 통째로 막았습니다**
- 크기·날짜를 폴더 읽을 때 한 번만 채웁니다. 선택형인 이유는 이미 디스크에
  깔린 캐시의 JSON 에 키가 없어 디코딩이 통째로 실패하지 않게 하려는 것입니다

---

## 2. 칸 뷰

### `Spartan/Panes/FilePaneView.swift`  (268줄)

```swift
//
//  FilePaneView.swift
//  Argo
//
//  목록을 그리는 뷰. FilePaneModel 하나만 받는다.
//  전역 상태를 참조하면 두 번 못 쓰기 때문이다.
//
//  파일을 여는 일은 하지 않는다. onOpen으로 ContentView에 돌려주고,
//  거기서 기존 defaultAction이 39개 시트를 그대로 띄운다.
//

import SwiftUI

/// 파일 종류 번호 → 아이콘. 원래 ContentView의 639줄 switch가 하던 일이다.
/// tvOS 13/14 분기로 같은 switch가 두 번 들어 있었다.
enum FileGlyph {
    static func symbol(for fileType: Double, name: String) -> String {
        switch Int(fileType) {
        case 0:  return name.hasSuffix("/") ? "folder" : "folder.fill"
        case 1:  return "waveform.circle"
        case 2:  return "video"
        case 3:  return "photo"
        case 4:  return "doc.text"
        case 6:  return "rectangle.compress.vertical"
        case 7:  return "terminal"
        case 8:  return "link"
        case 9:  return "archivebox"
        case 10: return "tray.full"
        case 11: return "textformat"
        case 12: return "photo.fill"
        case 13: return "externaldrive"
        case 14: return "safari"
        case 15: return "doc.richtext"
        case 16: return "doc.text.magnifyingglass"
        default: return "doc"
        }
    }
}

/// 길게 눌렀을 때 쓸 동작들. 뷰는 무엇을 하는지 모르고 부르기만 한다.
@available(tvOS 13.0, *)
struct PaneRowMenu {
    let copy: (Int) -> Void
    let cut: (Int) -> Void
    let paste: () -> Void
    let canPaste: () -> Bool
    let pasteLabel: () -> String
    let hasOtherPane: () -> Bool
    let sendToOtherPane: (Int, Bool) -> Void
    let rename: (Int) -> Void
    let info: (Int) -> Void
    let delete: (Int) -> Void
    let more: (Int) -> Void
    /// AirDrop 으로 보낼 수 있는 항목인가 (쓸 수 있는 기기이고, 폴더가 아닐 것).
    let canAirDrop: (Int) -> Bool
    let airDrop: (Int) -> Void
}

@available(tvOS 13.0, *)
struct FilePaneView: View {
    @ObservedObject var model: FilePaneModel

    /// 이 칸이 활성 칸인지. 테두리로 표시한다.
    let isActive: Bool

    /// 항목을 눌렀을 때. 인덱스는 model.masterFiles 기준이다.
    let onOpen: (Int) -> Void
    /// ".." 을 눌렀을 때.
    let onGoUp: () -> Void
    /// 이 칸에 포커스가 들어왔을 때. 활성 칸을 바꾸는 데 쓴다.
    let onFocus: () -> Void

    /// 길게 눌러 뜨는 메뉴의 동작들. ContentView 가 실제 일을 한다.
    /// 전에는 '자세히' 하나뿐이어서 시트를 한 번 더 거쳐야 했다 —
    /// 길게 누른 뜻이 '이 파일로 무언가 하겠다'인데 한 단계가 더 있었다.
    let menu: PaneRowMenu?

    init(model: FilePaneModel,
         isActive: Bool,
         onOpen: @escaping (Int) -> Void,
         onGoUp: @escaping () -> Void,
         onFocus: @escaping () -> Void,
         menu: PaneRowMenu? = nil) {
        self.model = model
        self.isActive = isActive
        self.onOpen = onOpen
        self.onGoUp = onGoUp
        self.onFocus = onFocus
        self.menu = menu
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            list
        }
        .argoSurface()
        .overlay(
            // 활성 칸 표시. 두 칸이면 어느 쪽이 명령을 받는지 보여야 한다.
            RoundedRectangle(cornerRadius: Theme.corner)
                .stroke(isActive ? Theme.accent : Color.clear, lineWidth: 3)
        )
        // 칸을 하나의 포커스 구역으로 묶는다. 구역 안에서 더 갈 곳이 없으면
        // 포커스가 구역 밖으로 나가며, 좌우 끝에서는 툴바(위쪽 구역)로 빠진다.
        // tvOS 15부터 있는 수정자이고 배포 대상이 16이라 쓸 수 있다.
        //
        // 1픽셀 투명 뷰로 포커스를 '받아서' 돌려보내는 방식은 쓰지 않는다.
        // SwiftUI에는 포커스를 보낼 수단이 없어, 받은 뒤 갇힐 수 있다 —
        // 파일 탐색기에서 포커스가 갇히면 앱을 끄는 수밖에 없다.
        .focusSection()
    }

    // MARK: - 머리

    private var header: some View {
        VStack(spacing: 4) {
            pathRow
            if !model.filter.isEmpty {
                HStack(spacing: 6) {
                    Image(systemName: "line.3.horizontal.decrease.circle")
                    Text(model.filter)
                        .font(.caption)
                    Spacer()
                }
                .foregroundColor(Theme.accent)
                .padding(.horizontal, 12)
            }
        }
    }

    private var pathRow: some View {
        HStack(spacing: 12) {
            if let symbol = model.currentDirRisk.badgeSymbol {
                Image(systemName: symbol)
                    .foregroundColor(model.currentDirRisk.badgeColor)
            }
            // 긴 경로는 앞을 줄입니다 — 끝쪽(지금 폴더)이 중요합니다.
            Text(model.directory)
                .font(.caption)
                .foregroundColor(Theme.textPrimary)
                .lineLimit(1)
                .truncationMode(.head)
            Spacer()
            Text("\(model.masterFiles.count)")
                .font(.caption)
                .foregroundColor(Theme.textSecondary)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
    }

    // MARK: - 목록

    private var list: some View {
        List {
            Button(action: onGoUp) {
                HStack {
                    Image(systemName: "arrowshape.turn.up.left")
                    Text("..")
                }
            }

            ForEach(model.masterFiles.indices, id: \.self) { index in
                row(index)
            }
        }
    }

    @ViewBuilder
    private func row(_ index: Int) -> some View {
        // 행당 포커스 버튼은 하나다. tvOS 포커스 규칙이고, 깨뜨리면 목록을 못 쓴다.
        // #available은 컴파일 시점 검사라 .if 수정자로는 분기할 수 없어 통째로 두 벌이다.
        if #available(tvOS 14.0, *) {
            Button(action: {
                onFocus()
                onOpen(index)
            }) {
                rowContent(index)
            }
            .contextMenu {
                // 동작을 바로 담는다. 여기서 한 번 더 시트를 거치면
                // 길게 누른 의미가 없다.
                if let menu = menu {
                    Button(action: { onFocus(); menu.copy(index) }) {
                        Text("복사")
                    }
                    Button(action: { onFocus(); menu.cut(index) }) {
                        Text("잘라내기")
                    }
                    if menu.canPaste() {
                        Button(action: { onFocus(); menu.paste() }) {
                            Text(menu.pasteLabel())
                        }
                    }
                    if menu.hasOtherPane() {
                        Button(action: { onFocus(); menu.sendToOtherPane(index, false) }) {
                            Text("반대 칸으로 복사")
                        }
                        Button(action: { onFocus(); menu.sendToOtherPane(index, true) }) {
                            Text("반대 칸으로 이동")
                        }
                    }
                    if menu.canAirDrop(index) {
                        Button(action: { onFocus(); menu.airDrop(index) }) {
                            Text("AirDrop 으로 보내기")
                        }
                    }
                    Button(action: { onFocus(); menu.rename(index) }) {
                        Text("이름 바꾸기")
                    }
                    Button(action: { onFocus(); menu.info(index) }) {
                        Text("정보")
                    }
                    Button(action: { onFocus(); menu.delete(index) }) {
                        Text("삭제")
                    }
                    Button(action: { onFocus(); menu.more(index) }) {
                        Text("더보기…")
                    }
                }
                // tvOS 컨텍스트 메뉴는 버튼을 눌러야 닫힌다. 다른 방법이 없다.
                Button("닫기", action: { })
            }
        } else {
            Button(action: {
                onFocus()
                onOpen(index)
            }) {
                rowContent(index)
            }
        }
    }

    @ViewBuilder
    private func rowContent(_ index: Int) -> some View {
        let file = model.masterFiles[index]
        HStack {
            if file.isLoadingFile {
                ProgressView()
            } else {
                if model.multiSelect {
                    Image(systemName: file.isSelected ? "checkmark.circle" : "circle")
                        .transition(.opacity)
                }

                // 위험도 배지. 부모 폴더가 안전하면 상속해 디스크를 덜 친다.
                let isSym = file.fileType == 8
                let itemRisk = PathSafety.fastRisk(parentRisk: model.currentDirRisk,
                                                   fullPath: file.fullPath,
                                                   isSymlink: isSym)
                if let symbol = itemRisk.badgeSymbol {
                    Image(systemName: symbol)
                        .foregroundColor(itemRisk.badgeColor)
                }

                Image(systemName: FileGlyph.symbol(for: file.fileType, name: file.name))

                // 심볼릭 링크는 이름 끝의 표시 문자를 떼고 보여준다.
                // 색을 못 박지 않는다. tvOS 는 포커스된 줄의 바탕을 흰색으로
                // 바꾸면서 글자색을 같이 뒤집는데, 여기서 흰색을 지정하면
                // 포커스된 줄의 파일명이 흰 바탕에 흰 글자가 되어 사라진다.
                Text(file.fileType == 8 ? removeLastChar(file.name) : file.name)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
        }
    }
}
```

**볼 곳**

- **행당 포커스 버튼은 하나입니다.** tvOS 규칙이고 깨뜨리면 목록을 못 씁니다
- 파일명에 **색을 못 박지 않습니다.** tvOS 가 포커스된 줄의 바탕을 흰색으로
  바꾸면서 글자색을 같이 뒤집는데, 흰색을 지정했더니 포커스된 줄의 이름이
  사라졌습니다
- `FileGlyph` 에 빠진 파일 종류가 없는지 (0·1·2·3·4·6·7·8·9·10·11·12·13·14·15·16 —
  **5가 없는 것이 원본과 같습니다**)

---

## 3. 두 칸 라우팅과 계산 속성

### `ContentView.swift (칸 보유와 계산 속성)`  (73줄)

```swift
    @StateObject var leftPane: FilePaneModel
    @StateObject var rightPane: FilePaneModel
    @StateObject private var selection = PaneSelection()

    /// 이름은 그대로 두고 저장소만 클래스로 옮겼다. 쓰고 바로 읽어도 새 값이 보인다.
    var activeSide: PaneSide {
        get { selection.active }
        nonmutating set { selection.active = newValue }
    }
    @StateObject var transfer = PaneTransfer()
    @StateObject var clipboard = ArgoClipboard()
    @StateObject var airdrop = AirDropState()

    /// 알림은 한 번만 뜨고 닫히면 지워진다.
    var airdropFailureBinding: Binding<Bool> {
        Binding(get: { airdrop.lastFailure != nil },
                set: { if !$0 { airdrop.lastFailure = nil } })
    }
    /// 두 칸을 함께 보일지. 끄면 한 칸만 나와 기존 동작과 같다.
    @AppStorage("dualPaneEnabled") var dualPaneEnabled = false
    /// 툴바에 나머지 아이콘을 펼칠지. 기본은 접어 둔다 —
    /// 25개가 한 줄에 늘어서면 무엇이 무엇인지 알 수 없다.
    @State private var toolbarExpanded = false

    /// 활성 칸. 툴바의 모든 동작은 이쪽을 대상으로 한다.
    var pane: FilePaneModel { activeSide == .left ? leftPane : rightPane }
    /// 반대 칸. 붙여넣기가 가는 곳이다.
    var otherPane: FilePaneModel { activeSide == .left ? rightPane : leftPane }

    // 아래 네 개는 기존 코드가 쓰던 이름 그대로다. 안쪽만 활성 칸을 가리킨다.
    // 구조체이지만 pane이 클래스 참조라 nonmutating set이 성립한다.
    var directory: String {
        get { pane.directory }
        nonmutating set { pane.directory = newValue }
    }
    var masterFiles: [SpartanFile] {
        get { pane.masterFiles }
        nonmutating set { pane.masterFiles = newValue }
    }
    var multiSelect: Bool {
        get { pane.multiSelect }
        nonmutating set { pane.multiSelect = newValue }
    }
    var multiSelectFiles: [String] {
        get { pane.multiSelectFiles }
        nonmutating set { pane.multiSelectFiles = newValue }
    }
    var allWereSelected: Bool {
        get { pane.allWereSelected }
        nonmutating set { pane.allWereSelected = newValue }
    }
    var currentDirRisk: PathRisk { pane.currentDirRisk }

    var directoryBinding: Binding<String> {
        Binding(get: { pane.directory }, set: { pane.directory = $0 })
    }
    var multiSelectBinding: Binding<Bool> {
        Binding(get: { pane.multiSelect }, set: { pane.multiSelect = $0 })
    }
    var multiSelectFilesBinding: Binding<[String]> {
        Binding(get: { pane.multiSelectFiles }, set: { pane.multiSelectFiles = $0 })
    }

    init(directory: String = "/var/mobile/Media/",
         isRootless: Bool = false,
         scaleFactor: CGFloat = 1,
         globalAVPlayer: Binding<AVPlayer>) {
        _leftPane = StateObject(wrappedValue: FilePaneModel(directory: directory))
        _rightPane = StateObject(wrappedValue: FilePaneModel(directory: directory))
        _isRootless = State(initialValue: isRootless)
        _scaleFactor = State(initialValue: scaleFactor)
        _globalAVPlayer = globalAVPlayer
    }
```

### `ContentView.swift (항목 열기)`  (27줄)

```swift

    /// 칸에서 항목을 눌렀을 때. 활성 칸을 먼저 바꿔야 defaultAction이 올바른 칸을 본다.
    /// directory·masterFiles가 활성 칸을 가리키는 계산 속성이기 때문이다.
    func openInPane(_ side: PaneSide, index: Int) {
        activeSide = side
        let model = side == .left ? leftPane : rightPane
        guard model.masterFiles.indices.contains(index) else { return }

        // 여러 개 고르는 중이면 열지 않고 선택만 토글한다.
        if model.multiSelect {
            model.toggleSelection(at: index)
            return
        }
        defaultAction(index: index, isDirectPath: false)
    }

    /// 길게 눌러 뜨는 메뉴. 기존 동작 시트(showSubView[1])를 그대로 쓴다.
    func openContextMenu(_ side: PaneSide, index: Int) {
        activeSide = side
        let model = side == .left ? leftPane : rightPane
        guard model.masterFiles.indices.contains(index) else { return }
        newViewFilePath = model.masterFiles[index].fullPath
        newViewFileName = model.masterFiles[index].name
        showSubView[1] = true
    }

    /// 활성 칸의 선택을 반대 칸으로 복사한다. 두 칸의 존재 이유다.
```

### `ContentView.swift (반대 칸으로)`  (42줄)

```swift
    /// 활성 칸의 선택을 반대 칸으로 복사한다. 두 칸의 존재 이유다.
    /// 쓰기는 반드시 FileGuard를 지난다 — 관문을 우회하면 안전장치가 무의미해진다.
    func copySelectionToOtherPane(move: Bool) {
        let sources = pane.selectedPaths.isEmpty
            ? (newViewFilePath.isEmpty ? [] : [newViewFilePath])
            : pane.selectedPaths
        guard !sources.isEmpty else {
            alertTitle = "복사할 항목이 없습니다"
            alertMsg = "먼저 항목을 고르십시오."
            showAlert = true
            return
        }

        // 큰 파일을 복사하는 동안 화면이 멈추면 안 된다. 뒷줄에서 돌리고 진행률만 올린다.
        let destination = otherPane.directory
        let destinationPane = otherPane
        let sourcePane = pane
        transfer.run(sources: sources,
                     destination: destination,
                     move: move,
                     onBlocked: { reason in
                        alertTitle = move ? "이동이 차단되었습니다" : "복사가 차단되었습니다"
                        alertMsg = reason
                        showAlert = true
                     },
                     onFinish: { done, failed in
                        destinationPane.updateFiles()
                        if move { sourcePane.updateFiles() }
                        sourcePane.multiSelect = false
                        if !failed.isEmpty {
                            alertTitle = "일부를 옮기지 못했습니다"
                            alertMsg = "\(done)개 완료, \(failed.count)개 실패\n" + failed.prefix(5).joined(separator: "\n")
                            showAlert = true
                        }
                     })
    }

    /// 마지막 이동·이름 변경을 되돌린다.
    func undoLastAction() {
        guard transfer.canUndo else {
            alertTitle = "되돌릴 작업이 없습니다"
            alertMsg = "이동과 이름 바꾸기만 되돌릴 수 있습니다."
```

---

## 4. 클립보드

### `Spartan/Panes/ArgoClipboard.swift`  (57줄)

```swift
//
//  ArgoClipboard.swift
//  Argo
//
//  복사·잘라내기·붙여넣기.
//
//  원래 복사는 이랬다: 길게 누르기 → 메뉴 → 복사 → 목적지 경로를 리모컨으로
//  한 글자씩 입력 → 확인. TV에서 쓸 수 있는 흐름이 아니다.
//
//  그래서 클립보드를 둔다. 고르고 ⌘C, 갈 곳으로 가서 ⌘V — 모든 파일
//  관리자가 쓰는 방식이고, 목적지를 타이핑할 일이 없다.
//
//  두 칸일 때는 '반대 칸으로' 한 번에 보내는 길이 따로 있다. 그쪽이 더
//  빠르지만 한 칸에서는 쓸 수 없으므로, 클립보드가 기본이고 저쪽이 지름길이다.
//

import Foundation
import SwiftUI

enum ClipboardMode {
    case copy
    case move

    var label: String {
        switch self {
        case .copy: return "복사"
        case .move: return "이동"
        }
    }
}

@available(tvOS 13.0, *)
final class ArgoClipboard: ObservableObject {

    @Published private(set) var paths: [String] = []
    @Published private(set) var mode: ClipboardMode = .copy

    var isEmpty: Bool { paths.isEmpty }
    var count: Int { paths.count }

    /// 붙여넣기 버튼에 쓸 설명. 몇 개가 무엇으로 들어 있는지 보여준다.
    var summary: String {
        guard !isEmpty else { return "" }
        let name = (paths[0] as NSString).lastPathComponent
        if paths.count == 1 { return "\(name) \(mode.label)" }
        return "\(name) 외 \(paths.count - 1)개 \(mode.label)"
    }

    func take(_ newPaths: [String], mode: ClipboardMode) {
        paths = newPaths
        self.mode = mode
    }

    func clear() {
        paths = []
    }
}
```

### `ContentView.swift (담기와 붙여넣기)`  (56줄)

```swift
    var currentTargets: [String] {
        let selected = pane.selectedPaths
        if !selected.isEmpty { return selected }
        return newViewFilePath.isEmpty ? [] : [newViewFilePath]
    }

    func clipboardTake(mode: ClipboardMode) {
        let targets = currentTargets
        guard !targets.isEmpty else {
            alertTitle = "고른 항목이 없습니다"
            alertMsg = "파일을 길게 눌러 고르거나 여러 개 고르기를 켜십시오."
            showAlert = true
            return
        }
        clipboard.take(targets, mode: mode)
        pane.multiSelect = false
        pane.resetMultiSelectArrays()
    }

    /// 지금 보고 있는 폴더에 붙여넣는다. 목적지를 입력할 일이 없다.
    func clipboardPaste() {
        guard !clipboard.isEmpty else { return }
        let sources = clipboard.paths
        let move = clipboard.mode == .move
        let destination = pane.directory
        let destinationPane = pane

        transfer.run(sources: sources,
                     destination: destination,
                     move: move,
                     onBlocked: { reason in
                        alertTitle = move ? "이동이 차단되었습니다" : "복사가 차단되었습니다"
                        alertMsg = reason
                        showAlert = true
                     },
                     onFinish: { done, failed in
                        destinationPane.updateFiles()
                        // 잘라내기는 원본이 사라졌으니 양쪽 다 다시 읽는다.
                        if move {
                            leftPane.updateFiles()
                            rightPane.updateFiles()
                            clipboard.clear()
                        }
                        if !failed.isEmpty {
                            alertTitle = "일부를 옮기지 못했습니다"
                            alertMsg = "\(done)개 완료, \(failed.count)개 실패\n" + failed.prefix(5).joined(separator: "\n")
                            showAlert = true
                        }
                     })
    }

    // MARK: - 두 칸 라우팅

    /// 칸에서 항목을 눌렀을 때. 활성 칸을 먼저 바꿔야 defaultAction이 올바른 칸을 본다.
    /// directory·masterFiles가 활성 칸을 가리키는 계산 속성이기 때문이다.
    func openInPane(_ side: PaneSide, index: Int) {
```

---

## 5. 전송 · 되돌리기

### `Spartan/Panes/PaneTransfer.swift`  (208줄)

```swift
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
```

---

## 6. 압축 — 뒷줄 처리

### `Spartan/FileOpeners/Compression/CompressionRunner.swift`  (103줄)

```swift
//
//  CompressionRunner.swift
//  Argo
//
//  압축과 해제를 뒷줄에서 돌린다.
//
//  전에는 버튼 액션 안에서 바로 돌았다. 10GB 파일이면 메인 스레드가 몇 분
//  막히고, 화면이 통째로 멈추며 리모컨도 안 먹는다. tvOS 가 응답 없는 앱을
//  죽이면 반쪽 아카이브가 남는다.
//
//  진행률은 없다. libarchive 와 minizip 을 거치는 호출이 중간에 아무것도
//  알려주지 않기 때문이다. 측정할 수 없는 것에 가짜 퍼센트를 붙이지 않는다 —
//  무엇을 하는 중인지만 말한다.
//
//  취소도 같은 이유로 '요청'까지만 한다. 아래로 내려간 C 호출을 중간에
//  끊을 방법이 없으므로, 취소는 여러 파일을 다룰 때 다음 파일로 넘어가지
//  않게 하는 것까지다. 할 수 있는 것과 없는 것을 그대로 말한다.
//

import Foundation
import SwiftUI

@available(tvOS 13.0, *)
final class CompressionRunner: ObservableObject {

    @Published var isRunning = false
    /// 지금 무엇을 하는 중인지. 사용자가 읽을 문장이다.
    @Published var message = ""
    /// 끝난 뒤의 오류. nil 이면 성공이다.
    @Published var failure: Error?
    /// 취소를 요청했는가. 이미 내려간 호출은 끊지 못한다.
    @Published private(set) var cancelRequested = false

    func requestCancel() {
        cancelRequested = true
        message = "현재 파일을 마치는 중입니다…"
    }

    /// 뒷줄에서 돌리고 끝나면 메인에서 알린다.
    /// onFinish 는 성공일 때만 불린다 — 실패는 failure 로 간다.
    func run(_ what: String,
             work: @escaping () throws -> Void,
             onFinish: @escaping () -> Void) {
        guard !isRunning else { return }
        isRunning = true
        cancelRequested = false
        failure = nil
        message = what

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            var thrown: Error?
            do {
                try work()
            } catch {
                thrown = error
            }
            DispatchQueue.main.async {
                guard let self = self else { return }
                self.isRunning = false
                self.message = ""
                if let thrown = thrown {
                    self.failure = thrown
                } else {
                    onFinish()
                }
            }
        }
    }
}

/// 작업 중 화면. 진행률 막대가 없는 이유는 위에 적었다.
@available(tvOS 13.0, *)
struct CompressionProgressView: View {
    @ObservedObject var runner: CompressionRunner
    let fileName: String

    var body: some View {
        VStack(spacing: 20) {
            ProgressView()
            Text(runner.message)
                .font(.title3)
            Text(fileName)
                .font(.caption)
                .foregroundColor(Theme.textSecondary)
                .lineLimit(1)
                .truncationMode(.middle)

            // 큰 파일은 몇 분이 걸린다. 멈춘 것이 아니라고 말해 둔다.
            Text("큰 파일은 몇 분이 걸릴 수 있습니다. 화면을 끄지 마십시오.")
                .font(.caption)
                .foregroundColor(Theme.textDisabled)
                .multilineTextAlignment(.center)

            if !runner.cancelRequested {
                Button(action: { runner.requestCancel() }) {
                    Text("취소")
                }
            }
        }
        .padding(40)
        .frame(maxWidth: 800)
    }
}
```

**볼 곳**

- 진행률 막대가 **없습니다.** libarchive·minizip 이 도는 중에 아무것도
  알려주지 않아 측정할 수 없고, 측정 못 하는 것에 가짜 퍼센트를 붙이지
  않는다는 판단입니다
- 취소가 "다음 파일로 넘어가지 않게 하는 것"까지만 하고, 화면에도 그렇게
  적혀 있습니다

---

## 7. 키보드

### `Spartan/Panes/ArgoKeyboard.swift`  (288줄)

```swift
//
//  ArgoKeyboard.swift
//  Argo
//
//  애플 블루투스 키보드 단축키.
//
//  확인된 사실 (CI로 검증함):
//  - UIKeyCommand는 tvOS에 있다. 리스폰더 체인으로 간다
//  - iOS 13의 UIKeyCommand(title:action:input:modifierFlags:)는 tvOS에 없다.
//    tvOS에서 그 자리는 image:·propertyList:를 더 요구한다.
//    UIKeyCommand(input:modifierFlags:action:)를 써야 한다
//  - 화살표 상수는 UIKeyCommand.inputUpArrow 다. UIKeyInputUpArrow는 Swift에서 이름이 바뀌었다
//
//  단축키는 지름길이지 유일한 길이 아니다. 키보드를 빼도 리모컨만으로 전부 되어야 한다.
//

import UIKit
import SwiftUI

/// 키보드에서 올라오는 명령. 알림으로 SwiftUI에 전달한다.
/// 리스폰더 체인에서 SwiftUI 상태로 바로 들어갈 길이 없어 알림을 끼운다.
enum ArgoCommand: String {
    case refresh
    case goUp
    case goBack
    case goForward
    case toggleDualPane
    case switchPane
    case clipboardCopy
    case clipboardCut
    case clipboardPaste
    case copyToOtherPane
    case moveToOtherPane
    case syncPanes
    case toggleMultiSelect
    case selectAll
    case deleteSelection
    case rename
    case fileInfo
    case newFolder
    case newFile
    case compress
    case extractArchive
    case toggleHidden
    case sortByName
    case sortBySize
    case sortByDate
    case sortByKind
    case reverseSort
    case focusPathField
    case search
    case showHelp
    case goHome
    case goRoot
    case undo

    static let notification = Notification.Name("ArgoKeyCommandNotification")

    /// 도움말 화면에 보여줄 설명. tvOS에는 메뉴 막대가 없어
    /// 이 화면이 없으면 단축키를 알 길이 자체가 없다.
    var title: String {
        switch self {
        case .refresh:           return "새로 읽기"
        case .goUp:              return "한 칸 위로"
        case .goBack:            return "뒤로"
        case .goForward:         return "앞으로"
        case .toggleDualPane:    return "두 칸 켜고 끄기"
        case .switchPane:        return "반대 칸으로"
        case .clipboardCopy:     return "복사"
        case .clipboardCut:      return "잘라내기"
        case .clipboardPaste:    return "붙여넣기 (이 폴더에)"
        case .copyToOtherPane:   return "반대 칸으로 바로 복사"
        case .moveToOtherPane:   return "반대 칸으로 바로 이동"
        case .syncPanes:         return "반대 칸을 이 폴더로"
        case .toggleMultiSelect: return "여러 개 고르기"
        case .selectAll:         return "전부 고르기"
        case .deleteSelection:   return "선택 삭제"
        case .rename:            return "이름 바꾸기"
        case .fileInfo:          return "정보 보기"
        case .newFolder:         return "새 폴더"
        case .newFile:           return "새 파일"
        case .compress:          return "압축"
        case .extractArchive:    return "압축 풀기"
        case .toggleHidden:      return "숨긴 항목 보기"
        case .sortByName:        return "이름순"
        case .sortBySize:        return "크기순"
        case .sortByDate:        return "날짜순"
        case .sortByKind:        return "종류순"
        case .reverseSort:       return "정렬 뒤집기"
        case .focusPathField:    return "경로 칸으로"
        case .search:            return "찾기"
        case .showHelp:          return "단축키 도움말"
        case .goHome:            return "미디어 폴더로"
        case .goRoot:            return "최상위로"
        case .undo:              return "되돌리기 (이동·이름)"
        }
    }

    /// 화면에 보여줄 키 조합. 실제 등록값과 같아야 한다.
    var keyLabel: String {
        let (input, flags) = Self.binding(for: self)
        var out = ""
        if flags.contains(.command)  { out += "⌘" }
        if flags.contains(.shift)    { out += "⇧" }
        if flags.contains(.alternate) { out += "⌥" }
        if flags.contains(.control)  { out += "⌃" }

        switch input {
        case UIKeyCommand.inputUpArrow:    out += "↑"
        case UIKeyCommand.inputDownArrow:  out += "↓"
        case UIKeyCommand.inputLeftArrow:  out += "←"
        case UIKeyCommand.inputRightArrow: out += "→"
        case "\r": out += "⏎"
        case "\u{8}": out += "⌫"
        case " ": out += "空白"
        default: out += input.uppercased()
        }
        return out
    }

    /// 키 하나당 조합 하나. 여기가 유일한 정의다.
    /// ⌘Space와 ⌘Tab은 시스템이 쓰므로 피한다.
    static func binding(for command: ArgoCommand) -> (String, UIKeyModifierFlags) {
        switch command {
        case .refresh:           return ("r", .command)
        case .goUp:              return (UIKeyCommand.inputUpArrow, .command)
        case .goBack:            return ("[", .command)
        case .goForward:         return ("]", .command)
        case .toggleDualPane:    return ("2", .command)
        // 맨 Tab 은 포커스 엔진이 쓰는 키다. 수정키 없이 가로채면
        // 칸 사이 이동이 깨질 수 있어 ⌥ 를 붙였다.
        case .switchPane:        return ("\t", .alternate)
        // 표준 자리를 표준 동작에 돌려준다. 전에는 ⌘C 가 '반대 칸으로 바로
        // 복사'였는데, 한 칸만 쓰는 사람에게는 복사할 길이 아예 없었다.
        case .clipboardCopy:     return ("c", .command)
        case .clipboardCut:      return ("x", .command)
        case .clipboardPaste:    return ("v", .command)
        // 두 칸을 쓸 때의 지름길. 클립보드를 거치지 않는다.
        case .copyToOtherPane:   return ("c", [.command, .alternate])
        case .moveToOtherPane:   return ("x", [.command, .alternate])
        case .syncPanes:         return ("=", .command)
        case .toggleMultiSelect: return ("m", .command)
        case .selectAll:         return ("a", .command)
        case .deleteSelection:   return ("\u{8}", .command)
        case .rename:            return ("\r", .command)
        case .fileInfo:          return ("i", .command)
        case .newFolder:         return ("n", [.command, .shift])
        case .newFile:           return ("n", .command)
        // ⌘Z는 모든 프로그램에서 되돌리기다. 압축은 ⌘⇧Z로 둔다.
        // 바꾸라는 지시가 오면 그때 바꾼다.
        case .compress:          return ("z", [.command, .shift])
        case .extractArchive:    return ("e", .command)
        case .toggleHidden:      return (".", [.command, .shift])
        case .sortByName:        return ("1", [.command, .alternate])
        case .sortBySize:        return ("2", [.command, .alternate])
        case .sortByDate:        return ("3", [.command, .alternate])
        case .sortByKind:        return ("4", [.command, .alternate])
        case .reverseSort:       return ("t", .command)
        case .focusPathField:    return ("l", .command)
        case .search:            return ("f", .command)
        case .showHelp:          return ("/", .command)
        case .goHome:            return ("h", [.command, .shift])
        case .goRoot:            return ("/", [.command, .shift])
        // ⌘Z는 모든 프로그램에서 되돌리기다. 압축은 ⌘⇧Z로 뒀다.
        case .undo:              return ("z", .command)
        }
    }

    /// 도움말 화면의 묶음.
    static let groups: [(String, [ArgoCommand])] = [
        ("이동", [.goUp, .goBack, .goForward, .goHome, .goRoot, .focusPathField, .refresh]),
        ("복사·이동", [.clipboardCopy, .clipboardCut, .clipboardPaste]),
        ("두 칸", [.toggleDualPane, .switchPane, .copyToOtherPane, .moveToOtherPane, .syncPanes]),
        ("고르기", [.toggleMultiSelect, .selectAll]),
        ("파일", [.newFolder, .newFile, .rename, .fileInfo, .deleteSelection, .compress, .extractArchive, .undo]),
        ("보기", [.toggleHidden, .sortByName, .sortBySize, .sortByDate, .sortByKind, .reverseSort, .search]),
        ("도움말", [.showHelp])
    ]
}

/// ContentView를 담는 호스팅 컨트롤러. 단축키는 여기서 리스폰더 체인에 낀다.
/// SwiftUI 뷰는 리스폰더가 아니라 keyCommands를 직접 가질 수 없다.
///
/// 제네릭이 아니다. @objc 선택자(#selector)를 쓰기 때문이다 —
/// 제네릭 클래스의 멤버는 Objective-C 런타임에 온전히 노출되지 않아,
/// UIKit이 그 선택자를 보낼 때 찾지 못할 수 있다. keyCommands는 실행 직후
/// 조회되므로 그 위험을 실행 경로 한가운데 두는 셈이 된다.
@available(tvOS 13.0, *)
final class ArgoHostingController: UIHostingController<ContentView> {

    /// 글자 입력 점프를 모으는 버퍼. 파일 300개 폴더에서 가장 큰 이득이다.
    private var typeAheadBuffer = ""
    private var typeAheadTimer: Timer?



    override var keyCommands: [UIKeyCommand]? {
        guard !ArgoTextEntry.isActive else { return nil }

        var out: [UIKeyCommand] = []
        for command in allCommands {
            let (input, flags) = ArgoCommand.binding(for: command)
            let key = UIKeyCommand(input: input,
                                   modifierFlags: flags,
                                   action: #selector(handleKeyCommand(_:)))
            // tvOS에는 메뉴 막대가 없지만 이것을 ⌘/ 도움말이 읽는다.
            key.discoverabilityTitle = command.title
            out.append(key)
        }
        return out
    }

    private var allCommands: [ArgoCommand] {
        ArgoCommand.groups.flatMap { $0.1 }
    }

    @objc private func handleKeyCommand(_ sender: UIKeyCommand) {
        guard let input = sender.input else { return }
        let flags = sender.modifierFlags
        guard let matched = allCommands.first(where: { command in
            let (i, f) = ArgoCommand.binding(for: command)
            return i == input && f == flags
        }) else { return }

        NotificationCenter.default.post(name: ArgoCommand.notification,
                                        object: matched.rawValue)
    }

    // MARK: - 글자 입력 점프

    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        guard !ArgoTextEntry.isActive else {
            super.pressesBegan(presses, with: event)
            return
        }

        var handled = false
        for press in presses {
            guard let key = press.key else { continue }
            let chars = key.charactersIgnoringModifiers
            // 수정키가 걸린 입력은 단축키 쪽이 처리한다.
            guard key.modifierFlags.isEmpty, chars.count == 1,
                  let scalar = chars.unicodeScalars.first,
                  CharacterSet.alphanumerics.contains(scalar) else { continue }

            typeAheadBuffer += chars
            handled = true
            restartTypeAheadTimer()
        }

        if handled {
            NotificationCenter.default.post(name: ArgoTypeAhead.notification,
                                            object: typeAheadBuffer)
        } else {
            super.pressesBegan(presses, with: event)
        }
    }

    /// 0.8초 쉬면 버퍼를 비운다. 'ab' 치고 쉬었다가 'c' 치면 'c'부터 찾는다.
    private func restartTypeAheadTimer() {
        typeAheadTimer?.invalidate()
        typeAheadTimer = Timer.scheduledTimer(withTimeInterval: 0.8, repeats: false) { [weak self] _ in
            self?.typeAheadBuffer = ""
        }
    }
}

enum ArgoTypeAhead {
    static let notification = Notification.Name("ArgoTypeAheadNotification")
}

/// 지금 글자를 받는 칸이 떠 있는가. 제네릭이 아닌 타입에 둔다 —
/// 호스팅 컨트롤러에 두면 호출마다 ArgoHostingController<ContentView> 를
/// 적어야 한다.
///
/// UserDefaults 에 두었던 것을 메모리로 옮겼다. 이것은 지금 이 순간의
/// 상태이지 설정이 아니다. 저장소에 두면 입력 중에 앱이 죽었을 때
/// 다음 실행에도 단축키가 죽은 채로 남는다.
enum ArgoTextEntry {
    private static var depth = 0

    static var isActive: Bool { depth > 0 }

    /// 글자를 받는 칸이 떠오를 때. 겹쳐도 되도록 깊이로 센다 —
    /// 경로 칸에 포커스가 있는 채로 찾기 시트가 뜰 수 있다.
    static func begin() { depth += 1 }
    static func end()   { depth = max(0, depth - 1) }
}
```

**볼 곳**

- `UIKeyCommand(input:modifierFlags:action:)` 를 씁니다.
  **iOS 13 의 `(title:action:input:modifierFlags:)` 는 tvOS 에 없습니다**
- 맨 `Tab` 은 포커스 엔진이 쓰므로 칸 전환을 `⌥Tab` 으로 옮겼습니다
- `ArgoTextEntry` 는 깊이로 셉니다 — 경로 칸에 포커스가 있는 채로 시트가
  뜰 수 있어, 참·거짓으로 두면 시트를 닫을 때 먼저 켜 둔 것까지 풀립니다

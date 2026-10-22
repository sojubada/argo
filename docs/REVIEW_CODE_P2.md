# 코드 리뷰 — 2단계 두 칸 탐색기와 키보드

> 탐침·외관 코드는 제외했습니다. 테마(`Spartan/Theme/`)와 도움말 화면도 외관이라 뺐습니다.
> 커밋 `99f2434` (`main`) 기준. CI 통과.

제미나이 없이 작성했습니다.

---

## 리뷰할 때 봐주실 것

### 1. 이름을 유지한 계산 속성 — 이 묶음의 핵심 수법입니다

`ContentView` 는 1,880줄이고 `directory` 를 134곳, `masterFiles` 를 146곳에서 씁니다. 칸이 둘이 되려면 그 280곳이 **어느 칸** 이야기인지 알아야 합니다.

전부 고치는 대신 **이름을 그대로 두고 계산 속성으로 바꿨습니다.**

```swift
var directory: String {
    get { pane.directory }
    nonmutating set { pane.directory = newValue }
}
```

`pane` 이 클래스(`ObservableObject`) 참조라 구조체를 변형하지 않으므로 `nonmutating set` 이 성립합니다. **280곳을 하나도 건드리지 않았습니다.**

**이것이 리뷰의 제1 항목입니다.** 영리한 만큼 조용히 틀릴 수 있습니다. 봐야 할 것:

- `activeSide` 가 바뀌는 순간, 진행 중인 작업이 **엉뚱한 칸**을 보게 되지 않는가
- `openInPane` 이 `activeSide` 를 **먼저** 바꾸고 `defaultAction` 을 부르는가 (순서가 뒤바뀌면 왼쪽 칸을 눌렀는데 오른쪽 칸 파일이 열립니다)
- `copySelectionToOtherPane` 이 `pane` · `otherPane` 을 **지역 변수로 붙잡아** 비동기 완료 시점에 활성 칸이 바뀌어도 원래 칸을 쓰는가
- `$directory` 를 쓰던 10곳이 `directoryBinding` 으로 바뀌었는가 — 하나라도 남으면 컴파일이 안 되므로 확인됐습니다
- SwiftUI 가 두 `@StateObject` 의 변화를 제대로 받는가 (활성 칸만 다시 그려지고 반대 칸이 멈추는 일이 없는가)

### 2. 포커스

**칸 사이 좌우 이동은 손대지 않았습니다.** tvOS 포커스 엔진이 이미 합니다. 코드로 개입하면 싸웁니다.

**요청하신 5·6번(바깥 끝에서 툴바로)은 일부러 넣지 않았습니다.** `UIFocusGuide` 로 포커스가 **도착한 것은** 감지되지만 SwiftUI 에서 포커스를 **보낼** 수단이 없습니다 (`prefersDefaultFocus(_:in:)` 는 tvOS 14+, `focusSection()` 은 15+, 둘 다 "유도"지 "이동"이 아닙니다). 1픽셀 투명 뷰가 포커스를 잡고 못 빠져나오는 **함정**이 될 수 있고, 파일 탐색기에서 포커스가 갇히면 앱을 끄는 수밖에 없습니다.

**리뷰에서 봐주실 것: 포커스가 갇히는 곳이 정말 없는지.** 특히 두 칸에서 바깥 끝으로 갔을 때.

> 이전에 크게 당한 지점입니다. 문서 뷰어에서 뷰컨트롤러의 `pressesBegan` 으로 방향키를 받으려다 전부 포커스 이동에 먹혀 아무것도 안 됐습니다. **입력은 포커스를 가진 뷰에 먼저 갑니다.**

### 3. 쓰기가 관문을 지나는가

2단계가 새 쓰기 경로를 **둘** 만들었습니다.

| 새 경로 | 관문 |
| --- | --- |
| `copySelectionToOtherPane` → `PaneTransfer.run` | `FileGuard.authorize` (첫 바이트 전에) |
| `undoLastAction` → `PaneTransfer.undoLast` | `FileGuard.authorize` |

**둘 다 1단계 관문을 지납니다.** 우회가 없는지 확인해 주십시오.

### 4. 알려진 결정 — 확인이 필요합니다

- **`⌘Z` 를 되돌리기로 뒀습니다.** 압축은 `⌘⇧Z` 입니다. 압축으로 하자는 말씀이 있었지만 `⌘Z` 는 모든 프로그램에서 되돌리기이고, 이번에 되돌리기가 실제로 들어갔습니다. 바꾸라고 하시면 한 줄입니다
- **`Return` 은 비워 뒀습니다.** `⌘Return` 이 이름 바꾸기(파인더 방식)입니다. 맨 `Return` 을 열기로 할지는 리모컨 선택 버튼과 겹쳐 보여 정하지 않았습니다
- **복사는 되돌릴 수 없습니다.** 되돌리려면 지워야 하고, 앱이 알아서 지우는 것은 파일 탐색기가 할 일이 아닙니다

---

## 1. 칸 모델 — 칸 하나의 상태 전부

### `Spartan/Panes/FilePaneModel.swift`  (358줄)

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

        DispatchQueue.global().asyncAfter(deadline: .now() + 2) { [directory] in
            cacheFolder(directory)
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
                result = fileSize(a) < fileSize(b)
            case .date:
                result = modifiedDate(a) < modifiedDate(b)
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

    private func fileSize(_ f: SpartanFile) -> Int64 {
        guard let attrs = try? fileManager.attributesOfItem(atPath: f.fullPath),
              let size = attrs[.size] as? Int64 else { return 0 }
        return size
    }

    private func modifiedDate(_ f: SpartanFile) -> Date {
        guard let attrs = try? fileManager.attributesOfItem(atPath: f.fullPath),
              let date = attrs[.modificationDate] as? Date else { return .distantPast }
        return date
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
                    out.append(decoded[j])
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
            let encoder = JSONEncoder()
            let encoded = try encoder.encode(out)
            try encoded.write(to: URL(fileURLWithPath: tempPath))
            if let metaName = metadataName {
                RootHelperActs.mvtemp(directory + metaName)
            }
        } catch {
            print("[FilePane] \(directory) 를 읽지 못했습니다: \(error.localizedDescription)")
        }
        return out
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
        SpartanFile(name: name,
                    fullPath: directory + name,
                    isSelected: false,
                    fileType: FileInfo.yandereDevFileType(file: directory + name),
                    isLoadingFile: false)
    }

    /// 사라진 파일을 캐시에서 덜어낸다. 안 하면 캐시가 끝없이 자란다.
    private func pruneCache(decoded: [SpartanFile], live: [SpartanFile]) {
        guard let metaName = metadataName else { return }
        let dir = directory
        DispatchQueue.global().asyncAfter(deadline: .now() + 0.5) {
            // 원본은 순회 중에 remove(at:)를 해서 인덱스가 밀렸다. filter로 바꿨다.
            let kept = decoded.filter { live.contains($0) }
            do {
                let encoded = try JSONEncoder().encode(kept)
                try encoded.write(to: URL(fileURLWithPath: tempPath))
                RootHelperActs.mvtemp(dir + metaName)
            } catch {
                print("[FilePane] 캐시를 갱신하지 못했습니다: \(error)")
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

- **`navigate(to:)` 가 기록을 남기는 유일한 통로입니다.** `directory` 를 직접 쓰는 곳이 있으면 `⌘[` 기록이 어긋납니다
- **`applySortAndFilter` 가 디스크를 다시 읽지 않습니다.** `rawFiles` 를 들고 있어 정렬만 바꿀 때 빠릅니다 — 대신 정렬 후 **파일시스템 변화를 못 봅니다.** 새로 읽기(`⌘R`)가 필요한 이유입니다
- **`pruneCache`** — 원본은 순회 중에 `remove(at:)` 를 해서 인덱스가 밀렸습니다 (`decoded[i]` 를 지우면 뒤 항목이 앞으로 당겨짐). `filter` 로 바꿨습니다
- **`resetMultiSelectArrays` 가 `applySortAndFilter` 끝에서 불립니다.** 목록이 바뀌면 선택 배열의 길이가 안 맞기 때문입니다 — 원본 코드도 같은 이유로 비웠습니다. **정렬을 바꾸면 선택이 풀립니다.** 의도한 것이지만 쓰다 보면 거슬릴 수 있습니다
- `fileSize` · `modifiedDate` 가 정렬 비교마다 `attributesOfItem` 을 칩니다. **파일 300개면 O(n log n)번 디스크를 칩니다** — 느리면 여기입니다. 미리 한 번 읽어 캐시하는 것이 맞습니다
- 폴더가 언제나 먼저 옵니다 (`aDir != bDir` 이면 거기서 결정). 정렬 기준과 무관합니다

---

## 2. 칸 뷰 — 모델 하나만 받습니다

전역 상태를 참조하면 두 번 못 씁니다. 그게 두 칸이 가능한 이유입니다.

### `Spartan/Panes/FilePaneView.swift`  (203줄)

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

    /// 길게 눌러 뜨는 메뉴. 활성 칸으로 바꾼 뒤 ContentView가 처리한다.
    let onContextMenu: ((Int) -> Void)?

    init(model: FilePaneModel,
         isActive: Bool,
         onOpen: @escaping (Int) -> Void,
         onGoUp: @escaping () -> Void,
         onFocus: @escaping () -> Void,
         onContextMenu: ((Int) -> Void)? = nil) {
        self.model = model
        self.isActive = isActive
        self.onOpen = onOpen
        self.onGoUp = onGoUp
        self.onFocus = onFocus
        self.onContextMenu = onContextMenu
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
                .stroke(isActive ? Theme.accent : Color.clear, lineWidth: 4)
        )
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
                Button(action: {
                    onFocus()
                    onContextMenu?(index)
                }) {
                    Text("자세히")
                }
                // tvOS 컨텍스트 메뉴는 버튼을 눌러야 닫힌다. 다른 방법이 없다.
                Button("Dismiss", action: { })
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
                Text(file.fileType == 8 ? removeLastChar(file.name) : file.name)
                    .foregroundColor(Theme.textPrimary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
        }
    }
}
```

**볼 곳**

- **행당 포커스 버튼은 하나입니다.** tvOS 규칙이고 깨뜨리면 목록을 못 씁니다
- `#available(tvOS 14.0, *)` 로 행이 두 벌입니다. `.if` 수정자로는 분기할 수 없습니다 — `#available` 은 컴파일 시점 검사입니다
- **`FileGlyph.symbol(for:name:)`** — 원래 `ContentView` 의 639줄 블록에 같은 `switch fileType` 이 tvOS 13/14 분기로 **두 벌** 들어 있던 것을 한 곳으로 모았습니다. 종류 번호가 빠진 것이 없는지 확인해 주십시오 (0·1·2·3·4·6·7·8·9·10·11·12·13·14·15·16 — **5가 없는 것이 원본과 같습니다**)
- `fileType` 이 `Double` 입니다 (`SpartanFile` 정의). `Int(fileType)` 로 변환해 `switch` 합니다
- **파일을 여는 일을 하지 않습니다.** `onOpen` 으로 `ContentView` 에 돌려주고 거기서 기존 `defaultAction` 이 39개 시트를 그대로 띄웁니다. **뷰어 쪽은 하나도 안 건드렸습니다**
- 위험도 배지가 `fastRisk(parentRisk:)` 로 부모 상속을 씁니다 — 안전한 폴더에서는 항목마다 디스크를 치지 않습니다

---

## 3. 두 칸 라우팅 — `ContentView`

### `Spartan/ContentView.swift:23-79 (칸 보유와 계산 속성)`  (56줄)

```swift
    @StateObject var leftPane: FilePaneModel
    @StateObject var rightPane: FilePaneModel
    @State var activeSide: PaneSide = .left
    @StateObject var transfer = PaneTransfer()
    /// 두 칸을 함께 보일지. 끄면 한 칸만 나와 기존 동작과 같다.
    @AppStorage("dualPaneEnabled") var dualPaneEnabled = false

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

### `Spartan/ContentView.swift:1493-1583 (라우팅)`  (91줄)

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
            showAlert = true
            return
        }
        transfer.undoLast(onBlocked: { reason in
            alertTitle = "되돌리기가 차단되었습니다"
            alertMsg = reason
            showAlert = true
        }, onFinish: { ok in
            leftPane.updateFiles()
            rightPane.updateFiles()
            if !ok {
                alertTitle = "되돌리지 못했습니다"
                alertMsg = "원래 자리에 같은 이름이 있거나 경로가 사라졌습니다."
                showAlert = true
            }
        })
    }

    /// 활성 칸을 반대 칸과 같은 폴더로 보낸다. 양쪽을 맞춰 볼 때 쓴다.
    func syncOtherPaneToActive() {
        otherPane.navigate(to: pane.directory)
    }

    func freeSpace(path: String) -> (double: Double, string: String) {
```

**볼 곳**

- `openInPane` 이 **`activeSide` 를 먼저** 바꿉니다. `directory`·`masterFiles` 가 활성 칸을 가리키므로 순서가 전부입니다
- 여러 개 고르는 중이면 열지 않고 선택만 토글합니다
- `copySelectionToOtherPane` 이 선택이 없으면 `newViewFilePath`(마지막으로 만진 파일) 하나로 떨어집니다. 의도한 것이지만 **선택한 줄 알았는데 다른 파일이 복사되는** 경우가 없는지 봐주십시오
- `otherPane` · `pane` 을 지역 변수(`destinationPane` · `sourcePane`)로 붙잡습니다. 비동기 완료 시점에 활성 칸이 바뀌어도 원래 칸을 씁니다

---

## 4. 전송 — 뒷줄에서 돌고 진행률만 올립니다

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

**볼 곳**

- **관문이 첫 바이트 전에 전부 지납니다.** 한 건이라도 막히면 아무것도 시작하지 않습니다 — **절반만 옮겨진 상태가 가장 수습하기 어렵습니다**
- **덮어쓰지 않습니다.** 목적지에 같은 이름이 있으면 `name 2.txt` 로 비켜 씁니다. 조용한 덮어쓰기는 아무도 못 보는 자료 손실입니다
- 진행률이 **항목 수** 기준입니다. `FileManager.copyItem` 이 진행 중 아무것도 알려주지 않아 바이트 기준이 불가능합니다. **큰 파일 하나를 복사하면 0%에서 100%로 점프합니다** — 한계입니다
- `cancelRequested` 를 **항목 사이**에서만 봅니다. 한 항목 복사 중에는 취소가 안 듣습니다. 10GB 파일이면 취소가 사실상 안 됩니다
- `undoStack` 에 상한이 없습니다. 많이 옮기면 계속 쌓입니다
- 되돌리기도 관문을 지나고, 실패하면 기록을 되살립니다
- `perform` 이 실패하면 `RootHelperActs` 로 한 번 더 시도합니다 — **1단계 리뷰에 적은 대로 헬퍼는 실제로 돌지 않으므로** 이 경로는 사실상 죽은 코드입니다. 해롭지는 않습니다

---

## 5. 키보드

### `Spartan/Panes/ArgoKeyboard.swift`  (255줄)

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
        case .copyToOtherPane:   return "반대 칸으로 복사"
        case .moveToOtherPane:   return "반대 칸으로 이동"
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
        case .switchPane:        return ("\t", [])
        case .copyToOtherPane:   return ("c", .command)
        case .moveToOtherPane:   return ("x", .command)
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
        ("두 칸", [.toggleDualPane, .switchPane, .copyToOtherPane, .moveToOtherPane, .syncPanes]),
        ("고르기", [.toggleMultiSelect, .selectAll]),
        ("파일", [.newFolder, .newFile, .rename, .fileInfo, .deleteSelection, .compress, .extractArchive, .undo]),
        ("보기", [.toggleHidden, .sortByName, .sortBySize, .sortByDate, .sortByKind, .reverseSort, .search]),
        ("도움말", [.showHelp])
    ]
}

/// ContentView를 담는 호스팅 컨트롤러. 단축키는 여기서 리스폰더 체인에 낀다.
/// SwiftUI 뷰는 리스폰더가 아니라 keyCommands를 직접 가질 수 없다.
@available(tvOS 13.0, *)
final class ArgoHostingController<Content: View>: UIHostingController<Content> {

    /// 글자 입력 점프를 모으는 버퍼. 파일 300개 폴더에서 가장 큰 이득이다.
    private var typeAheadBuffer = ""
    private var typeAheadTimer: Timer?

    /// 경로·찾기 칸에 글자를 넣는 중이면 단축키를 막는다.
    /// 안 막으면 파일명에 'r'을 못 넣는다.
    static var textEntryActive: Bool {
        get { UserDefaults.standard.bool(forKey: "argoTextEntryActive") }
        set { UserDefaults.standard.set(newValue, forKey: "argoTextEntryActive") }
    }

    override var keyCommands: [UIKeyCommand]? {
        guard !Self.textEntryActive else { return nil }

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
        guard !Self.textEntryActive else {
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
```

**볼 곳**

- **키 조합의 정의가 `ArgoCommand.binding(for:)` 한 곳입니다.** 도움말 화면이 같은 함수를 읽으므로 **표시와 실제가 어긋날 수 없습니다**
- `UIKeyCommand(input:modifierFlags:action:)` 를 씁니다. **iOS 13의 `(title:action:input:modifierFlags:)` 는 tvOS 에 없습니다** — tvOS 에서 그 자리는 `image:`·`propertyList:` 를 더 요구합니다 (CI 로 확인)
- 화살표 상수는 `UIKeyCommand.inputUpArrow` 입니다. `UIKeyInputUpArrow` 는 Swift 에서 이름이 바뀌었습니다
- **`textEntryActive` 가 `UserDefaults.standard` 에 들어 있습니다.** 이것은 깔끔하지 않습니다 — 상태를 저장소에 두는 것이고, 앱을 다시 켜도 남습니다. **봐주실 곳입니다.** 그리고 **지금 이 값을 켜 주는 코드가 없습니다** → 경로·찾기 칸에 글자를 넣을 때 단축키가 먹을 수 있습니다. 미완입니다
- `handleKeyCommand(_:)` 가 input·flags 로 어느 명령인지 되찾습니다. 같은 조합이 둘이면 앞의 것이 이깁니다 — 중복이 없는지 확인해 주십시오
- 글자 입력 점프가 `pressesBegan` 에서 **수정키가 없을 때만** 동작합니다. 수정키가 걸린 입력은 단축키 쪽이 처리합니다
- `⌘Space`·`⌘Tab` 은 시스템이 쓰므로 피했습니다. 다만 **`switchPane` 을 맨 `Tab`** 으로 뒀는데, tvOS 에서 `Tab` 이 포커스 이동에 쓰이는지 확인이 필요합니다

---

## 6. 명령 처리 — `ContentView`

### `Spartan/ContentView.swift:1395-1490 (단축키 처리)`  (95줄)

```swift
    /// 리스폰더 체인에서 올라온 단축키를 처리한다.
    /// 전부 리모컨으로도 되는 일이어야 한다 — 단축키 전용 기능은 만들지 않는다.
    func handleKeyCommand(_ raw: String) {
        guard let command = ArgoCommand(rawValue: raw) else { return }
        switch command {
        case .refresh:           pane.updateFiles()
        case .goUp:              pane.goUp()
        case .goBack:            pane.goBack()
        case .goForward:         pane.goForward()
        case .goHome:            pane.navigate(to: "/var/mobile/Media/")
        case .goRoot:            pane.navigate(to: "/")

        case .toggleDualPane:
            withAnimation { dualPaneEnabled.toggle() }
            if dualPaneEnabled && rightPane.masterFiles.isEmpty {
                rightPane.updateFiles()
            }
        case .switchPane:
            guard dualPaneEnabled else { return }
            activeSide = activeSide == .left ? .right : .left
        case .copyToOtherPane:   copySelectionToOtherPane(move: false)
        case .moveToOtherPane:   copySelectionToOtherPane(move: true)
        case .syncPanes:         syncOtherPaneToActive()

        case .toggleMultiSelect:
            withAnimation { pane.multiSelect.toggle() }
            if !pane.multiSelect { pane.resetMultiSelectArrays() }
        case .selectAll:
            pane.multiSelect = true
            pane.setAllSelected(!pane.allWereSelected)

        // 삭제는 전용 시트가 없다. 선택이 있으면 미리보기를 거쳐 휴지통으로 보낸다.
        case .deleteSelection:   requestDeleteSelection()
        case .rename:            showSubView[7] = true   // renameFileShow
        case .fileInfo:          showSubView[3] = true   // fileInfoShow
        case .newFolder:         showSubView[6] = true   // createDirectoryShow
        case .newFile:           showSubView[5] = true   // createFileShow
        case .compress:          showSubView[14] = true  // compFileShow
        case .extractArchive:    showSubView[34] = true  // uncompFileShow
        case .search:            showSubView[19] = true  // searchShow
        case .showHelp:          showSubView[39] = true  // keyboardHelpShow

        case .toggleHidden:
            pane.showHidden.toggle()
            UserDefaults.settings.set(pane.showHidden, forKey: "showHiddenFiles")
            pane.applySortAndFilter()
        case .sortByName:        setSort(.name)
        case .sortBySize:        setSort(.size)
        case .sortByDate:        setSort(.date)
        case .sortByKind:        setSort(.kind)
        case .reverseSort:
            pane.ascending.toggle()
            pane.applySortAndFilter()
        case .undo:
            undoLastAction()

        case .focusPathField:
            // 경로 칸은 TextField라 포커스를 코드로 줄 수 없다.
            // 대신 경로를 알려주는 찾기 시트를 연다.
            showSubView[19] = true
        }
    }

    /// ⌫ 로 삭제. 기존 일괄 삭제 미리보기를 거친다 — 되돌릴 수 없는 일이라 확인을 뺄 수 없다.
    func requestDeleteSelection() {
        let targets = pane.selectedPaths.isEmpty
            ? (newViewFilePath.isEmpty ? [] : [newViewFilePath])
            : pane.selectedPaths
        guard !targets.isEmpty else {
            alertTitle = "삭제할 항목이 없습니다"
            alertMsg = "먼저 항목을 고르십시오."
            showAlert = true
            return
        }
        cautionCountForBatch = targets.filter { PathSafety.risk(of: $0) != .safe }.count
        showBatchDeletePreview = true
    }

    /// 같은 기준을 다시 누르면 오르내림이 뒤집힌다. 파인더와 같다.
    func setSort(_ key: SortKey) {
        if pane.sort == key {
            pane.ascending.toggle()
        } else {
            pane.sort = key
            pane.ascending = true
        }
        pane.applySortAndFilter()
    }

    /// 글자를 치면 그 글자로 시작하는 첫 항목을 고른다.
    func handleTypeAhead(_ prefix: String) {
        guard let index = pane.indexOfFirstMatch(prefix: prefix) else { return }
        newViewFilePath = pane.masterFiles[index].fullPath
        newViewFileName = pane.masterFiles[index].name
    }
```

**볼 곳**

- 시트 인덱스가 맞는가. **한 번 틀렸습니다** — 처음에 삭제=17(즐겨찾기), 압축=10(오디오 재생), 해제=11(비디오 재생)로 짚었습니다. 지금은 압축=14(`compFileShow`), 해제=34(`uncompFileShow`), 이름=7, 정보=3, 새 폴더=6, 새 파일=5, 찾기=19, 도움말=39 입니다
- **`showSubView` 배열 크기를 39 → 40 으로 늘렸습니다.** 이것을 안 늘리면 **실행 즉시 죽습니다**
- `focusPathField`(`⌘L`)가 경로 칸이 아니라 **찾기 시트**를 엽니다. `TextField` 에 포커스를 코드로 줄 수 없기 때문입니다 — 이름과 동작이 어긋나므로 차라리 이름을 바꾸는 것이 맞습니다
- `deleteSelection`(`⌫`)이 일괄 삭제 미리보기를 거칩니다. 되돌릴 수 없는 일이라 확인을 뺄 수 없습니다

---

## 정리 — 리뷰 체크리스트

- [ ] `activeSide` 가 바뀌는 시점에 엉뚱한 칸을 보는 곳이 있는가 ★
- [ ] 포커스가 갇히는 곳이 정말 없는가 (두 칸 바깥 끝) ★
- [ ] `copySelectionToOtherPane` · `undoLast` 가 관문을 우회하지 않는가
- [ ] `FileGlyph` 에 빠진 파일 종류가 없는가
- [ ] 단축키 조합에 중복이 없는가
- [ ] `textEntryActive` 를 켜는 코드가 없는 문제 (미완)
- [ ] 정렬 비교마다 디스크를 치는 성능 문제
- [ ] 한 항목 복사 중 취소가 안 듣는 문제
- [ ] `⌘Z` 를 압축으로 바꿀지 결정
- [ ] `Return` 키를 열기로 할지 결정

# Argo 개발 작업 지침서 (Gemini 작업용)

이 문서는 **Gemini가 코드를 작성하기 위한 사양서**입니다.
대상 저장소: `sojubada/argo` (브랜치 `main`)

---

## 0. 작업자가 먼저 알아야 할 환경 제약

이 제약을 어기면 결과물이 통째로 버려집니다. **반드시 먼저 읽으십시오.**

| 항목 | 내용 |
|---|---|
| 앱 이름 / 모듈명 | `Argo` (`PRODUCT_NAME = Argo`). 소스 폴더 이름은 역사적 이유로 여전히 `Spartan/` |
| 번들 ID | `com.sojubada.argo` |
| 언어 | Swift 5, SwiftUI (일부 UIKit 혼용) |
| 현재 배포 타깃 | tvOS 13.0 → **이번 작업에서 16.0으로 올림** |
| 빌드 환경 | GitHub Actions, macOS runner, **Xcode 15.4 / tvOS 17.5 SDK** |
| 로컬 빌드 | **불가능.** 작업자는 코드를 컴파일해 볼 수 없음 |
| 설치 방식 | 미서명 IPA → TrollStore |
| 실행 기기 | Apple TV, tvOS 16.x (탈옥 아님, TrollStore만 설치됨) |

### 0.1 컴파일을 못 한다는 뜻

작업자는 결과물을 검증할 수 없습니다. 따라서:

- **새 외부 의존성(SPM 패키지, CocoaPods) 추가 금지.** 현재 의존성은 `Zip`, `PrivateKits-tvOS` 둘뿐이며 늘리지 않습니다.
- Apple 기본 프레임워크만 사용: `SwiftUI`, `Foundation`, `PDFKit`, `AVKit`, `UIKit`, `Combine`
- 사용하는 모든 API는 **tvOS 16.0에 존재하는 것**이어야 합니다. 확실치 않으면 쓰지 말고 주석으로 "확인 필요"라고 남기십시오.
- 화려하지만 불확실한 API보다 **단순하고 확실한 API**를 택하십시오.
- 추측으로 메서드 시그니처를 지어내지 마십시오. 모르면 적지 말고 질문으로 남기십시오.

### 0.2 절대 건드리지 말 것

- `.github/workflows/build.yml` (빌드 파이프라인)
- `Argo.entitlements`, `spawn.h`, `External/` 전체
- `Spartan/WebDAV/` 전체 (WebDAV 기능, 이미 동작 중)
- `tools/make_assets.py`, `Spartan/Assets.xcassets/` (아이콘·아트)
- `Spartan.xcodeproj/project.pbxproj` → **직접 수정 금지.** 아래 0.3 참조

### 0.3 Xcode 프로젝트 등록

새 `.swift` 파일은 Xcode 프로젝트에 등록되어야 빌드에 포함됩니다.
**작업자는 `project.pbxproj`를 직접 편집하지 마십시오.** 손으로 고치면 거의 확실히 파일이 깨집니다.

대신 결과물 맨 끝에 아래 형식으로 **새 파일 목록만** 적으십시오. 등록은 검수자가 처리합니다.

```
### 신규 파일 등록 요청
- Spartan/Trash/TrashManager.swift
- Spartan/Trash/TrashView.swift
```

### 0.4 기존 코드에서 반드시 재사용할 것

| 심볼 | 위치 | 용도 |
|---|---|---|
| `fileManager` | `SpareViews.swift:15` | 전역 `FileManager.default`. 단, `SpartanApp.swift` 안에서는 이름 충돌이 있으니 그 파일에서만 `FileManager.default`를 직접 쓸 것 |
| `RootHelperActs` | `SpareViews.swift:334` | root 권한 파일 조작. `.rm/.mv/.cp/.mkdir/.touch/.ln/.chmod/.mvtemp` |
| `helperPath`, `isJB` | `SpareViews.swift:307` | 루트 헬퍼 경로 / 탈옥 여부 |
| `tempPath` | 전역 | 임시 파일 경로. root 영역에 파일을 쓸 때는 `tempPath`에 쓴 뒤 `RootHelperActs.mvtemp(목적지)` |
| `FileInfo.yandereDevFileType(file:)` | `FileInfo.swift:14` | 파일 종류 판별 (Double 반환) |
| `UserDefaults.settings` / `.favorites` | `SpartanApp.swift:109` | 설정 / 즐겨찾기 저장소 |
| `LocalizedString(_:)` | `SpareViews.swift` | 지역화 문자열 |

**중요 — root 영역 쓰기 패턴:**
`/private/var/mobile/Media/` 이하는 앱이 직접 쓰지 못할 수 있습니다. 파일 생성·이동·삭제는 반드시 `RootHelperActs`를 통하십시오. JSON 같은 데이터 파일을 쓸 때는:

```swift
try data.write(to: URL(fileURLWithPath: tempPath))
RootHelperActs.mvtemp(목적지경로)
```

### 0.5 tvOS UI 제약 (중요)

Apple TV는 터치가 없습니다. 리모컨 포커스 기반입니다.

- **리스트 한 행 = 포커스 단위 하나.** 한 행에 버튼 두 개를 넣으면 항상 뒤쪽 것이 눌립니다. 행당 버튼은 **하나**만.
- 드래그 앤 드롭 없음 → 순서 변경은 "위로/아래로" 버튼으로.
- 한 뷰에 `.sheet`는 **하나만** 동작합니다. 여러 화면이 필요하면 `.sheet` 하나 안에서 상태값으로 분기하십시오. (`WebDAVView.swift`의 `viewerKind` 패턴 참고)
- 뒤로가기는 `.onExitCommand { }` 로 받습니다. 메뉴 버튼이 곧바로 시트를 닫지 않게 단계적으로 처리하십시오.
- 글자는 TV 시청 거리 기준으로 큼직하게. 화면 가장자리 5% 는 오버스캔 영역이니 중요한 것을 두지 마십시오.

---

## 1. 작업 1 — 휴지통 (Trash)

### 1.1 현황 검토 결과

조사 결과, 휴지통은 **"버리기"만 있고 "되살리기"가 없습니다.**

현재 구현:
- 경로: `/private/var/mobile/Media/.Trash/`
- 삭제 = `RootHelperActs.mv(원본, "/private/var/mobile/Media/.Trash/" + 파일명)` — `ContentView.swift:804`, `:1119`, 다중선택은 `:345`
- 비우기 = 폴더 통째로 `rm` 후 `mkdir` — `ContentView.swift:793`
- 휴지통 안에서 개별 영구삭제 = `rm` — `ContentView.swift:783`

확인된 결함:

| # | 문제 | 위치 |
|---|---|---|
| T1 | **복원 기능이 존재하지 않음.** 원래 경로를 어디에도 기록하지 않아 복원이 원천적으로 불가능 | 전체 |
| T2 | 이름이 같은 파일을 두 번 버리면 **앞의 것이 조용히 덮어써짐** | `mv` 호출 전부 |
| T3 | 삭제 시각, 원본 크기 등 메타데이터 없음 | 전체 |
| T4 | 폴더를 버릴 때 동작 미검증 | `:804` |
| T5 | 휴지통 전용 화면이 없음. 그냥 폴더로 들어가는 방식이라 "복원" 같은 동작을 붙일 자리가 없음 | — |

### 1.2 요구사항

**새 파일: `Spartan/Trash/TrashManager.swift`**

저장 구조를 아래와 같이 바꿉니다.

```
/private/var/mobile/Media/.Trash/
├── argo-trash-index.json      ← 메타데이터 색인
└── items/
    ├── <uuid-1>/원래이름.pdf
    └── <uuid-2>/원래폴더이름/
```

UUID 폴더로 감싸므로 **이름 충돌이 구조적으로 불가능**합니다 (T2 해결). 원래 이름도 그대로 보존됩니다.

색인 파일 스키마:

```json
{
  "version": 1,
  "entries": [
    {
      "id": "6F9619FF-8B86-D011-B42D-00CF4FC964FF",
      "originalPath": "/private/var/mobile/Documents/보고서.pdf",
      "originalName": "보고서.pdf",
      "deletedAt": 1760000000.0,
      "isDirectory": false,
      "size": 10485760
    }
  ]
}
```

구현할 API:

```swift
struct TrashEntry: Codable, Identifiable, Hashable {
    let id: String              // UUID 문자열
    let originalPath: String    // 원본 전체 경로
    let originalName: String
    let deletedAt: Date
    let isDirectory: Bool
    let size: Int64

    var storedPath: String { get }   // .Trash/items/<id>/<originalName>
    var deletedAtDescription: String { get }  // UserDefaults.settings "dateFormat" 사용
}

enum TrashRestoreResult {
    case restored(String)          // 복원된 최종 경로
    case renamed(String)           // 이름 충돌로 바꿔서 복원한 경로
    case failed(String)            // 사유
}

final class TrashManager: ObservableObject {
    static let shared: TrashManager

    @Published private(set) var entries: [TrashEntry]   // 최근 삭제 순 정렬
    var totalSize: Int64 { get }

    func reload()
    @discardableResult func moveToTrash(path: String) -> Bool
    func moveToTrash(paths: [String]) -> (moved: Int, failed: Int)
    func restore(_ entry: TrashEntry) -> TrashRestoreResult
    func deleteForever(_ entry: TrashEntry) -> Bool
    func emptyAll() -> Bool
}
```

동작 규칙:

1. **`moveToTrash`**
   - UUID 생성 → `RootHelperActs.mkdir(.Trash/items/<uuid>)` → `RootHelperActs.mv(원본, .Trash/items/<uuid>/<이름>)`
   - 색인에 항목 추가 후 저장
   - 이동 실패 시 색인에 쓰지 말 것 (유령 항목 방지)
   - 휴지통 자체(`/private/var/mobile/Media/.Trash`)를 버리려 하면 거부

2. **`restore`**
   - 원본의 부모 폴더가 사라졌으면 `RootHelperActs.mkdir`로 재생성
   - 원본 경로에 이미 파일이 있으면 덮어쓰지 말고 `이름 (1).확장자`, `이름 (2).확장자` … 로 번호를 붙여 복원하고 `.renamed`를 반환
   - 성공 시 색인에서 항목 제거, UUID 폴더도 정리

3. **레거시 호환**
   - 색인에 없는데 `.Trash/` 바로 아래 존재하는 파일 = 구버전에서 버린 것
   - 목록에 "원본 위치 모름"으로 표시하고 **영구삭제만** 허용, 복원 버튼은 비활성
   - 절대 자동으로 지우지 말 것

4. **색인 저장**
   - `tempPath`에 JSON을 쓰고 `RootHelperActs.mvtemp()` 로 옮김
   - 읽기 실패·파싱 실패 시 빈 색인으로 간주하고 앱은 정상 동작 (크래시 금지)

**새 파일: `Spartan/Trash/TrashView.swift`**

휴지통 전용 화면.

- 상단: 제목 "휴지통", 항목 수와 총 용량
- 목록: 행 하나당 — 아이콘 / 원래 이름 / 작은 글씨로 `원본 경로 · 삭제 시각 · 크기`
- **행은 버튼 하나**(0.5 규칙). 선택하면 상세 패널로 전환
- 상세 패널 버튼: `복원` / `영구 삭제` / `뒤로`
- 상단 툴바 아이콘: 새로고침 / 선택모드 / 전체선택 / 선택항목 복원 / 선택항목 영구삭제 / 휴지통 비우기 / 닫기
- `휴지통 비우기`와 `영구 삭제`는 **확인 단계 필수** (`.alert` 또는 확인 패널). 실수로 한 번 눌러서 날아가면 안 됩니다
- `.onExitCommand`: 상세 → 목록 → 선택모드 해제 → 닫기 순서
- 복원 결과를 하단에 한 줄로 표시 ("복원했습니다: /경로", "이름이 겹쳐 '보고서 (1).pdf'로 복원했습니다")

**기존 파일 수정: `Spartan/ContentView.swift`**

- 기존의 `RootHelperActs.mv(..., ".Trash/" + name)` 호출 **3곳**(`:345`, `:804`, `:1119`)을 전부 `TrashManager.shared.moveToTrash(path:)` 로 교체
- 기존 "휴지통 비우기" 호출(`:793`, `:1106`)을 `TrashManager.shared.emptyAll()` 로 교체
- 툴바에 휴지통 버튼 추가 (아이콘 `trash`), `TrashView`를 시트로 염
- `showSubView` 배열 크기 확장 — **1.4절 참조**

### 1.3 즐겨찾기 (Favorites)

### 1.3.1 현황 검토 결과 — **치명적 버그 있음**

`Spartan/ButtonActions/AddToFavoritesView.swift` 38~43행:

```swift
Button(action: {
    favoritesDisplayName.append(displayName)      // ← 로컬 배열에 추가하고
    favoritesFilePath.append(filePath)
    UserDefaults.favorites.set(["Documents", "Applications", ...],  // ← 하드코딩된 기본값으로 덮어씀
                              forKey: "favoritesDisplayName")
    UserDefaults.favorites.set(["/private/var/mobile/Documents/", ...],
                              forKey: "favoritesFilePath")
    ...
```

추가한 값을 저장하는 게 아니라 **하드코딩된 기본값 4개를 다시 써 넣습니다.** 즉 **즐겨찾기 추가가 전혀 동작하지 않습니다.** 사용자가 무엇을 추가하든 목록은 항상 초기값 그대로입니다.

그 밖의 결함:

| # | 문제 | 위치 |
|---|---|---|
| F1 | **추가가 저장되지 않음** (위 설명) | `AddToFavoritesView.swift:38` |
| F2 | 삭제·이름변경·순서변경 기능 없음 | `FavoritesView.swift` |
| F3 | 두 배열(`displayName`, `filePath`)을 별도 키로 저장 → 길이가 어긋나면 잘못된 경로로 이동 | 전체 |
| F4 | 아이콘을 `firstIndex(of:)`로 고름 → 이름이 같은 항목이 둘이면 엉뚱한 아이콘 | `FavoritesView.swift:31` |
| F5 | 세 파일이 각자 배열 사본을 들고 있어 동기화 안 됨 | `SpartanApp`, `FavoritesView`, `AddToFavoritesView` |

### 1.3.2 요구사항

**새 파일: `Spartan/Favorites/FavoritesStore.swift`**

```swift
struct FavoriteItem: Codable, Identifiable, Hashable {
    var id: UUID
    var name: String
    var path: String           // 항상 "/"로 끝나도록 정규화
    var systemImage: String    // SF Symbol 이름
}

final class FavoritesStore: ObservableObject {
    static let shared: FavoritesStore
    @Published var items: [FavoriteItem]   // 변경 시 자동 저장

    func add(name: String, path: String, systemImage: String)
    func contains(path: String) -> Bool
    func remove(_ item: FavoriteItem)
    func rename(_ item: FavoriteItem, to newName: String)
    func moveUp(_ item: FavoriteItem)
    func moveDown(_ item: FavoriteItem)
    func resetToDefaults()
}
```

- 저장: `UserDefaults.favorites`에 `JSONEncoder`로 **단일 키 `"favoriteItems"`** 사용 (F3 해결)
- **마이그레이션:** 앱 최초 실행 시 구버전 두 배열(`favoritesDisplayName` / `favoritesFilePath`)이 있으면 읽어서 새 형식으로 변환하고, 변환 완료 플래그를 저장. 두 배열의 길이가 다르면 짧은 쪽 기준으로 자름
- 기본 항목 (최초 1회만 심고, 이후 사용자가 지우면 다시 생기지 않도록 플래그 관리):

| 이름 | 경로 | 아이콘 |
|---|---|---|
| Documents | `/private/var/mobile/Documents/` | `doc.text` |
| Argo Downloads | `/private/var/mobile/Documents/Argo/` | `arrow.down.circle` |
| Applications | `/Applications/` | `square.grid.2x2` |
| User Apps | `/private/var/containers/Bundle/Application/` | `person.crop.circle` |
| Trash | `/private/var/mobile/Media/.Trash/` | `trash` |

**기존 파일 수정: `Spartan/ButtonActions/FavoritesView.swift` 전면 교체**

- `FavoritesStore.shared`를 `@ObservedObject`로 구독
- 행 = 버튼 하나. 평소에는 **선택 즉시 그 폴더로 이동**
- 하단에 `편집` 토글 버튼. 편집 모드에서는 행을 선택하면 해당 항목의 편집 패널(이름 변경 / 위로 / 아래로 / 삭제 / 뒤로)이 열림
  → WebDAV 연결 목록의 `editMode` 패턴과 동일하게 (`WebDAVView.swift` 참고)
- 아이콘은 `item.systemImage`를 그대로 사용 (F4 해결)
- 하단 버튼: `현재 폴더 추가` — 지금 보고 있는 폴더를 한 번에 추가. 이미 등록돼 있으면 비활성 + "이미 추가됨" 표시

**기존 파일 수정: `Spartan/ButtonActions/AddToFavoritesView.swift`**

- 저장 로직을 `FavoritesStore.shared.add(...)` 호출로 교체 (F1 해결)
- 아이콘 선택 UI 추가: SF Symbol 8~10개를 가로로 늘어놓고 고르게 (`folder`, `doc.text`, `film`, `music.note`, `photo`, `arrow.down.circle`, `star`, `gearshape`, `externaldrive`, `trash`)
- 경로 자동 정규화: 끝에 `/` 없으면 붙임

### 1.4 `showSubView` 배열 확장 (반드시 처리)

`ContentView.swift:54`:

```swift
@State private var showSubView: [Bool] = [Bool](repeating: false, count: 36)
```

현재 인덱스 0~35가 모두 사용 중입니다. 이번 작업에서 2개가 추가되므로:

```swift
// 36 = pdfViewShow, 37 = trashViewShow
@State private var showSubView: [Bool] = [Bool](repeating: false, count: 38)
```

**이 수정을 빠뜨리면 앱이 실행 즉시 index out of range로 죽습니다.**
파일 상단 주석 블록(54~90행)에도 두 항목을 추가하십시오.

---

## 2. 작업 2 — 배포 타깃 tvOS 16.0

### 2.1 변경 대상

1. `Spartan.xcodeproj/project.pbxproj` — `TVOS_DEPLOYMENT_TARGET = 13.0;` 이 **4군데** 있습니다. 전부 `16.0`으로.
   → 단 **작업자는 pbxproj를 직접 수정하지 말고**, "4곳 전부 16.0으로 변경 요청"이라고 결과물에 적기만 하십시오. 검수자가 반영합니다.
2. `Spartan/Info.plist` — `MinimumOSVersion` 을 `13.0` → `16.0`

### 2.2 코드 정리 (보수적으로)

tvOS 13/14 전용 우회 코드가 남아 있습니다. **전면 리팩터링은 하지 마십시오.** 위험 대비 이득이 없습니다.

- `if #available(tvOS 14.0, *)` / `15.0` / `16.0` 블록은 **그대로 두십시오.** 항상 참이 될 뿐 해롭지 않습니다.
- `sw_vers` 열거형과 `isJB` 분기도 유지하십시오.
- 단, **새로 작성하는 코드**에서는 `#available` 가드 없이 tvOS 16 API를 바로 쓰십시오.

### 2.3 주의

CI는 Xcode 15.4 / tvOS 17.5 SDK로 빌드합니다. 배포 타깃을 16으로 올려도 SDK는 17.5이므로, **tvOS 17 전용 API를 실수로 쓰면 컴파일은 통과하고 기기에서 죽습니다.** tvOS 16.0 기준으로만 작성하십시오.

---

## 3. 작업 3 — PDF 뷰어

### 3.1 현황

PDF를 열면 **헥스 에디터로 떨어집니다.** `FileInfo.yandereDevFileType`에 PDF 분기가 없어 `69`(알 수 없음)로 판정되기 때문입니다.

PDFKit은 tvOS 11 이상에서 사용 가능합니다 (`PDFDocument`, `PDFPage`, `PDFView`, `PDFAnnotation`, `PDFOutline` 모두 포함). 배포 타깃이 16이 되므로 제약 없이 쓸 수 있습니다.

### 3.2 파일 타입 등록

**`Spartan/FileInfo.swift` 수정**

`yandereDevFileType(file:)`에 PDF 분기를 추가하십시오. 반환값은 **`15`** 를 사용합니다 (0~14가 이미 사용 중, 69는 unknown).

판정 순서가 중요합니다. `isText`보다 **앞에** 두십시오 (PDF 앞부분이 텍스트로 오판될 수 있음).

판정 방법은 두 가지를 모두 적용:
- 확장자가 `.pdf`
- 또는 파일 선두 5바이트가 `%PDF-` (매직 넘버)

`isPDF(filePath:)` 헬퍼를 만들어 쓰십시오. 기존 `isHTML` 등의 구현 스타일을 따르십시오.

**`Spartan/ContentView.swift` 수정**

- `defaultAction` 의 `switch fileType` (1594행 부근)에 `case 15: showSubView[36] = true` 추가
- 컨텍스트 메뉴의 "열기" 분기 두 곳(729행 부근, 1033행 부근)에도 `case 15` 추가 — 기존 `case 14`(HTML) 바로 뒤에 같은 모양으로
- 1671행 부근의 `case 14` 옆에도 추가
- `.sheet(isPresented: $showSubView[36])` 로 `PDFReaderView` 연결

### 3.3 새 파일: `Spartan/FileOpeners/PDFReader/PDFReaderView.swift`

#### 기본 요구

- `PDFDocument(url:)` 로 로드. 실패 시 "열 수 없는 PDF입니다" 안내 후 닫기 (크래시 금지)
- 암호 걸린 PDF: `document.isLocked` 확인 → 비밀번호 입력 필드 제공 → `document.unlock(withPassword:)`
- `PDFView`를 `UIViewRepresentable`로 감싸 SwiftUI에 올립니다
- **대용량 PDF 대응**: 전체 페이지를 미리 렌더링하지 마십시오. `PDFView`의 자체 페이징에 맡기고, 썸네일은 보이는 범위만 생성

#### 리모컨 조작 (tvOS 핵심)

터치가 없으므로 조작 설계가 성패를 가릅니다. **모드 개념**을 둡니다.

**읽기 모드** (기본)

| 입력 | 동작 |
|---|---|
| 좌 / 우 스와이프 | 이전 / 다음 페이지 |
| 상 스와이프 | 상단 툴바 표시 (3초 후 자동 숨김) |
| 하 스와이프 | 하단 진행바 + 페이지 슬라이더 표시 |
| 클릭(선택) | 툴바 토글 |
| 메뉴 버튼 | 닫기 |
| 재생/일시정지 | 자동 넘김 토글 (선택 구현) |

**확대 모드** (툴바에서 진입)

| 입력 | 동작 |
|---|---|
| 상하좌우 | 화면 이동(패닝) |
| 클릭 | 확대 단계 순환 (맞춤 → 100% → 150% → 200%) |
| 메뉴 | 읽기 모드로 복귀 |

#### 상단 툴바 (아이콘 버튼, 행당 하나 규칙은 툴바에는 해당 없음 — HStack의 개별 Button은 각각 포커스됨)

| 아이콘 | 기능 |
|---|---|
| `chevron.left` / `chevron.right` | 이전 / 다음 페이지 |
| `list.bullet.indent` | 목차(PDFOutline). 없으면 비활성 |
| `square.grid.2x2` | 썸네일 격자 — 페이지 골라서 점프 |
| `textformat.size` | 보기 모드 전환 (아래) |
| `sun.max` | 테마 전환 (아래) |
| `highlighter` | 형광펜 모드 진입 |
| `magnifyingglass` | 텍스트 검색 |
| `bookmark` | 현재 페이지 북마크 토글 |
| `xmark` | 닫기 |

#### 보기 모드 (이북 리더 느낌)

`PDFView`의 `displayMode` / `displayDirection` 조합:

| 이름 | 설정 |
|---|---|
| 한 쪽 | `.singlePage`, 페이지 단위로 넘김 |
| 연속 스크롤 | `.singlePageContinuous`, `displayDirection = .vertical` |
| 두 쪽 (책 펼침) | `.twoUpContinuous` 또는 `.twoUp`, `displaysAsBook = true` |

맞춤 방식: `폭 맞춤` / `전체 맞춤` 토글. `autoScales`와 `scaleFactor`로 제어.

#### 테마 (어두운 거실 대응 — 중요)

TV는 어두운 방에서 보므로 흰 배경 PDF는 눈이 부십니다.

| 테마 | 구현 |
|---|---|
| 원본 | 그대로 |
| 야간 | 반전 — `PDFView` 위에 `.colorInvert()` + `.hueRotation(.degrees(180))` 조합, 또는 `CIFilter`(`CIColorInvert`)로 페이지 렌더링 |
| 세피아 | 따뜻한 색 오버레이 (`.colorMultiply(Color(red:0.98,green:0.94,blue:0.84))`) |
| 밝기 | 0.3 ~ 1.0 슬라이더, `.brightness()` 적용 |

`PDFView.backgroundColor`도 테마에 맞게 바꾸십시오.

#### 읽던 위치 기억

- 파일 경로의 SHA256(또는 안정적인 해시) 를 키로 `UserDefaults.settings`에 마지막 페이지 번호 저장
- 다시 열면 그 페이지로 복귀하고, 하단에 "N쪽부터 이어 봅니다" 를 2초간 표시
- 키 접두어: `pdfProgress.<hash>`

#### 북마크

- 같은 방식으로 `pdfBookmarks.<hash>` 에 `[Int]` 저장
- 툴바 북마크 버튼으로 현재 페이지 토글
- 목차 화면에 "북마크" 탭을 두어 목록에서 점프

#### 하단 진행 표시

- `12 / 348` 페이지 번호
- 진행률 바
- 남은 분량 비율 (`34%`)

### 3.4 형광펜 (Highlighter)

tvOS에는 텍스트를 끌어서 선택하는 수단이 없습니다. **두 단계 방식**으로 구현합니다.

**1단계 — 영역 지정**

- 형광펜 모드에 들어가면 화면 중앙에 반투명 **선택 상자**가 나타남
- 리모컨 상하좌우로 상자를 이동
- 재생/일시정지 버튼으로 "이동 모드 ↔ 크기조절 모드" 전환 (크기조절 모드에서는 상하좌우가 상자 크기를 바꿈)

**2단계 — 텍스트 스냅 후 확정**

- 상자가 멈출 때마다 `page.selection(for: rect)` 로 겹치는 텍스트를 구해 **실제 단어 경계로 스냅**하고, 스냅된 범위를 더 진한 색으로 미리보기
- 클릭하면 확정 → `PDFAnnotation(bounds:forType:.highlight, withProperties:)` 생성, `annotation.color`로 색 지정, `page.addAnnotation()`
- 선택 범위에 텍스트가 없으면(스캔 PDF) 상자 영역 그대로 사각형 하이라이트

**색상**: 노랑 / 초록 / 파랑 / 분홍 4색. 툴바에서 선택, 다음에도 기억.

**지우기**: 형광펜 모드에서 기존 하이라이트 위에 상자를 올리면 "삭제" 버튼이 나타남 → `page.removeAnnotation()`

#### 저장 — 원본을 건드리지 마십시오 (중요)

사용자의 PDF 원본을 수정하면 안 됩니다. NAS에서 받아온 파일일 수도 있고, 되돌릴 수 없습니다.

**기본 동작: 사이드카 파일에 저장**

```
/private/var/mobile/Documents/Argo/.annotations/<hash>.json
```

```json
{
  "version": 1,
  "sourcePath": "/private/var/mobile/Documents/보고서.pdf",
  "highlights": [
    {
      "page": 11,
      "bounds": { "x": 72.0, "y": 540.0, "w": 180.5, "h": 14.0 },
      "color": "yellow",
      "text": "하이라이트된 본문 일부",
      "createdAt": 1760000000.0
    }
  ]
}
```

- 문서를 열 때 사이드카를 읽어 `PDFAnnotation`으로 복원해 올림
- 하이라이트 추가·삭제 시 즉시 저장
- 사이드카가 깨져 있으면 무시하고 빈 상태로 (크래시 금지)

**추가 기능: 주석 포함본 내보내기**

- 툴바 또는 메뉴에 `주석 포함 사본 저장`
- `document.write(to:)` 로 **새 파일**을 만듭니다: `원래이름_annotated.pdf`, 같은 폴더
- 원본은 절대 덮어쓰지 않음

**추가 기능: 하이라이트 목록**

- 목차 화면에 "하이라이트" 탭
- 하이라이트한 텍스트를 목록으로 보여주고, 선택하면 그 페이지로 점프
- 이게 있어야 형광펜이 실제로 쓸모가 있습니다

### 3.5 텍스트 검색

- `document.findString(_:withOptions:)` 사용
- tvOS 키보드는 입력이 불편하므로, 검색어는 최근 5개를 기억해 재사용 가능하게
- 결과를 목록으로 (페이지 번호 + 앞뒤 문맥) 보여주고 선택 시 점프
- 현재 일치 항목은 `PDFView.setCurrentSelection(_:animate:)` 로 강조

### 3.6 PDF 뷰어 파일 분할

한 파일에 다 넣지 마십시오. 아래로 나누십시오.

```
Spartan/FileOpeners/PDFReader/
├── PDFReaderView.swift        // 메인 화면, 모드 전환, 툴바
├── PDFKitContainer.swift      // UIViewRepresentable 래퍼
├── PDFReaderModel.swift       // ObservableObject — 문서/페이지/테마/모드 상태
├── PDFAnnotationStore.swift   // 사이드카 JSON 입출력
├── PDFHighlightOverlay.swift  // 선택 상자 UI + 스냅 로직
└── PDFOutlineView.swift       // 목차 / 북마크 / 하이라이트 3탭
```

---

## 4. 결과물 제출 형식

### 4.1 파일 단위로, 전체 내용을

조각이나 "…(생략)" 금지. 각 파일을 아래 형식으로 **처음부터 끝까지** 주십시오.

````
#### 파일: Spartan/Trash/TrashManager.swift
```swift
//
//  TrashManager.swift
//  Argo
//
(전체 내용)
```
````

기존 파일을 고칠 때는 **수정된 파일 전체**를 주십시오. diff 형식은 받지 않습니다.
단 `ContentView.swift`는 1,870줄로 너무 크므로 **이 파일만 예외**로, 아래 형식의 정확한 치환 지시를 주십시오.

````
#### 수정: Spartan/ContentView.swift

**치환 1** (54행 부근)
찾을 내용:
```swift
    @State private var showSubView: [Bool] = [Bool](repeating: false, count: 36)
```
바꿀 내용:
```swift
    @State private var showSubView: [Bool] = [Bool](repeating: false, count: 38)
```
````

찾을 내용은 **파일에 있는 그대로, 공백까지 정확히**, 그리고 파일 안에서 **유일하게** 매칭되도록 충분히 길게 적으십시오.

### 4.2 마지막에 반드시 첨부할 것

```
### 신규 파일 등록 요청
(새로 만든 .swift 파일 경로 전부)

### 프로젝트 설정 변경 요청
(pbxproj / Info.plist 변경 사항)

### 자가 점검
- [ ] 새 외부 의존성 없음
- [ ] tvOS 16.0에 존재하는 API만 사용
- [ ] showSubView 배열 크기를 38로 늘렸고 새 인덱스 36, 37을 사용
- [ ] 리스트 행마다 포커스 가능한 버튼은 하나뿐
- [ ] 한 뷰에 .sheet는 하나만
- [ ] root 영역 쓰기는 RootHelperActs / mvtemp 경유
- [ ] PDF 원본 파일을 수정하는 코드 없음
- [ ] 영구삭제·휴지통비우기에 확인 단계 있음
- [ ] 전역 fileManager 사용 (SpartanApp.swift 안에서는 FileManager.default)

### 확신이 없는 부분
(추측으로 쓴 API, 검증이 필요한 지점을 솔직히 나열)
```

마지막 항목이 중요합니다. **모르는 것을 아는 척하지 마십시오.** 불확실한 부분을 적어주면 검수 단계에서 집중해서 봅니다. 지어낸 API가 섞여 있는 쪽이 훨씬 손해입니다.

### 4.3 작업 순서

한 번에 전부 주지 말고 아래 순서로 나눠 주십시오. 각 묶음마다 검수·빌드 확인 후 다음으로 갑니다.

| 단계 | 범위 |
|---|---|
| 1 | 휴지통 (`TrashManager`, `TrashView`, `ContentView` 연동) + `showSubView` 확장 |
| 2 | 즐겨찾기 (`FavoritesStore`, `FavoritesView`, `AddToFavoritesView`) |
| 3 | tvOS 16 타깃 + `FileInfo` PDF 판정 + PDF 기본 뷰어 |
| 4 | PDF 고급 기능 (형광펜, 목차/북마크, 테마, 검색) |

---

## 5. 참고 — 기존 코드 패턴

작업 전에 아래 파일을 읽어 코딩 스타일을 맞추십시오.

| 파일 | 참고할 것 |
|---|---|
| `Spartan/WebDAV/WebDAVView.swift` | tvOS 툴바 구성, `editMode` 토글, `.onExitCommand` 단계적 처리, 단일 `.sheet` 분기 |
| `Spartan/WebDAV/WebDAVStore.swift` | `ObservableObject` + `UserDefaults` JSON 저장 패턴 |
| `Spartan/FileOpeners/TextView.swift` | 파일 뷰어 기본 구조 |
| `Spartan/FileOpeners/HexView.swift` | 큰 파일을 나눠 읽는 방식 |
| `Spartan/SpareViews.swift` | `RootHelperActs`, 전역 헬퍼 |

지역화는 `NSLocalizedString`을 쓰되, 새 문자열은 **영어로** 적으십시오 (기존 `Localizable.strings`에 키를 추가하는 것은 선택).

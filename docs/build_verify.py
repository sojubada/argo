#!/usr/bin/env python3
"""검증 제출 자료를 실제 소스에서 뽑아 만든다.

코드를 옮겨 적지 않는다 — 옮겨 적으면 검토 대상과 빌드되는 것이 어긋난다.
"""
import io, os, subprocess, sys

ROOT = "/home/claude/argo"

def read(path):
    return io.open(os.path.join(ROOT, path), encoding="utf-8").read().rstrip("\n")

def block(path, lang="swift", label=None):
    body = read(path)
    n = len(body.split("\n"))
    return "### `%s`  (%d줄)\n\n```%s\n%s\n```\n\n" % (label or path, n, lang, body)

def span(path, start, end, lang="swift", label=None):
    lines = io.open(os.path.join(ROOT, path), encoding="utf-8").read().split("\n")
    body = "\n".join(lines[start-1:end]).rstrip("\n")
    n = len(body.split("\n"))
    where = label or ("%s:%d-%d" % (path, start, end))
    return "### `%s`  (%d줄)\n\n```%s\n%s\n```\n\n" % (where, n, lang, body)

def find(path, needle, before=0, after=40, lang="swift", label=None):
    """needle 이 든 줄을 찾아 그 둘레를 뜬다. 줄 번호가 밀려도 따라간다."""
    lines = io.open(os.path.join(ROOT, path), encoding="utf-8").read().split("\n")
    for i, L in enumerate(lines):
        if needle in L:
            a = max(0, i - before)
            b = min(len(lines), i + after)
            body = "\n".join(lines[a:b]).rstrip("\n")
            where = label or ("%s:%d-%d" % (path, a + 1, b))
            return "### `%s`  (%d줄)\n\n```%s\n%s\n```\n\n" % (
                where, len(body.split("\n")), lang, body)
    raise SystemExit("못 찾음: %s in %s" % (needle, path))

def write(name, text):
    out = os.path.join(ROOT, "docs", name)
    io.open(out, "w", encoding="utf-8").write(text)
    print("docs/%s  (%d줄)" % (name, len(text.split("\n"))))

def head():
    sha = subprocess.check_output(["git", "-C", ROOT, "rev-parse", "--short", "HEAD"]).decode().strip()
    return sha


SHA = head()

INDEX = """# Argo 검증 요청 — 제미나이 제출 자료

커밋 `{SHA}` · 릴리즈 `build 135` 기준. CI 빌드 통과.
대상 기기: Apple TV 4K / tvOS 16.6 / TrollStore (탈옥 아님).

---

## 0. 이 묶음을 읽는 법

**검토해 주셔야 할 것은 "잘 짰는가"가 아니라 "실제로 되는가"입니다.**

지난 며칠 동안 제가 자신 있게 말한 것 중 여러 개가 실기기에서 틀렸습니다.
빌드가 통과한다는 사실은 화면에 무엇이 나오는지, 리모컨이 먹는지,
파일이 지워지는지에 대해 **아무것도 말해 주지 않습니다.** 그래서 이 자료는
코드를 자랑하는 문서가 아니라 **의심 목록**입니다.

| 문서 | 내용 | 분량 |
| --- | --- | --- |
| `VERIFY_INDEX.md` | 이 문서 — 상태표, 우선 검증 항목, 제가 틀렸던 것들 | |
| `VERIFY_A_AIRDROP.md` | AirDrop 통합 — **전부 미검증**, 비공개 API | 약 500줄 |
| `VERIFY_B_READERS.md` | PDF·텍스트 뷰어와 포커스 모델 — **흰 화면 미해결** | 약 1,000줄 |
| `VERIFY_C_SAFETY.md` | 경로 권한·관문·삭제·휴지통 | 약 1,100줄 |
| `VERIFY_D_BROWSER.md` | 두 칸·키보드·클립보드·전송·압축 | 약 1,100줄 |

각 문서는 **맨 앞에 "답해 주셔야 할 질문"** 이 있습니다. 코드는 디스크에서
그대로 뽑았습니다 — 옮겨 적으면 검토 대상과 빌드되는 것이 어긋납니다.

---

## 1. 상태표 — 무엇이 확인되었고 무엇이 아닌가

**실기기에서 확인된 것** (사용자가 애플TV 에서 직접 봄)

| 항목 | 결과 |
| --- | --- |
| WebKit 을 `dlopen` 으로 꺼내 쓰기 | ✓ 동작 |
| docx · xlsx · pptx · hwp 렌더링 | ✓ 보임 |
| 인쇄 서식기(`UIPrintPageRenderer` 일가) | ✗ **클래스는 있으나 호출하면 SIGSEGV** |
| `CGPDFDocument` 로 PDF 열기 | ✓ 열림 (쪽 수를 읽음) |
| 앱 실행 직후 종료 | ✓ 고침 (`cacheFolder` 강제 언래핑) |
| 화면이 왼쪽으로 밀림 | ✓ 고침 (툴바 폭) |
| 포커스된 줄의 파일명이 안 보임 | ✓ 고침 (글자색 고정 해제) |
| 폴더에 못 들어감 | ✓ 고침 (`showAlert` 깃발) |

**고쳤다고 주장하지만 아직 기기에서 확인되지 않은 것**

| 항목 | 상태 |
| --- | --- |
| **PDF 가 흰 화면** | ✗ **미해결.** 렌더링 경로를 바꿨으나 원인을 모름 |
| 리모컨 쪽 넘김 | 고침 주장 — 미확인 |
| 삭제가 안 됨 | 고침 주장 — 미확인 |
| 권한이 과도하게 막힘 | 고침 주장 — 미확인 |
| 클립보드 복사·붙여넣기 | 미확인 |
| 압축 뒷줄 처리·빈 공간 검사 | 미확인 |
| 되돌리기(`⌘Z`) | 미확인 |

**한 번도 기기에서 돌려 본 적 없는 것**

| 항목 | 비고 |
| --- | --- |
| **AirDrop 통합 전부** | 별도 검증 앱에서는 송·수신 확인. **Argo 안에서는 처음** |
| 1픽셀 수신 컨트롤러 부착 | **검증 앱에 없던 방식.** 제가 만든 것 |
| 두 칸 탐색기 전반 | 빌드만 통과 |

---

## 2. 먼저 봐 주셔야 할 네 가지

### ① PDF 흰 화면 — 원인을 모릅니다  ★최우선★

증상: PDF 를 열면 **쪽 수는 읽히는데(`1 / 1`) 화면이 완전히 흽니다.**

확인된 사실:
- `CGPDFDocument` 가 문서를 **열었습니다** (못 열면 오류 화면이 뜨게 되어 있음)
- 쪽 높이 계산이 **끝까지 돌았습니다** (상태줄의 `· 0%` 는 그때만 나옴)
- 즉 **그리기까지 갔는데 잉크가 없습니다**

제가 한 것: 직접 만든 `CGBitmapContext` → `UIGraphicsImageRenderer` 로 교체.
**이것이 원인이라는 근거는 없습니다.** 가장 흔히 쓰이는 경로라서 바꾼 것입니다.

**질문:** `VERIFY_B_READERS.md` 의 `renderCurrentPage` 를 보고,
`drawPDFPage` 가 아무것도 그리지 않을 수 있는 경로를 찾아 주십시오.
특히 좌표계 뒤집기와 `getDrawingTransform` 의 조합이 맞는지 봐 주십시오.

### ② AirDrop — 비공개 API, 통합은 미검증

`SFAirDropDiscoveryController` 의 `view` 에 **접근하면 앱이 즉시 죽습니다.**
`addSubview`·`present`·`frame` 설정 전부 포함입니다. 코드에서 이 규칙이
어디선가 깨지지 않았는지 봐 주십시오.

그리고 **수신 광고를 1픽셀 자식 뷰로 붙이는 방식은 제가 만든 것입니다.**
검증 앱은 전체 화면으로 `present` 했습니다. 이 방식이 통할 근거가 없습니다.

### ③ 포커스 모델 — 세 뷰어를 같은 방식으로 바꿨습니다

툴바를 화면 아래로 옮기면서 "위쪽 화살표는 툴바로 가는 길" 코드를 그대로 둬,
**세 뷰어 모두 쪽 넘김이 죽어 있었습니다.** 이제 "보이는 것"과 "입력을 받는 것"을
나눴습니다. 같은 수정을 세 곳에 했으므로 **한 곳이 틀리면 세 곳이 틀립니다.**

### ④ 경로 권한 — 한 번 크게 틀렸던 곳

`resolvingSymlinksInPath()` 가 `/private` 접두사를 떼어내는데 목록은
`/private/var/...` 로 적혀 있어 **안전 구역이 하나도 안 맞았습니다.**
모든 폴더가 '주의'가 되고 안전 모드가 전부 막아 **자기 파일도 못 지웠습니다.**
막아야 할 곳은 덜 막고 열어야 할 곳은 더 막는, 양쪽으로 틀린 상태였습니다.

고친 뒤의 경계를 표로 뽑아 두었습니다(`VERIFY_C_SAFETY.md` 2장).
**그 표에서 틀린 줄을 찾아 주십시오.**

---

## 3. 제가 틀렸던 것들 — 제 주석을 사실로 읽지 마십시오

리뷰하실 때 제 판단을 근거로 삼지 마시라는 뜻에서 적습니다.
전부 실기기나 빌드가 반증한 것들입니다.

| 제가 한 말 | 실제 |
| --- | --- |
| "인쇄 API 가 SDK 에 전부 PRESENT" | 헤더에 이름만 있고 `API_UNAVAILABLE(tvos)`. 조회 도구가 가용성 속성을 안 읽었음 |
| "`focusSection()` 은 tvOS 15+ 라 못 쓴다" | 배포 대상이 **16.0**. 쓸 수 있었음. 확인하지 않고 적음 |
| "안전 모드 검사가 영어 문자열을 비교한다" | 그런 코드는 없었음 |
| "비동기 경로를 전부 고정했으니 경쟁 상태는 없다" | 절반만 맞음. **같은 클로저 안의 `@State` 읽기-쓰기**에서 터짐 |
| 단축키 시트 인덱스 (삭제·압축·해제) | 셋 다 틀림 |
| `.m` 파일을 Swift 로 프로젝트에 등록 | 빌드가 거부 |
| `Menu` 가 tvOS 14 에 있다고 가정 | tvOS 17 부터 |
| ObjC 메서드의 Swift 이름 추측 | 두 번 빌드 낭비. 지금은 `NS_SWIFT_NAME` 으로 못 박음 |

---

## 4. 검증 환경

- 빌드: GitHub Actions `macos-14`, Xcode 15.4, tvOS 17.5 SDK, 배포 대상 **tvOS 16.0**
- 로컬 Xcode 없음 — **컴파일 오류는 CI 에서만 발견됩니다**
- 설치: TrollStore 로 미서명 IPA
- 루트 헬퍼(`/bin/RootHelper`)는 **실제로 돈 적이 없습니다** —
  `TROLLSTORE` 컴파일 플래그가 정의되지 않았고 바이너리가 IPA 에 없습니다.
  **모든 쓰기는 `FileManager` 로 갑니다.** 검토도 거기에 집중해 주십시오.

---

## 5. 돌려주실 형식

고칠 코드를 주시기보다 **어디가 왜 틀렸는지**를 주시면 제가 고치겠습니다.
빌드가 한 번에 2~10분 걸려서, 추측으로 고치면 왕복만 쌓입니다.

가능하면 이렇게 적어 주십시오.

```
[심각도] 파일:줄  — 증상 → 원인 → 근거
```

**근거를 함께 주십시오.** "이러면 안 됩니다" 보다
"애플 문서의 X 는 Y 라고 적혀 있고, 이 코드는 Z 를 가정합니다" 가
훨씬 쓸모 있습니다. 제가 근거 없이 단정해서 틀린 적이 여러 번입니다.
""".replace("{SHA}", SHA)

write("VERIFY_INDEX.md", INDEX)


# ─────────────────────────────────────────────── A. AirDrop

A = """# 검증 A — AirDrop 통합

커밋 `{SHA}`. **Argo 안에서는 한 번도 기기에서 돌려 보지 않았습니다.**

별도 검증 앱(`sojubada/AirDrop_tvOS`)에서 Apple TV 4K / tvOS 16.6 / TrollStore 로
**송·수신이 모두 확인된** 방법을 옮긴 것입니다. 다만 아래 ③ 은 그 앱에 없던,
제가 만든 방식입니다.

---

## 답해 주셔야 할 질문

### Q1. `SFAirDropDiscoveryController` 의 `view` 를 건드리는 곳이 있습니까?  ★

검증 과정에서 확인된 사실입니다 — 이 컨트롤러는 이름이 Controller 이고
`UIViewController` 를 상속하지만 **화면에 올릴 수 있는 물건이 아닙니다.**
아래는 전부 즉시 크래시입니다.

```objc
[self addChildViewController:discovery];
[self.view addSubview:discovery.view];
[self presentViewController:discovery animated:YES completion:nil];
discovery.view.frame = ...;
```

`AirDropBridge.m` 전체에서 `sDiscovery` 의 `view` 에 닿는 경로가 있는지
봐 주십시오. **간접 경로도 포함입니다** — 예를 들어 이 객체를 배열에 넣어
UIKit 이 순회하게 두거나, 디버거 출력(`%@`)으로 `description` 을 부르는 것이
`view` 를 로드시키는지 확인이 필요합니다.

### Q2. 1픽셀 수신 컨트롤러 부착이 통합니까?  ★

```objc
sReceiver.view.frame = CGRectMake(0, 0, 1, 1);
sReceiver.view.alpha = 0.02;
[host.view addSubview:sReceiver.view];
[sReceiver startAdvertising];
```

**검증 앱은 전체 화면으로 `present` 했습니다.** 저는 파일 탐색기를 가리지
않으려고 1픽셀 자식으로 붙였습니다. 근거가 없습니다.

- `startAdvertising` 이 뷰의 실제 표시 여부에 의존합니까?
- 1픽셀·거의 투명 상태에서도 광고가 나갑니까?
- 이 뷰가 tvOS 포커스 엔진에 잡혀 파일 목록의 포커스를 훔칠 수 있습니까?
  (`userInteractionEnabled = NO` 로 막았습니다만 충분한지 모르겠습니다)

**만약 통하지 않는다면**: discoverable 모드만으로 수신이 되는지,
아니면 수신 전용 화면을 따로 띄워야 하는지 알려 주십시오.

### Q3. 모드 설정의 비동기 처리가 맞습니까?

`setDiscoverableMode:` 직후에 읽으면 **항상 0** 이 나오고 약 1초 뒤에 2가 됩니다.
그래서 1.6초 뒤에 확인합니다. 이 값이 충분합니까? 더 확실한 완료 신호가 있습니까?

### Q4. 1분 타이머와 전송 중 연장이 안전합니까?

받은 함을 2초마다 훑어 크기·개수가 바뀌면 시간을 다시 채웁니다.

- 전송이 **시작되기 전**(상대가 고르는 중)에는 파일이 안 생기므로 1분 안에
  꺼질 수 있습니다. 이게 문제입니까?
- 큰 파일 하나를 받는 중에는 크기가 계속 변하므로 연장됩니다. 맞습니까?
- `enumeratorAtPath` 를 2초마다 도는 비용이 받은 함이 커지면 문제가 됩니까?
  (4000개에서 끊도록 했습니다)

### Q5. 전체 화면 `present` 중에 부착된 수신 뷰는 어떻게 됩니까?

`attachReceiver` 는 **그 시점의** 최상위 뷰 컨트롤러에 붙습니다.
그 뒤 사용자가 문서 뷰어를 열면(시트) 수신 뷰는 가려진 아래쪽 화면에 남습니다.
광고가 계속 나갑니까, 아니면 끊깁니까?

---

## 1. 브리지 — 비공개 API 를 가두는 곳

"""

A += block("Spartan/AirDrop/AirDropBridge.h", lang="objc")
A += block("Spartan/AirDrop/AirDropBridge.m", lang="objc")

A += """**볼 곳**

- `sDiscovery` 는 static 으로 **한 번만** 만듭니다. 호출할 때마다 새로
  만들면 모드가 유지되지 않는다는 것이 검증 과정에서 확인됐습니다
- 모드 전이는 `Off → ContactsOnly → Everyone` 입니다. 일부 구현이 Off 에서
  곧장 Everyone 으로 가는 전이를 받지 않는다고 해서 한 단계를 거칩니다
- 모든 비공개 호출이 `@try/@catch` 안에 있는지
- `NSClassFromString` 이 nil 일 때 조용히 비활성화되는지 (오류 팝업 금지)

---

## 2. SwiftUI 쪽

"""

A += block("Spartan/AirDrop/AirDropState.swift")

A += """**볼 곳**

- **저장된 토글 값을 믿지 않습니다.** 모드는 디스크 설정이 아니라 실행 중인
  `sharingd` 의 상태라 재부팅하면 Off 로 돌아갑니다
- `@StateObject` 로 `ContentView` 가 들고 있습니다. 뷰가 다시 만들어질 때
  `init` 의 `isAvailable` 접근이 프레임워크를 다시 로드하지 않는지
  (`dispatch_once` 로 막았습니다)
- 실패했을 때 조용히 켜진 척하지 않고 말하는지

---

## 3. 툴바 버튼과 보내기 메뉴

"""

A += find("Spartan/ContentView.swift", "// AirDrop. 쓸 수 없는 기기에서는", before=1, after=27,
          label="Spartan/ContentView.swift (툴바의 AirDrop 버튼)")
A += find("Spartan/ContentView.swift", "canAirDrop: { index in", before=0, after=20,
          label="Spartan/ContentView.swift (길게 누르기 — 보내기)")

A += """**볼 곳**

- 쓸 수 없는 기기에서 버튼이 **아예 없는지** (비활성이 아니라 부재)
- 폴더를 보내려 하지 않는지 — tvOS 에서 폴더 AirDrop 은 동작이 불확실합니다
- 켜졌을 때 파란색(`Theme.airdropOn`)으로 바뀌고 남은 시간이 보이는지

---

## 4. 권한

```xml
<key>com.apple.private.airdrop.discovery</key><true/>
<key>com.apple.private.airdrop.settings</key><true/>
```

두 개만 추가했습니다. `no-sandbox`·`platform-application`·
`storage.AppDataContainers` 는 Argo 에 이미 있었습니다.

**볼 곳:** 이 두 키가 TrollStore 금지 목록에 없는지, 그리고 두 개로 충분한지.
""".replace("{SHA}", SHA)

write("VERIFY_A_AIRDROP.md", A.replace("{SHA}", SHA))


# ─────────────────────────────────────────────── B. 뷰어

B = """# 검증 B — PDF·텍스트 뷰어와 포커스 모델

커밋 `{SHA}`.

**이 문서의 ①번이 지금 가장 급한 문제입니다.**

---

## 답해 주셔야 할 질문

### Q1. PDF 가 왜 흰 화면으로 나옵니까?  ★★최우선★★

증상 (실기기, build 130):

- PDF 를 열면 **쪽 수가 읽힙니다** — 상태줄에 `1 / 1` 이 나옴
- **화면이 완전히 흽니다** — 글자도 선도 없음
- 상태줄에 `· 0%` 가 같이 나옴

이 세 가지를 코드와 맞춰 보면 이렇게 좁혀집니다.

1. `CGPDFDocument` 가 문서를 **열었습니다.** 못 열면 오류 화면이 뜨게 되어 있고,
   오류 화면은 어두운 패널입니다 — 흰 화면이 아닙니다
2. `· 0%` 는 `canScrollWithinPage` 가 참일 때만 나옵니다. 그 값은
   `renderedPageHeight > 화면 높이` 인데, `renderedPageHeight` 는
   **그리기가 끝난 뒤에** 대입됩니다
3. 따라서 **그리기 호출까지 갔고, 비트맵도 만들어졌는데, 잉크가 없습니다**

제가 한 조치는 **추측에 기반합니다.** 직접 만든 `CGBitmapContext` 에
`drawPDFPage` 하던 것을 `UIGraphicsImageRenderer` 로 바꿨습니다.
가장 널리 쓰이는 경로라서 바꾼 것이지, 원인을 알아서 바꾼 것이 아닙니다.

**봐 주실 것:**

- 아래 `renderCurrentPage` 에서 **좌표계 뒤집기와 `getDrawingTransform` 의
  조합**이 맞습니까? `UIGraphicsImageRenderer` 의 컨텍스트는 위에서 아래로
  갑니다. 그래서 `translateBy` + `scaleBy(1, -1)` 로 뒤집은 뒤
  `getDrawingTransform` 을 적용했습니다. **이 순서가 맞습니까?**
  혹시 `getDrawingTransform` 이 이미 뒤집기를 포함하고 있어 **두 번
  뒤집히는** 것은 아닙니까?
- 바꾸기 전의 `CGBitmapContext` 방식(뒤집기 없음)은 아래에 **함께 실어
  두었습니다.** 둘 중 어느 쪽이 맞습니까, 아니면 둘 다 틀립니까?
- `drawPDFPage` 가 조용히 아무것도 안 그리는 다른 조건이 있습니까?
  (암호·권한 제한·색 공간·누락된 폰트 등)

### Q2. 포커스 모델이 세 뷰어에서 맞습니까?  ★

툴바를 화면 **아래**로 옮긴 뒤에도 세 뷰어 모두 이 코드를 갖고 있었습니다.

```objc
// 도구 모음이 떠 있으면 위쪽은 거기로 가는 길이다.
if (type == UIPressTypeUpArrow && self.toolbarVisible) { return NO; }
```

툴바가 아래에 있으니 위쪽은 갈 곳이 없어 **아무 일도 일어나지 않았고**,
열자마자 활성화된 툴바가 **첫 포커스를 가져가** 화살표가 버튼 사이만
오갔습니다. 이것이 "리모컨으로 쪽 넘김이 안 된다"의 원인이었습니다.

지금은 **보이는 것과 입력을 받는 것을 나눴습니다.**

| 상태 | 툴바 | 포커스 | 화살표 |
| --- | --- | --- | --- |
| 열었을 때 | 보임 (옅게, 55%) | 캔버스 | 문서를 움직임 |
| 선택 버튼 | 보임 (진하게) | 툴바 | 버튼 사이 이동 |
| 메뉴 / 시간 초과 | 숨김 | 캔버스 | 문서를 움직임 |

**같은 수정을 세 곳에 했습니다. 한 곳이 틀리면 세 곳이 틀립니다.**

- 꺼진 버튼(`isEnabled = NO`)이 정말 포커스를 못 받습니까?
- `preferredFocusEnvironments` 를 바꾼 뒤 `setNeedsFocusUpdate` +
  `updateFocusIfNeeded` 를 부르는 것으로 충분합니까?
- 툴바 전체(`UIStackView`)를 포커스 환경으로 돌려주는 것이 맞습니까,
  아니면 그 안의 버튼 하나를 지목해야 합니까?

### Q3. 텍스트 뷰어가 실제로 스크롤됩니까?

`UITextView` 를 `isUserInteractionEnabled = false` 로 두고
`setContentOffset(_:animated:)` 으로만 움직입니다. 리모컨 입력은 위에 덮은
캔버스 버튼이 받습니다.

- 상호작용이 꺼진 텍스트 뷰에서 `contentSize` 가 제대로 계산됩니까?
- `attributedText` 를 넣은 직후 `contentSize` 를 읽으면 0 입니까?
  (`updateStatus` 가 그 시점에 불립니다)

### Q4. 8MB 상한과 인코딩 판정이 맞습니까?

`FileHandle.readData(ofLength:)` 로 앞 8MB 만 읽습니다. UTF-8 이 아니면
`NSString.stringEncoding(for:...)` 로 찍고, 그래도 안 되면 `isoLatin1` 로
손실을 감수하고 보여 줍니다. CP949 한글 로그가 이 경로로 읽힙니까?

**8MB 경계에서 멀티바이트 글자가 잘리면** 그 줄이 깨집니다. 이것을
다뤄야 합니까?

---

## 1. PDF 뷰어 — 지금 코드

"""

B += block("Spartan/FileOpeners/Readers/ArgoPDFReader.swift")

B += """---

## 2. 바꾸기 전의 렌더링 — 비교용

이쪽이 **흰 화면을 냈던** 코드입니다. 둘을 비교해 주십시오.

```swift
// 직접 만든 비트맵 컨텍스트. 원점이 왼쪽 아래라 PDF 와 같으므로
// 뒤집기를 하지 않았다.
let pixelScale = min(UIScreen.main.scale, 2.0)
let targetW = max(1, Int((pageW * scale * pixelScale).rounded()))
let targetH = max(1, Int((pageH * scale * pixelScale).rounded()))

guard let ctx = CGContext(data: nil,
                          width: targetW, height: targetH,
                          bitsPerComponent: 8,
                          bytesPerRow: 0,
                          space: CGColorSpaceCreateDeviceRGB(),
                          bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue) else { return }

ctx.setFillColor(gray: 1, alpha: 1)
ctx.fill(CGRect(x: 0, y: 0, width: targetW, height: targetH))

let target = CGRect(x: 0, y: 0, width: targetW, height: targetH)
ctx.concatenate(page.getDrawingTransform(.cropBox, rect: target,
                                         rotate: 0, preserveAspectRatio: true))
ctx.drawPDFPage(page)

guard let cg = ctx.makeImage() else { return }
pageImage.image = UIImage(cgImage: cg)
```

**이 코드에서 제가 못 찾은 결함이 있습니까?**

함께 고친 것이 하나 더 있습니다. `pageImage` 는 프레임으로 직접 놓는데
`translatesAutoresizingMaskIntoConstraints = false` 였고 제약이 하나도
없었습니다. 오토레이아웃이 그런 뷰를 0 크기로 되돌릴 수 있습니다.
지금은 `true` 입니다. **이것이 흰 화면의 원인일 수 있습니까?**
(제 생각에는 아닙니다 — 0 크기면 어두운 바탕이 보여야지 흰색이 아닙니다)

---

## 3. 텍스트·로그·plist 뷰어

"""

B += block("Spartan/FileOpeners/Readers/ArgoTextReader.swift")

B += """**볼 곳**

- 로그·코드·json 은 고정폭, 산문은 비례폭으로 가릅니다
- 바이너리 plist 를 XML 로 펴서 보여 줍니다
- 좌우 화살표는 **열 화면씩** 건너뜁니다 — 긴 로그에서 한 화면씩은 끝이 없습니다

---

## 4. 기존 문서 뷰어의 포커스 수정 (docx·xlsx·pptx·hwp)

같은 수정을 ObjC 쪽에도 했습니다.

"""

B += find("Spartan/FileOpeners/OfficeReader/WebDocumentView.m",
          "- (void)setToolbarVisible:(BOOL)visible animated:", before=3, after=50,
          lang="objc", label="Spartan/FileOpeners/OfficeReader/WebDocumentView.m (툴바 상태)")
B += find("Spartan/FileOpeners/OfficeReader/WebDocumentView.m",
          "- (NSArray<id<UIFocusEnvironment>> *)preferredFocusEnvironments", before=0, after=10,
          lang="objc", label="WebDocumentView.m (포커스 대상)")
B += find("Spartan/FileOpeners/OfficeReader/WebDocumentView.m",
          "- (BOOL)handlePress:(UIPressType)type", before=0, after=30,
          lang="objc", label="WebDocumentView.m (리모컨)")

B += """**볼 곳**

- 세 뷰어의 상태 전이가 **같은지** — 하나만 다르면 거기서 버그가 납니다
- 사진에서 docx 는 `1 / 2 쪽` 이 보였는데 넘어가지 않았습니다.
  JS 쪽 `nav.go()` 는 `u[k].scrollIntoView(true)` 를 부릅니다.
  이것이 WKWebView 안에서 동작합니까?
""".replace("{SHA}", SHA)

write("VERIFY_B_READERS.md", B.replace("{SHA}", SHA))


# ─────────────────────────────────────────────── C. 안전장치

C = """# 검증 C — 경로 권한 · 관문 · 삭제 · 휴지통

커밋 `{SHA}`.

이 묶음의 목적은 하나입니다. **파일 탐색기가 애플TV 를 벽돌로 만들지 않게
하는 것.** 그래서 판단 기준도 하나입니다 — **막아야 할 것을 막는가,
막으면 안 될 것을 막지는 않는가.**

여기서 한 번 크게 틀렸습니다. 아래 2장을 먼저 봐 주십시오.

---

## 답해 주셔야 할 질문

### Q1. 경로 경계표에서 틀린 줄을 찾아 주십시오  ★

### Q2. 관문을 지나지 않는 쓰기 경로가 있습니까?  ★

모든 파괴적 쓰기는 `FileGuard.authorize` 를 지나야 합니다.
`FileManager` 의 `removeItem` · `moveItem` · `copyItem` · `createFile` ·
`write` 와 `Data.write(to:)` 를 전수로 훑어 주십시오.

지금 관문을 지나는 곳:

| 위치 | 무엇 |
| --- | --- |
| `RootHelperActs.checkAuth` | rm · mv · cp · touch · mkdir · ln · chmod · mvtemp |
| `TrashManager` | 휴지통 보내기 · 비우기 · 되살리기 · 영구 삭제 |
| `Compression` | 압축 · 압축 풀기 |
| `PaneTransfer` | 칸 사이 복사·이동, 되돌리기 |
| `TextView` · `HexView` | 직접 저장 |
| `ContentView.requestDeleteSelection` | 일괄 삭제 |

**알려진 예외 하나**: IPA 설치는 `apple-magnifier://` 로 가고 관문을 안 거칩니다.
애플 자신의 설치 경로이고 우리가 파일을 쓰는 것이 아니라 의도한 것입니다.

**새로 생긴 것 하나**: `FilePaneModel.writeCacheIfWritable` 은 관문 대신
`PathSafety.isWriteAllowed` 를 직접 봅니다. 캐시는 편의이지 사용자 작업이
아니라서 차단 알림을 띄우지 않으려고 그렇게 했습니다. **이 판단이 맞습니까?**

### Q3. 과차단은 없습니까?

막으면 안 될 것을 막으면 앱이 쓸모없어집니다. 특히:

- `/var/mobile/` 아래가 전부 통과합니까?
- **심볼릭 링크 자체를 지우는 것**이 막히지 않습니까?
  (`effectivePathForWriteCheck` 가 링크의 **부모**를 보는 이유입니다 —
  이게 없으면 시스템을 가리키는 링크를 치울 수조차 없습니다)
- 안전 모드를 끄면 `.caution` 이 통과합니까?

### Q4. 안전 범위를 `/var/mobile` 전체로 넓힌 것이 맞습니까?

원본 Spartan 의 권한 모델은 이것뿐이었습니다.

```swift
func filePathIsNotMobileWritable(_ fullPath: String) -> Bool {
    return ((fullPath.count < 19) || (fullPath.substring(toIndex: 19) != "/private/var/mobile"))
}
```

`/private/var/mobile` 아래 전부 쓰기 가능이고, **쓰인 곳은 텍스트·헥스·plist
편집기 세 곳뿐**이었습니다. **삭제·이동·복사에는 경로 제한이 아예 없었습니다.**

저는 처음에 `Media`·`Documents`·`Downloads` 셋만 열었다가, 그러면
`Library/` 나 사용자가 만든 폴더가 전부 '주의'가 되어 자기 파일도 못 지우는
것을 보고 **원본과 같은 `/var/mobile` 전체**로 넓혔습니다.

- 이 범위가 적절합니까? `/var/mobile/Library/Preferences` 를 지우는 것이
  위험합니까? (벽돌은 아니지만 설정이 날아갑니다)
- 반대로 `/var/mobile/Containers` 처럼 더 막아야 할 하위 경로가 있습니까?

### Q5. 삭제가 한 길로만 갑니까?

두 번 망가졌던 곳입니다.

1. 미리보기는 **길게 누른 파일**을 보여 주는데 실제 삭제는
   `multiSelectFiles`(여러 개 고르기 목록)를 돌았습니다. 한 개를 길게 눌러
   지우면 그 배열이 비어 있어 **아무 일도 일어나지 않았습니다**
2. 툴바의 일괄 삭제 버튼은 미리보기를 띄우면서 지울 목록을 **채우지
   않아** `지울 것 0개` 가 떴습니다

지금은 `pendingDeletePaths` 한 곳에 담고 미리보기와 삭제가 같은 것을 봅니다.
**다른 경로가 남아 있지 않은지** 봐 주십시오.

### Q6. R2 는 함수를 지우는 것으로 끝났습니다 — 맞습니까?

`TrashManager.unlinkNestedSymlinksIfDirectory` 는 폴더를 지우기 전에 안의
심볼릭 링크를 미리 끊던 함수입니다. **지웠습니다.**

근거:
- 재귀 삭제가 링크를 따라가지 않는 것은 POSIX 전반의 동작입니다
  (`rm -rf`, `fts`(`FTS_PHYSICAL`) 모두 링크를 끊고 지나갑니다)
- 공개 소스 동급 앱(`NSAntoine/Santander`)은 루트 헬퍼까지 갖추고도
  `try FileManager.default.removeItem(at:)` 한 줄이 전부입니다
- 어느 쪽이든 지우는 것이 안전합니다 — 만약 정말 따라간다 해도 결과는
  원본 Spartan 과 같아져 전보다 나빠지지 않습니다. 반면 남겨 두면
  **삭제·이동 실패 시 멀쩡한 폴더의 링크만 영구히 사라집니다**

**이 논증에 구멍이 있습니까?** 특히 `FileManager.removeItem` 이 내부적으로
`removefile(3)` 을 쓸 때의 기본 동작을 확인해 주십시오.

---

## 1. 경로 위험도

"""

C += block("Spartan/Safety/PathSafety.swift")

C += """---

## 2. 지금 규칙으로 뽑은 경계표  ★

위 코드를 그대로 흉내내 돌린 결과입니다. **틀린 줄을 찾아 주십시오.**

| 경로 | 판정 |
| --- | --- |
| `/private/var/mobile/Media/Radio/` | safe |
| `/var/mobile/Media/x.pdf` | safe |
| `/private/var/mobile/Library/` | safe |
| `/var/mobile/Downloads/a.log` | safe |
| `/var/mobile/Documents/` | safe |
| `/private/var/mobile/` | safe |
| `/var/mobile` | safe |
| `/var/tmp/`, `/private/var/tmp/z` | safe |
| `/VAR/MOBILE/Media/x` | safe (대소문자 무시) |
| `/var/mobileXYZ/evil` | **caution** (접두사 충돌 아님) |
| `/var/log/` | caution |
| `/System/Library/` | forbidden |
| `/private/var/db/x` | forbidden |
| `/var/containers/Bundle/` | forbidden |
| `/Applications/` | forbidden |
| `/`, `/var`, `/private/var` | forbidden |

**고치기 전에는 이 표의 safe 가 전부 caution 이었습니다.**
`resolvingSymlinksInPath()` 가 `/private` 접두사를 떼어내는데 목록은
`/private/var/...` 로 적혀 있어 **하나도 안 맞았기** 때문입니다.
안전 모드가 기본 켜짐이라 결과적으로 **모든 쓰기가 막혔습니다.**

---

## 3. 관문

"""

C += block("Spartan/Safety/FileGuard.swift")

C += """**볼 곳**

- **`refuseUncheckable`** — 검사할 수 없으면 **막습니다.** 전에는 성공을
  돌려줘 그대로 풀렸습니다. 검사는 libarchive, 해제는 minizip 이라
  **한쪽이 거부하고 다른 쪽이 받아들이는 파일 하나로 검사가 통째로
  우회됐습니다**
- **`ARCHIVE_EOF`** — `ARCHIVE_OK` 는 0, `EOF` 는 1, `WARN` 은 -20 입니다.
  전에는 정상 종료와 손상을 구분하지 않아 50개 중 3번째에서 경고가 나면
  나머지 47개가 검사 없이 풀렸습니다
- **하드링크** — `archive_entry_symlink` 와 `archive_entry_hardlink` 는
  다른 필드입니다. 심볼릭 링크만 보면 같은 우회가 이름만 바꿔 남습니다
- **`checkFreeSpace`** — 읽지 못하면 **막지 않습니다.** 이것은 안전 검사가
  아니라 편의 검사이기 때문입니다. 압축 검사와 실패 방향이 **반대**인데
  이 구분이 맞습니까?

---

## 4. 휴지통

"""

C += block("Spartan/Trash/TrashManager.swift")

C += """**볼 곳**

- 모든 공개 진입점이 관문을 먼저 지나는가
- 되살리기가 원래 경로를 복원할 때도 관문을 지나는가
- 휴지통 자체(`.Trash`)가 `.caution` 으로 분류되어 비우기가 막히지는 않는가

---

## 5. 삭제 경로

"""

C += find("Spartan/ContentView.swift", "func requestDeleteSelection()", before=2, after=22,
          label="ContentView.swift (삭제 요청)")
C += find("Spartan/ContentView.swift", "func executeBatchDelete()", before=0, after=26,
          label="ContentView.swift (실제 삭제)")

C += """---

## 6. 작업 기록

"""

C += block("Spartan/Safety/OperationLog.swift")

write("VERIFY_C_SAFETY.md", C.replace("{SHA}", SHA))


# ─────────────────────────────────────────────── D. 탐색기

D = """# 검증 D — 두 칸 · 키보드 · 클립보드 · 전송 · 압축

커밋 `{SHA}`. **전부 빌드만 통과했고 기기에서 확인되지 않았습니다.**

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

"""

D += block("Spartan/Panes/FilePaneModel.swift")

D += """**볼 곳**

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

"""

D += block("Spartan/Panes/FilePaneView.swift")

D += """**볼 곳**

- **행당 포커스 버튼은 하나입니다.** tvOS 규칙이고 깨뜨리면 목록을 못 씁니다
- 파일명에 **색을 못 박지 않습니다.** tvOS 가 포커스된 줄의 바탕을 흰색으로
  바꾸면서 글자색을 같이 뒤집는데, 흰색을 지정했더니 포커스된 줄의 이름이
  사라졌습니다
- `FileGlyph` 에 빠진 파일 종류가 없는지 (0·1·2·3·4·6·7·8·9·10·11·12·13·14·15·16 —
  **5가 없는 것이 원본과 같습니다**)

---

## 3. 두 칸 라우팅과 계산 속성

"""

D += span("Spartan/ContentView.swift", 23, 95,
          label="ContentView.swift (칸 보유와 계산 속성)")
D += find("Spartan/ContentView.swift", "func openInPane(", before=3, after=24,
          label="ContentView.swift (항목 열기)")
D += find("Spartan/ContentView.swift", "func copySelectionToOtherPane(", before=2, after=40,
          label="ContentView.swift (반대 칸으로)")

D += """---

## 4. 클립보드

"""

D += block("Spartan/Panes/ArgoClipboard.swift")
D += find("Spartan/ContentView.swift", "func clipboardTake(mode:", before=6, after=50,
          label="ContentView.swift (담기와 붙여넣기)")

D += """---

## 5. 전송 · 되돌리기

"""

D += block("Spartan/Panes/PaneTransfer.swift")

D += """---

## 6. 압축 — 뒷줄 처리

"""

D += block("Spartan/FileOpeners/Compression/CompressionRunner.swift")

D += """**볼 곳**

- 진행률 막대가 **없습니다.** libarchive·minizip 이 도는 중에 아무것도
  알려주지 않아 측정할 수 없고, 측정 못 하는 것에 가짜 퍼센트를 붙이지
  않는다는 판단입니다
- 취소가 "다음 파일로 넘어가지 않게 하는 것"까지만 하고, 화면에도 그렇게
  적혀 있습니다

---

## 7. 키보드

"""

D += block("Spartan/Panes/ArgoKeyboard.swift")

D += """**볼 곳**

- `UIKeyCommand(input:modifierFlags:action:)` 를 씁니다.
  **iOS 13 의 `(title:action:input:modifierFlags:)` 는 tvOS 에 없습니다**
- 맨 `Tab` 은 포커스 엔진이 쓰므로 칸 전환을 `⌥Tab` 으로 옮겼습니다
- `ArgoTextEntry` 는 깊이로 셉니다 — 경로 칸에 포커스가 있는 채로 시트가
  뜰 수 있어, 참·거짓으로 두면 시트를 닫을 때 먼저 켜 둔 것까지 풀립니다
"""

write("VERIFY_D_BROWSER.md", D.replace("{SHA}", SHA))

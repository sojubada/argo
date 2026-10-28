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
    /// 지금 칸의 목록을 이름·확장자로 거른다.
    case filterList
    case search
    case showHelp
    case goHome
    case goRoot
    case undo
    /// ⌫. 되돌아갈 기록이 있으면 뒤로, 없으면 한 칸 위로.
    ///
    /// 막 켰을 때는 기록이 비어 있다. 그때 ⌫ 가 아무 일도 안 하면 키가
    /// 고장 난 것처럼 보인다.
    case goBackOrUp

    static let notification = Notification.Name("ArgoKeyCommandNotification")

    /// 키 자리로 가려 pressesBegan 이 직접 처리하는 명령.
    /// UIKeyCommand 로는 등록하지 않는다.
    static let pressOnly: Set<ArgoCommand> = [.deleteSelection, .goBackOrUp]

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
        case .deleteSelection:   return "휴지통으로 (⌦ 또는 ⌘⌦)"
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
        case .focusPathField:    return "경로로 이동"
        case .filterList:        return "걸러보기 (이름·확장자)"
        case .search:            return "찾기"
        case .showHelp:          return "단축키 도움말"
        case .goHome:            return "미디어 폴더로"
        case .goRoot:            return "최상위로 (⌘⌫ 도 같음)"
        case .undo:              return "되돌리기 (이동·이름)"
        case .goBackOrUp:        return "이전 폴더로 (기록이 없으면 위로)"
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
        case "\u{7f}": out += "⌦"
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
        // 표시용이다. 실제로는 pressesBegan 이 키 자리로 받는다(pressOnly).
        case .deleteSelection:   return ("\u{7f}", [])
        case .goBackOrUp:        return ("\u{8}", [])
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
        case .filterList:        return ("f", [.command, .shift])
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
        ("이동", [.goBackOrUp, .goUp, .goBack, .goForward, .goHome, .goRoot, .focusPathField, .refresh]),
        ("복사·이동", [.clipboardCopy, .clipboardCut, .clipboardPaste]),
        ("두 칸", [.toggleDualPane, .switchPane, .copyToOtherPane, .moveToOtherPane, .syncPanes]),
        ("고르기", [.toggleMultiSelect, .selectAll]),
        ("파일", [.newFolder, .newFile, .rename, .fileInfo, .deleteSelection, .compress, .extractArchive, .undo]),
        ("보기", [.toggleHidden, .sortByName, .sortBySize, .sortByDate, .sortByKind, .reverseSort, .search, .filterList]),
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
        // 지우기는 UIKeyCommand 로 등록하지 않는다. 키 자리(keyCode)로
        // pressesBegan 에서 가린다 — 등록해 두면 누름보다 먼저 가로채인다.
        for command in allCommands where !ArgoCommand.pressOnly.contains(command) {
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
        // 글자를 받는 칸이 떠 있으면 단축키는 돌지 않는다.
        //
        // keyCommands 에서 nil 을 돌려주는 것만으로는 부족하다. UIKit 은 그
        // 목록을 미리 받아 두고 쓰므로, 칸이 열린 뒤에도 먼저 등록해 둔
        // 조합이 그대로 살아 있을 수 있다. 받는 쪽에서 한 번 더 막는다 —
        // 글자를 지우려고 ⌫ 를 눌렀는데 파일이 지워지면 안 된다.
        guard !ArgoTextEntry.isActive else { return }
        guard let input = sender.input else { return }
        let flags = sender.modifierFlags
        guard let matched = allCommands.first(where: { command in
            let (i, f) = ArgoCommand.binding(for: command)
            return i == input && f == flags
        }) else { return }

        ArgoCommandPoster.post(matched)
    }

    // MARK: - 글자 입력 점프

    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        // 글자를 받는 칸이 떠 있거나, 목록 위에 다른 창이 덮여 있으면 목록용
        // 키를 쓰지 않는다.
        //
        // 덮인 창에서 처리되지 않은 누름은 리스폰더 체인을 타고 여기까지
        // 올라온다. 문서 뷰어에서 누른 ⌦ 가 뒤에 깔린 목록의 줄을 지우거나,
        // 삭제 확인 창에서 누른 스페이스가 '누름' 대신 고르기로 먹히면 안 된다.
        guard !ArgoTextEntry.isActive, presentedViewController == nil else {
            super.pressesBegan(presses, with: event)
            return
        }

        var handled = false
        var toggledSelection = false
        var command: ArgoCommand?
        for press in presses {
            guard let key = press.key else { continue }
            let chars = key.charactersIgnoringModifiers
            // 캡스록·숫자판 표시는 조합으로 치지 않는다. 그것까지 보면
            // 캡스록을 켠 채로는 글자 점프도 스페이스도 먹지 않는다.
            let mods = key.modifierFlags.intersection([.command, .shift, .alternate, .control])

            // 지우기 두 개는 글자가 아니라 키 자리로 가린다.
            //
            // ⌫(백스페이스)는 지우기가 아니라 이동이다 — 그냥 누르면 이전
            // 폴더로, ⌘ 와 함께면 최상위로. ⌦(딜리트)만 지우기이고, 그것도
            // 휴지통으로 보내는 확인 창을 거친다.
            //
            // 전에는 ⌘⌫ 가 지우기였다. 글자를 고치려고 누르는 바로 그 키가
            // 파일을 지우는 키였고, 실제로 폴더 하나가 그렇게 사라졌다.
            switch key.keyCode {
            case .keyboardDeleteOrBackspace:
                if mods == .command { command = .goRoot }
                else if mods.isEmpty { command = .goBackOrUp }
                continue
            case .keyboardDeleteForward:
                if mods.isEmpty || mods == .command { command = .deleteSelection }
                continue
            default:
                break
            }

            // 수정키가 걸린 입력은 단축키 쪽이 처리한다. 다만 입력 언어가
            // 한국어면 UIKeyCommand 가 맞지 않아 여기로 떨어진다 — 키 자리로
            // 미국 배열 글자를 얻어 단축키 표에서 다시 찾는다.
            if !mods.isEmpty {
                let shifted = ArgoKeyMap.latin(key.keyCode, shift: false)
                if let input = shifted,
                   let found = allCommands.first(where: { c in
                       guard !ArgoCommand.pressOnly.contains(c) else { return false }
                       let (i, f) = ArgoCommand.binding(for: c)
                       return i == input && f == mods
                   }) {
                    command = found
                } else if key.keyCode == .keyboardUpArrow, mods == .command {
                    command = .goUp
                } else if key.keyCode == .keyboardTab, mods == .alternate {
                    command = .switchPane
                } else if key.keyCode == .keyboardReturnOrEnter, mods == .command {
                    command = .rename
                }
                continue
            }
            guard chars.count == 1 else { continue }

            // 스페이스는 고르기다. 포커스 엔진으로 넘기지 않는다 —
            // 넘기면 포커스된 줄이 눌려서 파일이 열린다.
            if chars == " " {
                toggledSelection = true
                continue
            }

            guard let scalar = chars.unicodeScalars.first,
                  CharacterSet.alphanumerics.contains(scalar) else { continue }

            typeAheadBuffer += chars
            handled = true
            restartTypeAheadTimer()
        }

        if let command {
            ArgoCommandPoster.post(command)
        }
        if toggledSelection {
            NotificationCenter.default.post(name: ArgoSelectToggle.notification, object: nil)
        }
        if handled {
            NotificationCenter.default.post(name: ArgoTypeAhead.notification,
                                            object: typeAheadBuffer)
        }
        if !handled && !toggledSelection && command == nil {
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

/// 스페이스로 고르기.
///
/// 수정키가 없는 스페이스라 단축키 표에 둘 수 없습니다 — 포커스 엔진이
/// 스페이스를 '누름'으로 쓰기 때문에, UIKeyCommand 로 잡으면 목록에서
/// 파일을 여는 길과 다투게 됩니다. pressesBegan 에서 먼저 받아 처리하고
/// 엔진으로 넘기지 않습니다. 여는 것은 엔터와 리모컨 선택 버튼이 맡습니다.
enum ArgoSelectToggle {
    static let notification = Notification.Name("ArgoSelectToggleNotification")
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

/// 키 자리(HID 번호) → 미국 배열 글자.
///
/// 시스템 입력 언어가 한국어면 key.characters 가 'ㄴ' 처럼 자모로 온다.
/// 그러면 ⌘S 가 '⌘ㄴ' 이 되어 단축키가 하나도 맞지 않고, 편집기에는 조합되지
/// 않은 자모가 낱낱이 들어갔다. 키 자리는 입력 언어와 상관없이 같으므로,
/// 단축키와 글자 입력을 모두 키 자리에서 읽는다.
enum ArgoKeyMap {
    private static let digits = Array("1234567890")
    private static let digitsShifted = Array("!@#$%^&*()")
    // 0x2D 부터 키 자리 순서. 0x32 는 미국 배열에 없는 자리라 '\' 로 둔다.
    private static let punct = Array("-=[]\\\;'`,./")
    private static let punctShifted = Array("_+{}||:\"~<>?")

    static func latin(_ code: UIKeyboardHIDUsage, shift: Bool) -> String? {
        let v = code.rawValue
        switch v {
        case 0x04...0x1D:
            let base = Int((shift ? "A" : "a").unicodeScalars.first!.value)
            guard let u = UnicodeScalar(base + (v - 0x04)) else { return nil }
            return String(Character(u))
        case 0x1E...0x27:
            return String((shift ? digitsShifted : digits)[v - 0x1E])
        case 0x2C:
            return " "
        case 0x2D...0x38:
            let table = shift ? punctShifted : punct
            let i = v - 0x2D
            return i < table.count ? String(table[i]) : nil
        default:
            return nil
        }
    }
}

/// 두 갈래로 같은 단축키가 들어올 때 한 번만 쓰게 한다.
///
/// UIKeyCommand 가 맞으면 그쪽으로, 입력 언어 때문에 맞지 않으면 pressesBegan
/// 에서 키 자리로 다시 찾는다. 같은 누름이 양쪽에서 잡히는 일이 있어도
/// ⌘2 가 두 번 돌아 두 칸이 켰다 꺼지면 안 된다.
enum ArgoCommandPoster {
    private static var last: (ArgoCommand, Date)?

    static func post(_ command: ArgoCommand) {
        if let l = last, l.0 == command, Date().timeIntervalSince(l.1) < 0.25 { return }
        last = (command, Date())
        NotificationCenter.default.post(name: ArgoCommand.notification, object: command.rawValue)
    }
}

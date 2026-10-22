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
        // 맨 Tab 은 포커스 엔진이 쓰는 키다. 수정키 없이 가로채면
        // 칸 사이 이동이 깨질 수 있어 ⌥ 를 붙였다.
        case .switchPane:        return ("\t", .alternate)
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

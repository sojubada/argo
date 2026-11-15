//
//  RemoteKeys.swift
//  Argo
//
//  WebDAV·FTP 탐색 화면의 키보드 단축키.
//
//  로컬 목록의 단축키(ArgoKeyboard)는 '목록 위에 아무 창도 없을 때'만 돈다. WebDAV 화면은
//  전체 화면 창(fullScreenCover)이라 그 조건에 걸려 키가 하나도 먹지 않았다.
//  그래서 WebDAV 탐색 화면이 맨 위에 있을 때만 ArgoHostingController 가 키를 여기로 넘기고,
//  화면은 알림으로 받아 처리한다. 키는 로컬 목록과 같은 자리에 둔다 — 손이 기억하는 키 그대로.
//
//  키는 UIKeyCommand 로 등록하지 않고 키 자리(keyCode)로 읽는다. 입력 언어가 한국어면
//  UIKeyCommand 가 '⌘ㄴ' 처럼 와서 맞지 않기 때문이다(로컬 목록에서 겪은 문제).
//

import UIKit

enum ArgoRemoteCommand: String, CaseIterable {
    case goBackOrUp, goRoot, goUp, refresh
    case toggleSelect, toggleMultiSelect, selectAll
    case copy, cut, paste
    case download, compress
    case info, help

    var title: String {
        switch self {
        case .goBackOrUp:        return L("이전 폴더로")
        case .goRoot:            return L("서버 최상위로")
        case .goUp:              return L("한 칸 위로")
        case .refresh:           return L("새로 읽기")
        case .toggleSelect:      return L("지금 줄 고르기·풀기")
        case .toggleMultiSelect: return L("여러 개 고르기 켜고 끄기")
        case .selectAll:         return L("전부 고르기·풀기")
        case .copy:              return L("복사 (서버 안)")
        case .cut:               return L("잘라내기 (서버 안 이동)")
        case .paste:             return L("붙여넣기 (이 폴더에)")
        case .download:          return L("받기 (애플TV 받는 폴더로)")
        case .compress:          return L("받아서 zip 으로 압축")
        case .info:              return L("정보·설치")
        case .help:              return L("단축키 도움말")
        }
    }

    /// 도움말에 보이는 키. 아래 command(for:) 와 같은 표다.
    var keys: String {
        switch self {
        case .goBackOrUp:        return "⌫"
        case .goRoot:            return "⌘⌫"
        case .goUp:              return "⌘↑"
        case .refresh:           return "⌘R"
        case .toggleSelect:      return "Space"
        case .toggleMultiSelect: return "⌘M"
        case .selectAll:         return "⌘A"
        case .copy:              return "⌘C"
        case .cut:               return "⌘X"
        case .paste:             return "⌘V"
        case .download:          return "⌘D, ⌘⌥C"
        case .compress:          return "⌘⇧Z"
        case .info:              return "⌘I"
        case .help:              return "⌘/"
        }
    }
}

enum ArgoRemoteKeys {
    /// WebDAV 탐색 화면이 떠 있는가(화면이 켜고 끈다).
    static var active = false
    static let notification = Notification.Name("ArgoRemoteCommandNotification")

    static func post(_ command: ArgoRemoteCommand) {
        NotificationCenter.default.post(name: notification, object: command.rawValue)
    }

    /// 키 하나 → 명령. 수정키는 ⌘·⇧·⌥·⌃ 만 본다(캡스록 무시).
    static func command(for key: UIKey) -> ArgoRemoteCommand? {
        let m = key.modifierFlags.intersection([.command, .shift, .alternate, .control])
        switch key.keyCode {
        case .keyboardDeleteOrBackspace:
            if m.isEmpty { return .goBackOrUp }
            if m == .command { return .goRoot }
        case .keyboardUpArrow:
            if m == .command { return .goUp }
        case .keyboardSpacebar:
            if m.isEmpty { return .toggleSelect }
        case .keyboardR:
            if m == .command { return .refresh }
        case .keyboardM:
            if m == .command { return .toggleMultiSelect }
        case .keyboardA:
            if m == .command { return .selectAll }
        case .keyboardC:
            if m == .command { return .copy }
            if m == [.command, .alternate] { return .download }
        case .keyboardX:
            if m == .command { return .cut }
        case .keyboardV:
            if m == .command { return .paste }
        case .keyboardD:
            if m == .command { return .download }
        case .keyboardZ:
            if m == [.command, .shift] { return .compress }
        case .keyboardI:
            if m == .command { return .info }
        case .keyboardSlash:
            if m == .command || m == [.command, .shift] { return .help }
        default:
            break
        }
        return nil
    }

    static var helpText: String {
        ArgoRemoteCommand.allCases.map { "\($0.keys)   \($0.title)" }.joined(separator: "\n")
    }
}

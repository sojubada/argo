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
        case .copy: return L("복사")
        case .move: return L("이동")
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
        return LF("{0} 외 {1}개 {2}", name, paths.count - 1, mode.label)
    }

    func take(_ newPaths: [String], mode: ClipboardMode) {
        paths = newPaths
        self.mode = mode
    }

    func clear() {
        paths = []
    }
}

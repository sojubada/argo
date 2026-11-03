//
//  MediaKeys.swift
//  Argo
//
//  재생 화면의 리모컨·키보드 입력을 UIKit 단에서 확실히 받는다.
//
//  SwiftUI 의 onPlayPauseCommand / onExitCommand 는 포커스가 그 뷰 안에 있을 때만
//  불린다. 시트로 띄운 재생 화면에서는 포커스가 엇나가면 리모컨의 재생·멈춤과
//  키보드가 아무 데도 닿지 않았다. 여기서는 화면 전체를 UIHostingController
//  하위 클래스로 감싸, 포커스된 단추가 처리하지 않은 누름이 반드시 이리로 올라오게 한다.
//
//  키 배치
//    리모컨  재생/멈춤 → 재생·멈춤,  Menu → 닫기(가사 화면이면 가사 닫기)
//    키보드  Space·K → 재생·멈춤,  Esc → 닫기,  J/L → 10초 뒤/앞,
//            N → 다음 곡,  P → 이전 곡
//

import SwiftUI
import UIKit
import AVKit

struct MediaKeyHandlers {
    var playPause: () -> Void = {}
    var close: () -> Void = {}
    var seek: (Double) -> Void = { _ in }
    var next: () -> Void = {}
    var previous: () -> Void = {}
}

final class MediaKeyHostingController<Content: View>: UIHostingController<Content> {
    var handlers = MediaKeyHandlers()

    /// 이 누름을 우리가 처리하는가 (처리하면 단추로 넘기지 않는다).
    private func action(for press: UIPress) -> (() -> Void)? {
        switch press.type {
        case .playPause:
            return handlers.playPause
        case .menu:
            return handlers.close
        default:
            break
        }
        guard let key = press.key else { return nil }
        let mods = key.modifierFlags.intersection([.command, .shift, .alternate, .control])
        guard mods.isEmpty else { return nil }
        switch key.keyCode {
        case .keyboardSpacebar, .keyboardK, .keyboardPause:
            return handlers.playPause
        case .keyboardEscape:
            return handlers.close
        case .keyboardJ:
            return { [handlers] in handlers.seek(-10) }
        case .keyboardL:
            return { [handlers] in handlers.seek(10) }
        case .keyboardN:
            return handlers.next
        case .keyboardP:
            return handlers.previous
        default:
            return nil
        }
    }

    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        var rest = Set<UIPress>()
        for press in presses {
            if let run = action(for: press) {
                run()
            } else {
                rest.insert(press)
            }
        }
        if !rest.isEmpty {
            super.pressesBegan(rest, with: event)
        }
    }

    override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        let rest = presses.filter { action(for: $0) == nil }
        if !rest.isEmpty {
            super.pressesEnded(rest, with: event)
        }
    }
}

/// SwiftUI 화면을 키를 받는 호스트에 넣는다.
struct MediaKeyHost<Content: View>: UIViewControllerRepresentable {
    var handlers: MediaKeyHandlers
    var content: Content

    init(handlers: MediaKeyHandlers, @ViewBuilder content: () -> Content) {
        self.handlers = handlers
        self.content = content()
    }

    func makeUIViewController(context: Context) -> MediaKeyHostingController<Content> {
        let vc = MediaKeyHostingController(rootView: content)
        vc.handlers = handlers
        vc.view.backgroundColor = .clear
        return vc
    }

    func updateUIViewController(_ vc: MediaKeyHostingController<Content>, context: Context) {
        vc.rootView = content
        vc.handlers = handlers
    }
}

// MARK: - 동영상

/// 애플TV 기본 재생 화면. 리모컨(재생·멈춤, 훑기, ±10초, 정보 패널)은 기본 그대로
/// 쓰고, 키보드 단축키만 더한다.
final class ArgoAVPlayerController: AVPlayerViewController {
    var onClose: (() -> Void)?

    private func toggle() {
        guard let player else { return }
        if player.rate != 0 { player.pause() } else { player.play() }
    }

    private func seek(_ delta: Double) {
        guard let player else { return }
        let now = player.currentTime().seconds
        let end = player.currentItem?.duration.seconds ?? .infinity
        let target = max(0, min(end.isFinite ? end : now + delta, now + delta))
        player.seek(to: CMTime(seconds: target, preferredTimescale: 600))
    }

    private func keyAction(_ press: UIPress) -> (() -> Void)? {
        guard let key = press.key else { return nil }
        let mods = key.modifierFlags.intersection([.command, .shift, .alternate, .control])
        guard mods.isEmpty else { return nil }
        switch key.keyCode {
        case .keyboardSpacebar, .keyboardK, .keyboardPause:
            return { [weak self] in self?.toggle() }
        case .keyboardJ:
            return { [weak self] in self?.seek(-10) }
        case .keyboardL:
            return { [weak self] in self?.seek(10) }
        case .keyboardEscape:
            return { [weak self] in self?.player?.pause(); self?.onClose?() }
        default:
            return nil
        }
    }

    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        var rest = Set<UIPress>()
        for p in presses {
            if let run = keyAction(p) { run() } else { rest.insert(p) }
        }
        if !rest.isEmpty { super.pressesBegan(rest, with: event) }
    }

    override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        let rest = presses.filter { keyAction($0) == nil }
        if !rest.isEmpty { super.pressesEnded(rest, with: event) }
    }
}

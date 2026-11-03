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

extension MediaKeyHandlers {
    /// 이 누름을 재생 화면이 처리하는가. 처리하면 돌릴 일을 준다.
    func action(for press: UIPress) -> (() -> Void)? {
        switch press.type {
        case .playPause:
            return playPause
        case .menu:
            return close
        default:
            break
        }
        guard let key = press.key else { return nil }
        let mods = key.modifierFlags.intersection([.command, .shift, .alternate, .control])
        guard mods.isEmpty else { return nil }
        switch key.keyCode {
        case .keyboardSpacebar, .keyboardK, .keyboardPause:
            return playPause
        case .keyboardEscape:
            return close
        case .keyboardJ:
            return { [seek] in seek(-10) }
        case .keyboardL:
            return { [seek] in seek(10) }
        case .keyboardN:
            return next
        case .keyboardP:
            return previous
        default:
            return nil
        }
    }
}

/// 지금 떠 있는 재생 화면의 키 처리. 안쪽 호스트가 그릴 때마다 적어 두고,
/// 바깥 틀(ArgoMediaContainer)이 포커스 없이 올라온 누름을 이것으로 처리한다.
enum MediaKeyRouter {
    static var active: MediaKeyHandlers?
}

final class MediaKeyHostingController<Content: View>: UIHostingController<Content> {
    var handlers = MediaKeyHandlers()

    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        var rest = Set<UIPress>()
        for press in presses {
            if let run = handlers.action(for: press) {
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
        let rest = presses.filter { handlers.action(for: $0) == nil }
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
        MediaKeyRouter.active = handlers
        vc.view.backgroundColor = .clear
        return vc
    }

    func updateUIViewController(_ vc: MediaKeyHostingController<Content>, context: Context) {
        vc.rootView = content
        vc.handlers = handlers
        MediaKeyRouter.active = handlers
    }
}

// MARK: - 동영상

/// 애플TV 기본 재생 화면. 리모컨(재생·멈춤, 훑기, ±10초, 정보 패널)은 기본 그대로
/// 쓰고, 키보드 단축키만 더한다.
final class ArgoAVPlayerController: AVPlayerViewController {
    var onClose: (() -> Void)?

    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        // Menu 로 닫혔을 때도 멈추고, 부른 쪽 상태를 되돌린다.
        if isBeingDismissed || presentingViewController == nil {
            player?.pause()
            onClose?()
            onClose = nil
        }
    }

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
        // 리모컨 재생·멈춤 단추는 여기서 직접 처리한다. 위로 올려 보내면
        // 목록 화면의 '음악 화면 열기'(onPlayPauseCommand)가 가져간다.
        if press.type == .playPause {
            return { [weak self] in self?.toggle() }
        }
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
            return { [weak self] in self?.player?.pause(); self?.dismiss(animated: true) }
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

// MARK: - 띄우기

/// 재생 화면을 SwiftUI 시트가 아니라 UIKit 으로 직접 띄운다.
///
/// 시트 안에 넣으면 리모컨 누름이 시트 밖(목록 화면)의 onPlayPauseCommand ·
/// onExitCommand 로 먼저 새어 나갔다. 동영상의 기본 재생 화면조차 재생·멈춤을
/// 받지 못했던 이유다. 화면 가득 직접 띄우면 누름이 재생 화면에 먼저 닿는다.
final class ArgoMediaContainer: UIHostingController<AnyView> {
    var onClose: (() -> Void)?

    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        // 안쪽 호스트가 처리하지 못하고 올라온 누름(포커스된 단추가 없을 때 등).
        var rest = Set<UIPress>()
        for p in presses {
            if let run = MediaKeyRouter.active?.action(for: p) { run() } else { rest.insert(p) }
        }
        if !rest.isEmpty { super.pressesBegan(rest, with: event) }
    }

    override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        let rest = presses.filter { MediaKeyRouter.active?.action(for: $0) == nil }
        if !rest.isEmpty { super.pressesEnded(rest, with: event) }
    }

    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        if isBeingDismissed || presentingViewController == nil {
            MediaKeyRouter.active = nil
            onClose?()
            onClose = nil
        }
    }
}

enum ArgoMediaPresenter {
    private static weak var current: UIViewController?

    /// 재생 화면이 떠 있는가 (목록 화면이 누름을 위로 흘리지 않게 할 때 쓴다).
    static func isMedia(_ vc: UIViewController?) -> Bool {
        vc is ArgoMediaContainer || vc is ArgoAVPlayerController
    }

    private static func top() -> UIViewController? {
        let windows = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .flatMap { $0.windows }
        var vc = (windows.first { $0.isKeyWindow } ?? windows.first)?.rootViewController
        while let p = vc?.presentedViewController, !p.isBeingDismissed { vc = p }
        return vc
    }

    static func presentVideo(url: URL, player: AVPlayer, onClose: @escaping () -> Void) {
        if let c = current, c.presentingViewController != nil { return }
        guard let host = top() else { onClose(); return }
        let vc = ArgoAVPlayerController()
        let item = AVPlayerItem(url: url)
        // 정보 패널에 보일 제목 (태그가 없으면 파일 이름)
        let title = AVMutableMetadataItem()
        title.identifier = .commonIdentifierTitle
        title.value = (url.lastPathComponent as NSString).deletingPathExtension as NSString
        title.extendedLanguageTag = "und"
        item.externalMetadata = [title]
        player.replaceCurrentItem(with: item)
        vc.player = player
        vc.onClose = { current = nil; onClose() }
        vc.modalPresentationStyle = .fullScreen
        current = vc
        host.present(vc, animated: true) { player.play() }
    }

    static func presentAudio<V: View>(_ view: V, onClose: @escaping () -> Void) {
        if let c = current, c.presentingViewController != nil { return }
        guard let host = top() else { onClose(); return }
        let vc = ArgoMediaContainer(rootView: AnyView(view))
        vc.view.backgroundColor = .black
        vc.onClose = { current = nil; onClose() }
        vc.modalPresentationStyle = .fullScreen
        current = vc
        host.present(vc, animated: true)
    }

    /// 부른 쪽이 상태를 끄면 닫는다. 이미 닫혔으면 아무것도 하지 않는다.
    static func dismissCurrent() {
        guard let vc = current, vc.presentingViewController != nil, !vc.isBeingDismissed else {
            current = nil
            return
        }
        current = nil
        vc.dismiss(animated: true)
    }
}

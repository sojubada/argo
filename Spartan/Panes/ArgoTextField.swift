//
//  ArgoTextField.swift
//  Argo
//
//  키보드로 바로 치는 입력 칸.
//
//  tvOS 의 TextField 는 포커스를 받아 누르면 시스템이 전체 화면 입력 창을
//  띄운다. 블루투스 키보드가 있어도 그렇다 — 이름 하나 바꾸려 해도 화면을
//  한 번 갈아타야 했다. 시스템 입력 창은 UITextField 가 첫 응답자가 될 때
//  뜨므로, 여기서는 UITextField 를 쓰지 않고 키를 직접 받는다.
//
//  - 키보드: 칸에 포커스가 오면 그대로 친다. 한글은 두벌식으로 조합하고
//    (편집기와 같은 조합기), 한/영은 Caps Lock · ⌥ 한 번 · ⌃Space · 한/영 키.
//  - 리모컨: 가운데 버튼을 누르면 예전처럼 시스템 입력 창을 띄운다.
//    키보드가 없을 때의 길이다.
//
//  SwiftUI 의 TextField 와 인자 모양을 맞췄다. TextField( 를 ArgoTextField( 로
//  바꾸기만 하면 된다.
//

import SwiftUI
import UIKit

/// 한/영 상태. 편집기와 입력 칸이 같은 값을 쓴다 — 칸마다 따로면 옮겨 갈
/// 때마다 다시 바꿔야 한다.
enum ArgoInputMode {
    private static let key = "editorKoreanInput"
    static var korean: Bool {
        get { UserDefaults.settings.object(forKey: key) as? Bool ?? false }
        set { UserDefaults.settings.set(newValue, forKey: key) }
    }
}

final class ArgoKeyFieldView: UIControl {

    var text: String = "" { didSet { if oldValue != text { render() } } }
    var placeholder: String = "" { didSet { render() } }
    var isSecure = false { didSet { render() } }
    var onChange: ((String) -> Void)?
    var onCommit: (() -> Void)?
    var onEditingChanged: ((Bool) -> Void)?

    /// UTF-16 위치.
    private var cursor = 0
    private var hangul = ArgoHangulComposer()
    private var composingRange: NSRange?
    private var lonelyModifier: Int?
    private var repeatTimer: Timer?
    private var repeatCode: Int?

    private let clip = UIView()
    private let label = UILabel()
    private let caret = UIView()
    private let modeLabel = UILabel()
    private var blink: Timer?

    private var font: UIFont { UIFont.systemFont(ofSize: 30, weight: .medium) }

    override init(frame: CGRect) {
        super.init(frame: frame)
        layer.cornerRadius = 12
        clip.clipsToBounds = true
        clip.isUserInteractionEnabled = false
        label.isUserInteractionEnabled = false
        caret.isUserInteractionEnabled = false
        modeLabel.font = UIFont.systemFont(ofSize: 20, weight: .semibold)
        modeLabel.textAlignment = .center
        modeLabel.layer.cornerRadius = 6
        modeLabel.layer.masksToBounds = true
        modeLabel.isHidden = true
        addSubview(clip)
        clip.addSubview(label)
        clip.addSubview(caret)
        addSubview(modeLabel)
        caret.isHidden = true
        applyColors(focused: false)
    }
    required init?(coder: NSCoder) { fatalError() }

    override var canBecomeFocused: Bool { isEnabled }

    override var intrinsicContentSize: CGSize { CGSize(width: UIView.noIntrinsicMetric, height: 66) }

    override func layoutSubviews() {
        super.layoutSubviews()
        let modeWidth: CGFloat = 44
        modeLabel.frame = CGRect(x: bounds.width - modeWidth - 12, y: (bounds.height - 30) / 2,
                                 width: modeWidth, height: 30)
        clip.frame = CGRect(x: 18, y: 0, width: bounds.width - 36 - (modeLabel.isHidden ? 0 : modeWidth + 8),
                            height: bounds.height)
        render()
    }

    // MARK: 그리기

    private var shown: String { isSecure ? String(repeating: "•", count: text.count) : text }

    private func render() {
        let ns = text as NSString
        cursor = min(cursor, ns.length)
        if text.isEmpty {
            label.attributedText = NSAttributedString(string: placeholder, attributes: [
                .font: font, .foregroundColor: (isFocused ? UIColor.darkGray : UIColor(white: 1, alpha: 0.45))
            ])
        } else {
            let a = NSMutableAttributedString(string: shown, attributes: [
                .font: font, .foregroundColor: (isFocused ? UIColor.black : UIColor.white)
            ])
            if !isSecure, let r = composingRange, NSMaxRange(r) <= a.length {
                a.addAttribute(.underlineStyle, value: NSUnderlineStyle.single.rawValue, range: r)
            }
            label.attributedText = a
        }
        label.sizeToFit()
        let h = clip.bounds.height
        // 커서 앞까지의 폭. 숨긴 글은 점 개수로 센다.
        let before: String
        if isSecure {
            before = String(repeating: "•", count: (ns.substring(to: cursor) as String).count)
        } else {
            before = ns.substring(to: cursor)
        }
        let caretX = (before as NSString).size(withAttributes: [.font: font]).width
        // 커서가 보이도록 글을 옆으로 민다.
        let width = clip.bounds.width
        var offset: CGFloat = 0
        if caretX > width - 8 { offset = width - 8 - caretX }
        label.frame = CGRect(x: offset, y: (h - label.bounds.height) / 2,
                             width: max(label.bounds.width, 1), height: label.bounds.height)
        caret.frame = CGRect(x: offset + caretX, y: (h - font.lineHeight) / 2, width: 3, height: font.lineHeight)
        modeLabel.text = ArgoInputMode.korean ? "한" : "EN"
    }

    private func applyColors(focused: Bool) {
        backgroundColor = focused ? UIColor(white: 0.96, alpha: 1) : UIColor(white: 1, alpha: 0.12)
        caret.backgroundColor = ArgoTheme.accent()
        modeLabel.backgroundColor = focused ? UIColor(white: 0, alpha: 0.12) : .clear
        modeLabel.textColor = focused ? .darkGray : .clear
    }

    override func didUpdateFocus(in context: UIFocusUpdateContext, with coordinator: UIFocusAnimationCoordinator) {
        super.didUpdateFocus(in: context, with: coordinator)
        guard context.nextFocusedView === self || context.previouslyFocusedView === self else { return }
        let focused = context.nextFocusedView === self
        if !focused { endComposition() }
        if focused { cursor = (text as NSString).length }
        coordinator.addCoordinatedAnimations({
            self.transform = focused ? CGAffineTransform(scaleX: 1.04, y: 1.04) : .identity
            self.applyColors(focused: focused)
            self.layer.shadowOpacity = focused ? 0.35 : 0
            self.layer.shadowRadius = 14
            self.layer.shadowOffset = CGSize(width: 0, height: 8)
        }, completion: nil)
        caret.isHidden = !focused
        modeLabel.isHidden = !focused || isSecure
        setNeedsLayout()
        blink?.invalidate()
        if focused {
            blink = Timer.scheduledTimer(withTimeInterval: 0.55, repeats: true) { [weak self] _ in
                guard let self else { return }
                self.caret.alpha = self.caret.alpha > 0.5 ? 0.15 : 1
            }
        }
        onEditingChanged?(focused)
    }

    // MARK: 고치기

    private func setText(_ t: String, cursor c: Int) {
        text = t
        cursor = c
        render()
        onChange?(t)
    }

    private func replace(_ r: NSRange, with s: String) {
        let ns = text as NSString
        guard NSMaxRange(r) <= ns.length else { return }
        let out = ns.replacingCharacters(in: r, with: s)
        setText(out, cursor: r.location + (s as NSString).length)
    }

    private func insert(_ s: String) {
        replace(NSRange(location: cursor, length: 0), with: s)
    }

    private func composeJamo(_ j: UInt16) {
        let settled = hangul.input(j)
        let piece = settled + hangul.composing
        let target = composingRange ?? NSRange(location: cursor, length: 0)
        replace(target, with: piece)
        let len = (hangul.composing as NSString).length
        composingRange = (hangul.isComposing && len > 0) ? NSRange(location: cursor - len, length: len) : nil
        if composingRange == nil { hangul.reset() }
        render()
    }

    private func composeBackspace() {
        guard let r = composingRange else { return }
        _ = hangul.deleteBackward()
        replace(r, with: hangul.composing)
        let len = (hangul.composing as NSString).length
        composingRange = (hangul.isComposing && len > 0) ? NSRange(location: cursor - len, length: len) : nil
        if composingRange == nil { hangul.reset() }
        render()
    }

    private func endComposition() {
        composingRange = nil
        hangul.reset()
        render()
    }

    private func backspace(word: Bool, line: Bool) {
        let ns = text as NSString
        guard cursor > 0 else { return }
        var from: Int
        if line {
            from = 0
        } else if word {
            from = cursor
            while from > 0, let u = UnicodeScalar(ns.character(at: from - 1)),
                  CharacterSet.whitespaces.contains(u) { from -= 1 }
            while from > 0, let u = UnicodeScalar(ns.character(at: from - 1)),
                  !CharacterSet.whitespaces.contains(u), u != "/" { from -= 1 }
            if from == cursor { from -= 1 }
        } else {
            from = ns.rangeOfComposedCharacterSequence(at: cursor - 1).location
        }
        replace(NSRange(location: from, length: cursor - from), with: "")
    }

    private func toggleLanguage() {
        endComposition()
        ArgoInputMode.korean.toggle()
        render()
    }

    // MARK: 키

    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        var unhandled = Set<UIPress>()
        for press in presses {
            if let key = press.key, press.type != .menu {
                let code = key.keyCode.rawValue
                lonelyModifier = (code == 0xE2 || code == 0xE6) ? code : nil
                if handle(key) {
                    startRepeat(key)
                } else {
                    unhandled.insert(press)
                }
            } else if press.type == .select && press.key == nil {
                // 리모컨: 키보드가 없을 때의 길. 시스템 입력 창을 띄운다.
                presentSystemEntry()
            } else {
                unhandled.insert(press)
            }
        }
        if !unhandled.isEmpty { super.pressesBegan(unhandled, with: event) }
    }

    override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        for p in presses {
            guard let code = p.key?.keyCode.rawValue else { continue }
            if code == repeatCode { repeatTimer?.invalidate(); repeatCode = nil }
            if (code == 0xE2 || code == 0xE6), lonelyModifier == code {
                lonelyModifier = nil
                toggleLanguage()
            }
        }
        super.pressesEnded(presses, with: event)
    }

    override func pressesCancelled(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        repeatTimer?.invalidate()
        repeatCode = nil
        super.pressesCancelled(presses, with: event)
    }

    private func startRepeat(_ key: UIKey) {
        repeatTimer?.invalidate()
        let code = key.keyCode.rawValue
        // 지우기와 좌우만 되풀이한다.
        guard [0x2A, 0x4C, 0x4F, 0x50].contains(code) else { repeatCode = nil; return }
        repeatCode = code
        repeatTimer = Timer.scheduledTimer(withTimeInterval: 0.42, repeats: false) { [weak self] _ in
            guard let self, self.repeatCode == code else { return }
            self.repeatTimer = Timer.scheduledTimer(withTimeInterval: 0.05, repeats: true) { [weak self] t in
                guard let self, self.repeatCode == code else { t.invalidate(); return }
                _ = self.handle(key)
            }
        }
    }

    /// true 면 여기서 썼다. false 면 포커스 엔진 등 위로 넘긴다.
    private func handle(_ key: UIKey) -> Bool {
        let m = key.modifierFlags
        let cmd = m.contains(.command), shift = m.contains(.shift)
        let alt = m.contains(.alternate), ctrl = m.contains(.control)
        let code = key.keyCode.rawValue
        let ns = text as NSString

        if code == 0x90 || code == 0x39 || (code == 0x2C && ctrl) { toggleLanguage(); return true }
        if (0xE0...0xE7).contains(code) { return true }

        let latin = ArgoKeyMap.latin(key.keyCode, shift: shift)
        if !isSecure, ArgoInputMode.korean, !cmd, !ctrl, !alt,
           let c = latin?.first, let j = ArgoHangulComposer.jamo(forLatin: c) {
            composeJamo(j)
            return true
        }
        if code == 0x2A, !cmd, !alt, composingRange != nil {
            composeBackspace()
            return true
        }
        endComposition()

        switch key.keyCode {
        case .keyboardReturnOrEnter, .keypadEnter:
            onCommit?()
            return true
        case .keyboardDeleteOrBackspace:
            backspace(word: alt, line: cmd)
            return true
        case .keyboardDeleteForward:
            if cursor < ns.length {
                let r = ns.rangeOfComposedCharacterSequence(at: cursor)
                replace(r, with: "")
                cursor = r.location
                render()
            }
            return true
        case .keyboardLeftArrow:
            guard cursor > 0 else { return false }
            cursor = cmd ? 0 : ns.rangeOfComposedCharacterSequence(at: cursor - 1).location
            render()
            return true
        case .keyboardRightArrow:
            guard cursor < ns.length else { return false }
            cursor = cmd ? ns.length : NSMaxRange(ns.rangeOfComposedCharacterSequence(at: cursor))
            render()
            return true
        case .keyboardHome:
            cursor = 0; render(); return true
        case .keyboardEnd:
            cursor = ns.length; render(); return true
        case .keyboardUpArrow, .keyboardDownArrow, .keyboardTab, .keyboardEscape:
            return false
        default:
            break
        }

        if cmd {
            switch ArgoKeyMap.latin(key.keyCode, shift: false) ?? "" {
            case "a": cursor = ns.length; render(); return true
            case "c": ArgoTextClipboard.text = text; ArgoTextClipboard.isWholeLine = false; return true
            case "x":
                ArgoTextClipboard.text = text; ArgoTextClipboard.isWholeLine = false
                setText("", cursor: 0); return true
            case "v":
                if let t = ArgoTextClipboard.text {
                    insert(t.replacingOccurrences(of: "\n", with: " ").trimmingCharacters(in: .newlines))
                }
                return true
            default:
                return false
            }
        }
        if ctrl { return false }

        if let t = latin { insert(t); return true }
        let chars = key.characters
        if !chars.isEmpty,
           !chars.unicodeScalars.allSatisfy({ CharacterSet.controlCharacters.contains($0) }) {
            insert(chars)
            return true
        }
        return false
    }

    // MARK: 시스템 입력 창 (리모컨)

    private func presentSystemEntry() {
        guard let top = AirDropBridge.topViewController() else { return }
        let alert = UIAlertController(title: placeholder.isEmpty ? nil : placeholder,
                                      message: nil, preferredStyle: .alert)
        alert.addTextField { [weak self] tf in
            tf.text = self?.text
            tf.isSecureTextEntry = self?.isSecure ?? false
            tf.autocorrectionType = .no
            tf.autocapitalizationType = .none
        }
        alert.addAction(UIAlertAction(title: "확인", style: .default) { [weak self, weak alert] _ in
            guard let self else { return }
            let t = alert?.textFields?.first?.text ?? ""
            self.setText(t, cursor: (t as NSString).length)
            self.onCommit?()
        })
        alert.addAction(UIAlertAction(title: "취소", style: .cancel))
        top.present(alert, animated: true)
    }
}

// MARK: - SwiftUI

struct ArgoKeyFieldRepresentable: UIViewRepresentable {
    let placeholder: String
    @Binding var text: String
    var secure: Bool
    var onEditingChanged: ((Bool) -> Void)?
    var onCommit: (() -> Void)?

    func makeUIView(context: Context) -> ArgoKeyFieldView {
        let v = ArgoKeyFieldView()
        v.setContentHuggingPriority(.defaultLow, for: .horizontal)
        v.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        apply(v)
        v.text = text
        return v
    }

    func updateUIView(_ v: ArgoKeyFieldView, context: Context) {
        apply(v)
        // 치는 중에는 덮어쓰지 않는다. 바깥에서 바꾼 값(예: 끝에 / 붙이기)은
        // 포커스가 없을 때 들어온다.
        if v.text != text && !v.isFocused { v.text = text }
    }

    private func apply(_ v: ArgoKeyFieldView) {
        v.placeholder = placeholder
        v.isSecure = secure
        let binding = $text
        v.onChange = { binding.wrappedValue = $0 }
        v.onCommit = onCommit
        v.onEditingChanged = onEditingChanged
    }
}

/// TextField 와 같은 모양으로 쓴다.
struct ArgoTextField: View {
    let placeholder: String
    @Binding var text: String
    var secure = false
    var onEditingChanged: ((Bool) -> Void)?
    var onCommit: (() -> Void)?

    init(_ placeholder: String, text: Binding<String>, secure: Bool = false,
         onEditingChanged: ((Bool) -> Void)? = nil, onCommit: (() -> Void)? = nil) {
        self.placeholder = placeholder
        self._text = text
        self.secure = secure
        self.onEditingChanged = onEditingChanged
        self.onCommit = onCommit
    }

    var body: some View {
        ArgoKeyFieldRepresentable(placeholder: placeholder, text: $text, secure: secure,
                                  onEditingChanged: onEditingChanged, onCommit: onCommit)
            .frame(height: 66)
            .frame(maxWidth: .infinity)
    }
}

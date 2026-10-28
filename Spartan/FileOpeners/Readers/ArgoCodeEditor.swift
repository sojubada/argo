//
//  ArgoCodeEditor.swift
//  Argo
//
//  글·코드 편집기.
//
//  tvOS 의 UITextView 는 편집을 하지 않는다(isEditable 이 이 플랫폼에 없다).
//  그래서 전에는 줄마다 TextField 를 하나씩 놓았다 — 줄을 고르려면 숫자를
//  올려야 했고, 고른 줄로 목록이 내려가지 않았고, 긴 줄은 잘렸다.
//
//  여기서는 글을 UITextView 로 '보여 주기만' 하고, 커서·선택·입력은 직접
//  한다. 키보드 입력은 pressesBegan 의 UIKey 로 받는다.
//
//  - 블루투스 키보드: 맥의 코드 편집기와 같은 단축키 (⌘S 저장, ⌘Z 되돌리기,
//    ⌘F 찾기, ⌘/ 주석, ⌘D 줄 복제, ⌥↑↓ 줄 옮기기, Tab 들여쓰기 …)
//  - 리모컨: 방향키로 커서, 가운데 버튼으로 도구 모음, 재생 버튼으로 글 넣기
//  - 한글: tvOS 는 키보드에 한글 입력기를 주지 않는다. 키 자리를 미국 배열로
//    읽어 두벌식으로 직접 조합한다(TV-safari 의 조합기). 단축키도 키 자리로
//    가리므로 시스템 입력 언어가 한국어여도 ⌘S 가 ⌘ㄴ 이 되지 않는다.
//    시스템 입력 창이 필요하면 '글 넣기'(⌘E) · '줄 고치기'(⌘⇧E).
//

import UIKit
import SwiftUI

/// 편집기끼리 쓰는 클립보드. tvOS 에는 UIPasteboard 가 없다.
enum ArgoTextClipboard {
    static var text: String?
    /// 선택 없이 ⌘C 를 누르면 줄 전체를 담는다. 붙일 때 줄 단위로 넣으려고 표시한다.
    static var isWholeLine = false
}

/// 키 신호를 받는 보이지 않는 판. 포커스를 받아야 입력이 온다.
final class ArgoEditorInputView: UIControl {
    var onPressesBegan: ((Set<UIPress>, UIPressesEvent?) -> Bool)?
    var onPressesEnded: ((Set<UIPress>) -> Void)?

    override var canBecomeFocused: Bool { true }

    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        if onPressesBegan?(presses, event) == true { return }
        super.pressesBegan(presses, with: event)
    }

    override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        onPressesEnded?(presses)
        super.pressesEnded(presses, with: event)
    }

    override func pressesCancelled(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        onPressesEnded?(presses)
        super.pressesCancelled(presses, with: event)
    }
}

/// 줄 번호 칸. 보이는 줄만 그린다.
final class ArgoLineGutter: UIView {
    weak var textView: UITextView?
    /// 각 줄이 시작하는 글자 위치.
    var lineStarts: [Int] = [0]
    var currentLine = 0
    var font = UIFont.monospacedDigitSystemFont(ofSize: 22, weight: .regular)

    override init(frame: CGRect) {
        super.init(frame: frame)
        isOpaque = false
        contentMode = .redraw
        isUserInteractionEnabled = false
    }
    required init?(coder: NSCoder) { fatalError() }

    override func draw(_ rect: CGRect) {
        guard let tv = textView else { return }
        let lm = tv.layoutManager
        let inset = tv.textContainerInset
        let visible = CGRect(x: 0, y: tv.contentOffset.y - inset.top,
                             width: tv.bounds.width, height: tv.bounds.height)
        let glyphs = lm.glyphRange(forBoundingRect: visible, in: tv.textContainer)
        let storage = tv.textStorage.string as NSString

        func drawNumber(_ n: Int, at y: CGFloat, height: CGFloat) {
            let attrs: [NSAttributedString.Key: Any] = [
                .font: font,
                .foregroundColor: n == currentLine ? UIColor.white : ArgoCodeColors.dim
            ]
            let s = "\(n + 1)" as NSString
            let size = s.size(withAttributes: attrs)
            s.draw(at: CGPoint(x: bounds.width - size.width - 14,
                               y: y + (height - size.height) / 2), withAttributes: attrs)
        }

        lm.enumerateLineFragments(forGlyphRange: glyphs) { frag, _, _, glyphRange, _ in
            let charIndex = lm.characterIndexForGlyph(at: glyphRange.location)
            // 접힌 줄의 뒷부분에는 번호를 달지 않는다.
            guard charIndex == 0 || storage.character(at: charIndex - 1) == 10 else { return }
            let line = self.lineIndex(for: charIndex)
            drawNumber(line, at: frag.minY + inset.top - tv.contentOffset.y, height: frag.height)
        }
        // 맨 끝의 빈 줄.
        let extra = lm.extraLineFragmentRect
        if !extra.isEmpty {
            drawNumber(lineStarts.count - 1, at: extra.minY + inset.top - tv.contentOffset.y, height: extra.height)
        }
    }

    func lineIndex(for char: Int) -> Int {
        var lo = 0, hi = lineStarts.count - 1
        while lo < hi {
            let mid = (lo + hi + 1) / 2
            if lineStarts[mid] <= char { lo = mid } else { hi = mid - 1 }
        }
        return lo
    }
}

@available(tvOS 13.0, *)
final class ArgoCodeEditorController: UIViewController, UITextViewDelegate {

    var filePath: String = ""
    var onClose: (() -> Void)?
    /// 저장했을 때. 목록을 다시 읽는 데 쓴다.
    var onSaved: (() -> Void)?

    /// 편집할 수 있는 상한. 이보다 크면 한 글자마다 다시 그리는 값이 커진다.
    static let maxEditableBytes = 2 * 1024 * 1024

    // MARK: 상태

    private let textView = UITextView()
    private let gutter = ArgoLineGutter()
    private let input = ArgoEditorInputView()
    private let caret = UIView()
    private let statusLabel = UILabel()
    private let titleLabel = UILabel()
    private var toolbarSurface = ArgoSurfaceView.makeSurface()
    private var toolbar = UIStackView()
    private var toolbarInteractive = false

    private var storage: NSTextStorage { textView.textStorage }
    /// 글을 복사하지 않고 본다. .string 을 거치면 부를 때마다 통째로 복사된다.
    private var ns: NSString { textView.textStorage.mutableString }

    private var language: ArgoLanguage = .plain
    private var codec = ArgoTextCodec()
    private var indentUnit = "    "
    private var fontSize: CGFloat = 26
    private var font: UIFont { UIFont.monospacedSystemFont(ofSize: fontSize, weight: .regular) }

    /// 커서 (UTF-16 위치).
    private var cursor = 0
    /// 선택을 시작한 곳. nil 이면 선택 없음.
    private var anchor: Int?
    /// 위아래로 갈 때 지키려는 칸.
    private var preferredColumn: Int?

    private var dirty = false { didSet { updateTitle() } }
    private var readOnlyReason: String?

    private struct Edit {
        var range: NSRange      // 바뀐 뒤의 범위
        var oldText: String
        var newText: String
        var cursorBefore: Int
        var cursorAfter: Int
        var time: Date
    }
    private var undoStack: [Edit] = []
    private var redoStack: [Edit] = []

    private var lastFind: String = ""
    private var findHits: [NSRange] = []
    private var selectionPainted: NSRange?
    private var findHitsNeedPaint = false

    // 한글 입력
    private var hangul = ArgoHangulComposer()
    /// 만드는 중인 글자가 놓인 자리.
    private var composingRange: NSRange?
    private var koreanInput = false {
        didSet { UserDefaults.settings.set(koreanInput, forKey: "editorKoreanInput"); updateStatus() }
    }
    /// 오른쪽 ⌥ 를 혼자 눌렀다 뗐는지 보려고 둔다. 다른 키와 같이 누르면 바꾸지 않는다.
    private var lonelyModifier: Int?

    private var holdingTextEntry = false
    private var blinkTimer: Timer?
    private var repeatTimer: Timer?
    private var repeatKey: UIKeyboardHIDUsage?
    private var highlightTimer: Timer?
    private var lineStartsDirty = true

    // MARK: 화면

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = ArgoTheme.background()
        fontSize = CGFloat(UserDefaults.settings.object(forKey: "editorFontSize") as? Double ?? 26)

        titleLabel.font = ArgoTheme.fontCaption()
        titleLabel.textColor = ArgoTheme.textSecondary()
        titleLabel.lineBreakMode = .byTruncatingHead
        statusLabel.font = ArgoTheme.fontMonoBody()
        statusLabel.textColor = ArgoTheme.textSecondary()
        statusLabel.textAlignment = .right

        textView.isSelectable = false
        textView.isScrollEnabled = true
        textView.isUserInteractionEnabled = false
        textView.backgroundColor = .clear
        textView.delegate = self
        textView.textContainerInset = UIEdgeInsets(top: 16, left: 12, bottom: 400, right: ArgoTheme.screenMargin())
        textView.textContainer.lineFragmentPadding = 0
        textView.layoutManager.allowsNonContiguousLayout = false

        gutter.textView = textView
        caret.backgroundColor = ArgoTheme.accent()
        caret.isUserInteractionEnabled = false
        textView.addSubview(caret)

        input.backgroundColor = .clear
        input.onPressesBegan = { [weak self] presses, _ in self?.handlePresses(presses) ?? false }
        input.onPressesEnded = { [weak self] presses in self?.pressesFinished(presses) }

        for v in [titleLabel, statusLabel, gutter, textView, input] as [UIView] {
            v.translatesAutoresizingMaskIntoConstraints = false
            view.addSubview(v)
        }
        buildToolbar()

        let g = view.safeAreaLayoutGuide
        NSLayoutConstraint.activate([
            titleLabel.leadingAnchor.constraint(equalTo: g.leadingAnchor, constant: 20),
            titleLabel.topAnchor.constraint(equalTo: g.topAnchor, constant: 4),
            titleLabel.trailingAnchor.constraint(lessThanOrEqualTo: statusLabel.leadingAnchor, constant: -20),
            statusLabel.trailingAnchor.constraint(equalTo: g.trailingAnchor, constant: -20),
            statusLabel.centerYAnchor.constraint(equalTo: titleLabel.centerYAnchor),

            gutter.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 20),
            gutter.widthAnchor.constraint(equalToConstant: 110),
            gutter.topAnchor.constraint(equalTo: titleLabel.bottomAnchor, constant: 8),
            gutter.bottomAnchor.constraint(equalTo: view.bottomAnchor),

            textView.leadingAnchor.constraint(equalTo: gutter.trailingAnchor),
            textView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            textView.topAnchor.constraint(equalTo: gutter.topAnchor),
            textView.bottomAnchor.constraint(equalTo: view.bottomAnchor),

            input.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            input.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            input.topAnchor.constraint(equalTo: view.topAnchor),
            input.bottomAnchor.constraint(equalTo: view.bottomAnchor),

            toolbarSurface.centerXAnchor.constraint(equalTo: g.centerXAnchor),
            toolbarSurface.bottomAnchor.constraint(equalTo: g.bottomAnchor, constant: -ArgoTheme.gutter()),
            toolbar.leadingAnchor.constraint(equalTo: toolbarSurface.leadingAnchor, constant: ArgoTheme.gutter()),
            toolbar.trailingAnchor.constraint(equalTo: toolbarSurface.trailingAnchor, constant: -ArgoTheme.gutter()),
            toolbar.topAnchor.constraint(equalTo: toolbarSurface.topAnchor, constant: ArgoTheme.gutter() * 0.5),
            toolbar.bottomAnchor.constraint(equalTo: toolbarSurface.bottomAnchor, constant: -ArgoTheme.gutter() * 0.5),
        ])
        view.bringSubviewToFront(toolbarSurface)

        let menu = UITapGestureRecognizer(target: self, action: #selector(menuPressed))
        menu.allowedPressTypes = [NSNumber(value: UIPress.PressType.menu.rawValue)]
        view.addGestureRecognizer(menu)

        loadFile()
        setToolbar(interactive: false)
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        // 뒤의 파일 탐색기가 단축키를 가져가지 않게 한다. ⌘C·⌘V 가 파일
        // 복사로 흘러가면 안 된다. 경고창이 닫힐 때도 여기가 다시 불릴 수
        // 있어, 한 번만 센다.
        if !holdingTextEntry { holdingTextEntry = true; ArgoTextEntry.begin() }
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        setNeedsFocusUpdate()
        updateFocusIfNeeded()
        startBlink()
        refreshCaret(scroll: true)
    }

    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        // 경고창 때문에 잠깐 가려진 것이면 놓지 않는다.
        if presentedViewController == nil, holdingTextEntry {
            holdingTextEntry = false
            ArgoTextEntry.end()
        }
        blinkTimer?.invalidate()
        repeatTimer?.invalidate()
        highlightTimer?.invalidate()
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        // 칠하기는 하지 않는다. 글에 속성을 바꾸면 다시 레이아웃이 돌아
        // 여기로 또 들어온다.
        refreshCaret(scroll: false, paint: false)
        gutter.setNeedsDisplay()
    }

    override var preferredFocusEnvironments: [UIFocusEnvironment] {
        toolbarInteractive ? [toolbar] : [input]
    }

    /// 메뉴 버튼(키보드 Esc 도 이리로 온다)은 제스처 하나로만 받는다.
    ///
    /// 그냥 두면 UIKit 이 편집기를 닫아, 저장하지 않은 고침이 묻지도 않고
    /// 사라진다. 누름 처리와 제스처 양쪽에서 받으면 창이 두 번 뜬다.
    @objc private func menuPressed() {
        if toolbarInteractive {
            setToolbar(interactive: false)
        } else if selection != nil || !findHits.isEmpty {
            anchor = nil
            clearFindHits()
            refreshCaret(scroll: false)
        } else {
            closeRequested()
        }
    }

    func scrollViewDidScroll(_ scrollView: UIScrollView) {
        gutter.setNeedsDisplay()
    }

    // MARK: 읽기

    private func loadFile() {
        language = ArgoTextKind.language(for: filePath)
        let size = ((try? FileManager.default.attributesOfItem(atPath: filePath)[.size]) as? NSNumber)?.intValue ?? 0
        if size > Self.maxEditableBytes {
            readOnlyReason = "파일이 커서(\(ByteCountFormatter.string(fromByteCount: Int64(size), countStyle: .file))) 편집할 수 없습니다. 읽기 화면을 쓰십시오."
        }
        guard let data = FileManager.default.contents(atPath: filePath) else {
            readOnlyReason = "파일을 열 수 없습니다."
            setText("")
            return
        }
        if data.isEmpty {
            setText("")
        } else if let decoded = ArgoTextCodec.decode(readOnlyReason == nil ? data : data.prefix(Self.maxEditableBytes)) {
            codec = decoded.1
            setText(decoded.0)
        } else {
            readOnlyReason = "글자로 읽을 수 없는 파일입니다."
            setText("")
        }
        indentUnit = ArgoTextKind.indentUnit(for: storage.string, language: language)
        // 지난번 고른 입력 언어를 따른다. 처음이면 글은 한글, 코드는 영문.
        if let saved = UserDefaults.settings.object(forKey: "editorKoreanInput") as? Bool {
            koreanInput = saved
        } else {
            koreanInput = (language == .plain || language == .markdown)
        }
        updateTitle()
        updateStatus()
    }

    private func setText(_ s: String) {
        storage.setAttributedString(NSAttributedString(string: s, attributes: baseAttributes()))
        ArgoSyntax.highlight(storage, range: NSRange(location: 0, length: storage.length), language: language)
        lineStartsDirty = true
        rebuildLineStarts()
        cursor = 0
        anchor = nil
    }

    private func baseAttributes() -> [NSAttributedString.Key: Any] {
        let p = NSMutableParagraphStyle()
        p.lineSpacing = fontSize * 0.25
        p.lineBreakMode = .byCharWrapping
        // 접힌 줄은 들여 써서 새 줄과 구별한다.
        p.headIndent = fontSize * 1.2
        return [.font: font, .foregroundColor: ArgoCodeColors.text, .paragraphStyle: p]
    }

    // MARK: 줄

    private func rebuildLineStarts() {
        guard lineStartsDirty else { return }
        var starts = [0]
        let s = ns
        let n = s.length
        var range = NSRange(location: 0, length: n)
        while range.length > 0 {
            let r = s.range(of: "\n", options: .literal, range: range)
            if r.location == NSNotFound { break }
            starts.append(r.location + 1)
            let next = r.location + 1
            range = NSRange(location: next, length: n - next)
        }
        gutter.lineStarts = starts
        lineStartsDirty = false
    }

    private var lineStarts: [Int] { rebuildLineStarts(); return gutter.lineStarts }

    private func lineOf(_ pos: Int) -> Int { rebuildLineStarts(); return gutter.lineIndex(for: pos) }

    /// 줄의 시작과 끝(줄바꿈 앞).
    private func lineRange(_ line: Int) -> NSRange {
        let starts = lineStarts
        let start = starts[line]
        let end = line + 1 < starts.count ? starts[line + 1] - 1 : ns.length
        return NSRange(location: start, length: max(0, end - start))
    }

    private func lineText(_ line: Int) -> String { ns.substring(with: lineRange(line)) }

    private func leadingWhitespace(_ s: String) -> String {
        String(s.prefix(while: { $0 == " " || $0 == "\t" }))
    }

    // MARK: 커서

    private var selection: NSRange? {
        guard let a = anchor, a != cursor else { return nil }
        return NSRange(location: min(a, cursor), length: abs(a - cursor))
    }

    private func moveCursor(to pos: Int, extend: Bool, keepColumn: Bool = false) {
        let p = max(0, min(ns.length, pos))
        if extend {
            if anchor == nil { anchor = cursor }
        } else {
            anchor = nil
        }
        cursor = p
        if !keepColumn { preferredColumn = nil }
        refreshCaret(scroll: true)
    }

    private func composedStep(_ forward: Bool) -> Int {
        let s = ns
        if forward {
            guard cursor < s.length else { return cursor }
            return NSMaxRange(s.rangeOfComposedCharacterSequence(at: cursor))
        } else {
            guard cursor > 0 else { return 0 }
            return s.rangeOfComposedCharacterSequence(at: cursor - 1).location
        }
    }

    private func wordStep(_ forward: Bool) -> Int {
        let s = ns
        var i = cursor
        func isWord(_ c: unichar) -> Bool {
            guard let u = UnicodeScalar(c) else { return true }
            return CharacterSet.alphanumerics.contains(u) || c == 95
        }
        if forward {
            while i < s.length && !isWord(s.character(at: i)) { i += 1 }
            while i < s.length && isWord(s.character(at: i)) { i += 1 }
        } else {
            while i > 0 && !isWord(s.character(at: i - 1)) { i -= 1 }
            while i > 0 && isWord(s.character(at: i - 1)) { i -= 1 }
        }
        return i
    }

    private func verticalStep(_ lines: Int) -> Int {
        let line = lineOf(cursor)
        let col = preferredColumn ?? (cursor - lineStarts[line])
        preferredColumn = col
        let target = max(0, min(lineStarts.count - 1, line + lines))
        let r = lineRange(target)
        return r.location + min(col, r.length)
    }

    /// 줄 처음: 들여쓰기 끝으로 먼저, 한 번 더 누르면 진짜 처음으로.
    private func smartHome() -> Int {
        let line = lineOf(cursor)
        let r = lineRange(line)
        let indent = (leadingWhitespace(lineText(line)) as NSString).length
        let first = r.location + indent
        return cursor == first ? r.location : first
    }

    private func refreshCaret(scroll: Bool, paint: Bool = true) {
        if paint { paintSelection() }
        let lm = textView.layoutManager
        let inset = textView.textContainerInset
        var rect: CGRect
        let length = storage.length
        if length == 0 {
            rect = CGRect(x: 0, y: 0, width: 3, height: font.lineHeight)
        } else if cursor < length {
            let g = lm.glyphIndexForCharacter(at: cursor)
            let frag = lm.lineFragmentRect(forGlyphAt: g, effectiveRange: nil)
            let here = lm.boundingRect(forGlyphRange: NSRange(location: g, length: 1), in: textView.textContainer)
            rect = CGRect(x: here.minX, y: frag.minY, width: 3, height: frag.height)
            if ns.character(at: cursor) == 10, cursor > 0, ns.character(at: cursor - 1) != 10 {
                // 줄 끝. 줄바꿈 글자의 자리는 앞 글자 바로 뒤다.
                let pg = lm.glyphIndexForCharacter(at: cursor - 1)
                let br = lm.boundingRect(forGlyphRange: NSRange(location: pg, length: 1), in: textView.textContainer)
                rect.origin.x = br.maxX
            }
        } else {
            let extra = lm.extraLineFragmentRect
            if !extra.isEmpty {
                rect = CGRect(x: 0, y: extra.minY, width: 3, height: extra.height)
            } else {
                let pg = lm.glyphIndexForCharacter(at: length - 1)
                let br = lm.boundingRect(forGlyphRange: NSRange(location: pg, length: 1), in: textView.textContainer)
                let frag = lm.lineFragmentRect(forGlyphAt: pg, effectiveRange: nil)
                rect = CGRect(x: br.maxX, y: frag.minY, width: 3, height: frag.height)
            }
        }
        rect.origin.x += inset.left
        rect.origin.y += inset.top
        caret.frame = rect.insetBy(dx: 0, dy: fontSize * 0.05)
        caret.alpha = 1
        if scroll {
            let visible = rect.insetBy(dx: 0, dy: -fontSize * 3)
            textView.scrollRectToVisible(visible, animated: false)
        }
        gutter.currentLine = lineOf(cursor)
        gutter.font = UIFont.monospacedDigitSystemFont(ofSize: fontSize * 0.82, weight: .regular)
        gutter.setNeedsDisplay()
        updateStatus()
    }

    private func startBlink() {
        blinkTimer?.invalidate()
        blinkTimer = Timer.scheduledTimer(withTimeInterval: 0.55, repeats: true) { [weak self] _ in
            guard let self else { return }
            self.caret.alpha = self.caret.alpha > 0.5 ? 0.15 : 1
        }
    }

    /// 선택한 곳에 바탕색을 칠한다. 앞서 칠한 곳은 먼저 지운다.
    private func paintSelection() {
        let sel = selection
        if sel == selectionPainted && !findHitsNeedPaint { return }
        findHitsNeedPaint = false
        storage.beginEditing()
        if let old = selectionPainted, NSMaxRange(old) <= storage.length {
            storage.removeAttribute(.backgroundColor, range: old)
        }
        selectionPainted = nil
        for hit in findHits where NSMaxRange(hit) <= storage.length {
            storage.addAttribute(.backgroundColor, value: ArgoCodeColors.findHit, range: hit)
        }
        if let sel {
            storage.addAttribute(.backgroundColor, value: ArgoCodeColors.selection, range: sel)
            selectionPainted = sel
        }
        storage.endEditing()
    }

    // MARK: 고치기

    /// 모든 고침은 여기를 지난다. 되돌리기 기록과 색칠을 한곳에서 한다.
    private func replace(_ range: NSRange, with text: String, cursorAfter: Int? = nil, coalesce: Bool = false) {
        guard readOnlyReason == nil else { flash(readOnlyReason!); return }
        let old = ns.substring(with: range)
        let before = cursor
        clearFindHits()
        if let p = selectionPainted, NSMaxRange(p) <= storage.length {
            storage.removeAttribute(.backgroundColor, range: p)
            selectionPainted = nil
        }

        storage.beginEditing()
        storage.replaceCharacters(in: range, with: NSAttributedString(string: text, attributes: baseAttributes()))
        storage.endEditing()
        let newRange = NSRange(location: range.location, length: (text as NSString).length)
        let after = cursorAfter ?? NSMaxRange(newRange)

        // 연달아 친 글자는 한 번에 되돌린다.
        if coalesce, var last = undoStack.last, !text.contains("\n"),
           Date().timeIntervalSince(last.time) < 1.5, !last.newText.contains("\n"),
           range.location >= last.range.location, NSMaxRange(range) == NSMaxRange(last.range) {
            // 바로 앞 고침의 끝에 잇거나, 그 끝을 갈아 끼운다. 한글은 조합하는
            // 동안 마지막 글자를 계속 바꿔 끼우므로 뒤쪽이 필요하다.
            let keep = (last.newText as NSString).substring(to: range.location - last.range.location)
            last.newText = keep + text
            last.range.length = (last.newText as NSString).length
            last.cursorAfter = after
            last.time = Date()
            undoStack[undoStack.count - 1] = last
        } else {
            undoStack.append(Edit(range: newRange, oldText: old, newText: text,
                                  cursorBefore: before, cursorAfter: after, time: Date()))
            if undoStack.count > 500 { undoStack.removeFirst(undoStack.count - 500) }
        }
        redoStack.removeAll()

        afterChange(around: newRange)
        anchor = nil
        cursor = after
        preferredColumn = nil
        dirty = true
        refreshCaret(scroll: true)
    }

    private func afterChange(around r: NSRange) {
        lineStartsDirty = true
        // 고친 줄만 바로 칠하고, 여러 줄에 걸친 주석·문자열은 잠시 뒤 전체를 다시 칠한다.
        let s = ns
        let safe = NSRange(location: min(r.location, s.length), length: min(r.length, s.length - min(r.location, s.length)))
        let para = s.paragraphRange(for: safe)
        storage.beginEditing()
        storage.addAttributes(baseAttributes(), range: para)
        ArgoSyntax.highlight(storage, range: para, language: language)
        storage.endEditing()

        highlightTimer?.invalidate()
        if storage.length < 400_000 {
            highlightTimer = Timer.scheduledTimer(withTimeInterval: 0.8, repeats: false) { [weak self] _ in
                guard let self else { return }
                let all = NSRange(location: 0, length: self.storage.length)
                self.storage.beginEditing()
                ArgoSyntax.highlight(self.storage, range: all, language: self.language)
                self.storage.endEditing()
                self.selectionPainted = nil
                self.findHitsNeedPaint = true
                self.paintSelection()
            }
        }
    }

    private func insert(_ text: String, coalesce: Bool = true) {
        if let sel = selection {
            replace(sel, with: text)
        } else {
            replace(NSRange(location: cursor, length: 0), with: text, coalesce: coalesce)
        }
    }

    private func typeCharacter(_ ch: String) {
        // 닫는 괄호를 빈 줄에서 치면 한 단계 내어 쓴다.
        if ch.count == 1, "}])".contains(ch), selection == nil {
            let line = lineOf(cursor)
            let r = lineRange(line)
            let before = ns.substring(with: NSRange(location: r.location, length: cursor - r.location))
            if !before.isEmpty, before.allSatisfy({ $0 == " " || $0 == "\t" }) {
                let cut = before.hasSuffix(indentUnit) ? (indentUnit as NSString).length
                        : (before.hasSuffix("\t") ? 1 : 0)
                if cut > 0 {
                    replace(NSRange(location: cursor - cut, length: cut), with: ch)
                    return
                }
            }
        }
        insert(ch)
    }

    private func newline() {
        let line = lineOf(cursor)
        let r = lineRange(line)
        let start = selection?.location ?? cursor
        let before = ns.substring(with: NSRange(location: r.location, length: max(0, start - r.location)))
        var indent = leadingWhitespace(before)
        if before.allSatisfy({ $0 == " " || $0 == "\t" }) { indent = before }
        let trimmed = before.trimmingCharacters(in: .whitespaces)
        let opens = ArgoTextKind.opensBlock(trimmed, language: language)
        let end = selection.map { NSMaxRange($0) } ?? cursor
        let nextChar: unichar? = end < ns.length ? ns.character(at: end) : nil
        let closesRightAway = opens && nextChar != nil && "}])".utf16.contains(nextChar!)

        if closesRightAway {
            // { | }  →  {
            //              |
            //           }
            let inner = "\n" + indent + indentUnit
            let text = inner + "\n" + indent
            let target = selection ?? NSRange(location: cursor, length: 0)
            replace(target, with: text, cursorAfter: target.location + (inner as NSString).length)
        } else {
            insert("\n" + indent + (opens ? indentUnit : ""), coalesce: false)
        }
    }

    private func backspace() {
        if let sel = selection { replace(sel, with: ""); return }
        guard cursor > 0 else { return }
        // 들여쓰기 안에서는 한 단계씩 지운다.
        let line = lineOf(cursor)
        let r = lineRange(line)
        let before = ns.substring(with: NSRange(location: r.location, length: cursor - r.location))
        if indentUnit != "\t", !before.isEmpty, before.allSatisfy({ $0 == " " }) {
            let unit = (indentUnit as NSString).length
            let cut = before.count % unit == 0 ? unit : before.count % unit
            replace(NSRange(location: cursor - cut, length: cut), with: "")
            return
        }
        let from = composedStep(false)
        replace(NSRange(location: from, length: cursor - from), with: "")
    }

    private func deleteForward() {
        if let sel = selection { replace(sel, with: ""); return }
        guard cursor < ns.length else { return }
        let to = composedStep(true)
        replace(NSRange(location: cursor, length: to - cursor), with: "")
    }

    /// 고른 줄들(선택이 없으면 커서 줄)의 범위.
    private func selectedLines() -> (first: Int, last: Int) {
        let sel = selection ?? NSRange(location: cursor, length: 0)
        let first = lineOf(sel.location)
        var last = lineOf(NSMaxRange(sel))
        if last > first, sel.length > 0, lineStarts[last] == NSMaxRange(sel) { last -= 1 }
        return (first, last)
    }

    /// 줄 여러 개를 한 번에 바꾼다. 선택은 바뀐 줄 전체로 다시 잡는다.
    private func transformLines(_ f: ([String]) -> [String]) {
        let (first, last) = selectedLines()
        let start = lineRange(first).location
        let end = NSMaxRange(lineRange(last))
        let range = NSRange(location: start, length: end - start)
        let lines = ns.substring(with: range).components(separatedBy: "\n")
        let out = f(lines).joined(separator: "\n")
        let hadSelection = selection != nil
        replace(range, with: out, cursorAfter: start + (out as NSString).length)
        if hadSelection || first != last {
            anchor = start
            refreshCaret(scroll: false)
        }
    }

    private func indentLines() { transformLines { $0.map { $0.isEmpty ? $0 : indentUnit + $0 } } }

    private func outdentLines() {
        transformLines { lines in
            lines.map { line in
                if line.hasPrefix("\t") { return String(line.dropFirst()) }
                let n = min(line.prefix(while: { $0 == " " }).count, (indentUnit == "\t" ? 4 : indentUnit.count))
                return String(line.dropFirst(n))
            }
        }
    }

    private func toggleComment() {
        if let token = ArgoTextKind.lineComment(language) {
            transformLines { lines in
                let body = lines.filter { !$0.trimmingCharacters(in: .whitespaces).isEmpty }
                let allCommented = !body.isEmpty && body.allSatisfy {
                    $0.trimmingCharacters(in: .whitespaces).hasPrefix(token)
                }
                let minIndent = body.map { leadingWhitespace($0).count }.min() ?? 0
                return lines.map { line in
                    if line.trimmingCharacters(in: .whitespaces).isEmpty { return line }
                    if allCommented {
                        guard let r = line.range(of: token) else { return line }
                        let offset = line.distance(from: line.startIndex, to: r.lowerBound)
                        var out = line
                        out.removeSubrange(r)
                        let i = out.index(out.startIndex, offsetBy: offset)
                        if i < out.endIndex && out[i] == " " { out.remove(at: i) }
                        return out
                    }
                    let idx = line.index(line.startIndex, offsetBy: min(minIndent, line.count))
                    return String(line[..<idx]) + token + " " + String(line[idx...])
                }
            }
        } else if let pair = ArgoTextKind.blockComment(language) {
            let open = pair.0, close = pair.1
            transformLines { lines in
                let joined = lines.joined(separator: "\n")
                let t = joined.trimmingCharacters(in: .whitespacesAndNewlines)
                if t.hasPrefix(open) && t.hasSuffix(close) {
                    var s = joined
                    if let r = s.range(of: open) { s.removeSubrange(r) }
                    if let r = s.range(of: close, options: .backwards) { s.removeSubrange(r) }
                    return s.trimmingCharacters(in: .whitespaces).components(separatedBy: "\n")
                }
                return (open + " " + joined + " " + close).components(separatedBy: "\n")
            }
        } else {
            flash("이 형식에는 주석이 없습니다")
        }
    }

    private func duplicateLines() {
        let (first, last) = selectedLines()
        let start = lineRange(first).location
        let end = NSMaxRange(lineRange(last))
        let block = ns.substring(with: NSRange(location: start, length: end - start))
        let offset = cursor - start
        replace(NSRange(location: end, length: 0), with: "\n" + block,
                cursorAfter: end + 1 + offset)
    }

    private func deleteLines() {
        let (first, last) = selectedLines()
        var start = lineRange(first).location
        var end = NSMaxRange(lineRange(last))
        if end < ns.length { end += 1 } else if start > 0 { start -= 1 }
        replace(NSRange(location: start, length: end - start), with: "", cursorAfter: start)
    }

    private func moveLines(up: Bool) {
        let (first, last) = selectedLines()
        if up && first == 0 { return }
        if !up && last >= lineStarts.count - 1 { return }
        let a = up ? first - 1 : first
        let b = up ? last : last + 1
        let start = lineRange(a).location
        let end = NSMaxRange(lineRange(b))
        var lines = ns.substring(with: NSRange(location: start, length: end - start)).components(separatedBy: "\n")
        let col = cursor - lineRange(lineOf(cursor)).location
        if up { lines.append(lines.removeFirst()) } else { lines.insert(lines.removeLast(), at: 0) }
        let out = lines.joined(separator: "\n")
        let newLine = lineOf(cursor) + (up ? -1 : 1)
        replace(NSRange(location: start, length: end - start), with: out)
        let r = lineRange(newLine)
        moveCursor(to: r.location + min(col, r.length), extend: false)
    }

    // MARK: 되돌리기

    private func undo() {
        endComposition()
        guard let e = undoStack.popLast() else { flash("되돌릴 것이 없습니다"); return }
        apply(range: e.range, text: e.oldText)
        redoStack.append(e)
        cursor = e.cursorBefore
        anchor = nil
        dirty = true
        refreshCaret(scroll: true)
    }

    private func redo() {
        endComposition()
        guard let e = redoStack.popLast() else { flash("다시 할 것이 없습니다"); return }
        let r = NSRange(location: e.range.location, length: (e.oldText as NSString).length)
        apply(range: r, text: e.newText)
        undoStack.append(e)
        cursor = e.cursorAfter
        anchor = nil
        dirty = true
        refreshCaret(scroll: true)
    }

    /// 기록을 남기지 않고 바꾼다. 되돌리기 전용.
    private func apply(range: NSRange, text: String) {
        clearFindHits()
        if let p = selectionPainted, NSMaxRange(p) <= storage.length {
            storage.removeAttribute(.backgroundColor, range: p)
            selectionPainted = nil
        }
        guard NSMaxRange(range) <= storage.length else { return }
        storage.replaceCharacters(in: range, with: NSAttributedString(string: text, attributes: baseAttributes()))
        afterChange(around: NSRange(location: range.location, length: (text as NSString).length))
    }

    // MARK: 클립보드

    private func copy(cut: Bool) {
        if let sel = selection {
            ArgoTextClipboard.text = ns.substring(with: sel)
            ArgoTextClipboard.isWholeLine = false
            if cut { replace(sel, with: "") }
        } else {
            // 고른 것이 없으면 줄 전체.
            let r = lineRange(lineOf(cursor))
            ArgoTextClipboard.text = ns.substring(with: r) + "\n"
            ArgoTextClipboard.isWholeLine = true
            if cut { deleteLines() }
        }
        flash(cut ? "잘라냈습니다" : "복사했습니다")
    }

    private func paste() {
        guard let t = ArgoTextClipboard.text, !t.isEmpty else { flash("붙일 것이 없습니다"); return }
        if ArgoTextClipboard.isWholeLine && selection == nil {
            let start = lineRange(lineOf(cursor)).location
            let col = cursor - start
            replace(NSRange(location: start, length: 0), with: t,
                    cursorAfter: start + (t as NSString).length + col)
        } else {
            insert(t, coalesce: false)
        }
    }

    // MARK: 찾기

    private func clearFindHits() {
        guard !findHits.isEmpty else { return }
        storage.beginEditing()
        for h in findHits where NSMaxRange(h) <= storage.length {
            storage.removeAttribute(.backgroundColor, range: h)
        }
        storage.endEditing()
        findHits = []
    }

    private func find(_ needle: String, forward: Bool = true) {
        guard !needle.isEmpty else { return }
        lastFind = needle
        let s = ns
        let from = forward ? (selection.map { NSMaxRange($0) } ?? cursor) : (selection?.location ?? cursor)
        var r: NSRange
        if forward {
            r = s.range(of: needle, options: .caseInsensitive,
                        range: NSRange(location: from, length: s.length - from))
            if r.location == NSNotFound {
                r = s.range(of: needle, options: .caseInsensitive, range: NSRange(location: 0, length: s.length))
            }
        } else {
            r = s.range(of: needle, options: [.caseInsensitive, .backwards],
                        range: NSRange(location: 0, length: from))
            if r.location == NSNotFound {
                r = s.range(of: needle, options: [.caseInsensitive, .backwards],
                            range: NSRange(location: 0, length: s.length))
            }
        }
        guard r.location != NSNotFound else { flash("‘\(needle)’ 없음"); return }

        // 모든 자리를 옅게 칠해 둔다. 몇 개인지도 알린다.
        clearFindHits()
        var hits: [NSRange] = []
        var search = NSRange(location: 0, length: s.length)
        while hits.count < 2000 {
            let h = s.range(of: needle, options: .caseInsensitive, range: search)
            if h.location == NSNotFound { break }
            hits.append(h)
            let next = NSMaxRange(h)
            search = NSRange(location: next, length: s.length - next)
        }
        findHits = hits
        findHitsNeedPaint = true
        anchor = r.location
        cursor = NSMaxRange(r)
        refreshCaret(scroll: true)
        let idx = (hits.firstIndex(where: { $0.location == r.location }) ?? 0) + 1
        flash("\(idx) / \(hits.count)")
    }

    private func replaceAll(_ needle: String, with replacement: String) {
        guard !needle.isEmpty else { return }
        let s = ns.mutableCopy() as! NSMutableString
        let count = s.replaceOccurrences(of: needle, with: replacement, options: .caseInsensitive,
                                         range: NSRange(location: 0, length: s.length))
        guard count > 0 else { flash("‘\(needle)’ 없음"); return }
        replace(NSRange(location: 0, length: ns.length), with: s as String, cursorAfter: min(cursor, s.length))
        flash("\(count)군데 바꿨습니다")
    }

    // MARK: 입력 창 (한글은 여기서)

    private func ask(title: String, message: String? = nil, fields: [(placeholder: String, text: String)],
                     actions: [(String, ([String]) -> Void)]) {
        let alert = UIAlertController(title: title, message: message, preferredStyle: .alert)
        for f in fields {
            alert.addTextField { tf in
                tf.placeholder = f.placeholder
                tf.text = f.text
                tf.autocorrectionType = .no
                tf.autocapitalizationType = .none
            }
        }
        for (name, run) in actions {
            alert.addAction(UIAlertAction(title: name, style: .default) { [weak alert] _ in
                run(alert?.textFields?.map { $0.text ?? "" } ?? [])
            })
        }
        alert.addAction(UIAlertAction(title: "취소", style: .cancel))
        present(alert, animated: true)
    }

    @objc private func insertTextPrompt() {
        endComposition()
        ask(title: "글 넣기", message: "커서 자리에 넣습니다. 한글도 여기서 칠 수 있습니다.",
            fields: [("넣을 글", "")],
            actions: [("넣기", { [weak self] v in
                guard let self, let t = v.first, !t.isEmpty else { return }
                self.insert(t, coalesce: false)
            })])
    }

    @objc private func editLinePrompt() {
        endComposition()
        let line = lineOf(cursor)
        let r = lineRange(line)
        ask(title: "\(line + 1)번 줄 고치기", fields: [("줄 내용", ns.substring(with: r))],
            actions: [("바꾸기", { [weak self] v in
                guard let self, let t = v.first else { return }
                self.replace(r, with: t)
            })])
    }

    @objc private func findPrompt() {
        endComposition()
        ask(title: "찾기 · 바꾸기", message: "대소문자는 가리지 않습니다. 다음은 ⌘G, 이전은 ⌘⇧G.",
            fields: [("찾을 글", lastFind), ("바꿀 글 (모두 바꾸기만)", "")],
            actions: [
                ("찾기", { [weak self] v in self?.find(v.first ?? "") }),
                ("모두 바꾸기", { [weak self] v in
                    guard v.count == 2 else { return }
                    self?.replaceAll(v[0], with: v[1])
                })
            ])
    }

    @objc private func gotoLinePrompt() {
        endComposition()
        ask(title: "줄로 가기", message: "1 ~ \(lineStarts.count)", fields: [("줄 번호", "")],
            actions: [("가기", { [weak self] v in
                guard let self, let n = Int((v.first ?? "").trimmingCharacters(in: .whitespaces)) else { return }
                let line = max(0, min(self.lineStarts.count - 1, n - 1))
                self.moveCursor(to: self.lineRange(line).location, extend: false)
            })])
    }

    // MARK: 저장

    @objc private func saveTapped() {
        save(then: nil)
    }

    private func save(then: (() -> Void)?) {
        endComposition()
        if let why = readOnlyReason { flash(why); return }
        if case .failure(let err) = FileGuard.authorize(action: "write_text", paths: [filePath], notifyOnBlock: false) {
            alert("저장이 막혔습니다", err.localizedDescription)
            return
        }
        guard let data = codec.encode(storage.string) else {
            // 원래 인코딩에 없는 글자(예: Latin-1 파일에 한글)가 들어갔다.
            let a = UIAlertController(title: "\(codec.label) 로는 저장할 수 없습니다",
                                      message: "이 인코딩에 없는 글자가 들어 있습니다. UTF-8 로 바꿔 저장할까요?",
                                      preferredStyle: .alert)
            a.addAction(UIAlertAction(title: "UTF-8 로 저장", style: .default) { [weak self] _ in
                self?.codec.encoding = .utf8
                self?.updateStatus()
                self?.save(then: then)
            })
            a.addAction(UIAlertAction(title: "취소", style: .cancel))
            present(a, animated: true)
            return
        }

        var ok = false
        var why: String?
        do {
            try data.write(to: URL(fileURLWithPath: filePath), options: .atomic)
            ok = true
        } catch {
            why = error.localizedDescription
            // 권한이 없는 자리. 임시 파일에 쓰고 한 번에 바꿔 끼운다 —
            // 실패해도 원본이 남는다.
            do {
                try data.write(to: URL(fileURLWithPath: tempPath), options: .atomic)
                ok = RootHelperActs.mvtemp(filePath)
                if !ok { why = RootHelperActs.lastError ?? why }
            } catch {
                why = error.localizedDescription
            }
        }
        if ok {
            OperationLog.record(action: "EDIT_TEXT", path: filePath)
            dirty = false
            flash("저장했습니다")
            onSaved?()
            then?()
        } else {
            alert("저장하지 못했습니다", "원본은 그대로입니다.\n" + (why ?? ""))
        }
    }

    @objc private func closeRequested() {
        guard dirty else { close(); return }
        let a = UIAlertController(title: "저장하지 않은 고침이 있습니다", message: nil, preferredStyle: .alert)
        a.addAction(UIAlertAction(title: "저장하고 닫기", style: .default) { [weak self] _ in
            self?.save(then: { self?.close() })
        })
        a.addAction(UIAlertAction(title: "버리고 닫기", style: .destructive) { [weak self] _ in self?.close() })
        a.addAction(UIAlertAction(title: "계속 편집", style: .cancel))
        present(a, animated: true)
    }

    private func close() {
        blinkTimer?.invalidate()
        repeatTimer?.invalidate()
        onClose?()
    }

    // MARK: 알림

    private func alert(_ title: String, _ message: String) {
        let a = UIAlertController(title: title, message: message, preferredStyle: .alert)
        a.addAction(UIAlertAction(title: "확인", style: .default))
        present(a, animated: true)
    }

    private var flashText: String?
    private var flashTimer: Timer?

    /// 상태줄에 잠깐 띄운다. 창을 띄우면 손이 멈춘다.
    private func flash(_ text: String) {
        flashText = text
        updateStatus()
        flashTimer?.invalidate()
        flashTimer = Timer.scheduledTimer(withTimeInterval: 2.0, repeats: false) { [weak self] _ in
            self?.flashText = nil
            self?.updateStatus()
        }
    }

    private func updateTitle() {
        let name = (filePath as NSString).lastPathComponent
        titleLabel.text = (dirty ? "● " : "") + name + (readOnlyReason != nil ? "  (읽기만)" : "")
    }

    private func updateStatus() {
        let line = lineOf(cursor)
        let col = cursor - lineStarts[line] + 1
        var parts = [koreanInput ? "한" : "EN", "\(line + 1):\(col)"]
        if let sel = selection { parts.append("\(sel.length)자 선택") }
        parts.append(language == .plain ? "텍스트" : language.rawValue)
        parts.append(codec.label)
        parts.append(indentUnit == "\t" ? "탭" : "공백 \(indentUnit.count)")
        if let f = flashText { parts.insert(f, at: 0) }
        statusLabel.text = parts.joined(separator: "  ·  ")
    }

    // MARK: 도구 모음

    private func buildToolbar() {
        let items: [(String, Selector)] = [
            ("저장", #selector(saveTapped)),
            ("글 넣기", #selector(insertTextPrompt)),
            ("줄 고치기", #selector(editLinePrompt)),
            ("찾기", #selector(findPrompt)),
            ("줄로 가기", #selector(gotoLinePrompt)),
            ("되돌리기", #selector(undoTapped)),
            ("주석", #selector(commentTapped)),
            ("글자 크기", #selector(cycleFontSize)),
            ("단축키", #selector(showKeys)),
            ("닫기", #selector(closeRequested)),
        ]
        let buttons: [UIView] = items.map { title, sel in
            let b = ArgoBarButton.make(title: title)
            b.addTarget(self, action: sel, for: .primaryActionTriggered)
            return b
        }
        toolbar = UIStackView(arrangedSubviews: buttons)
        toolbar.axis = .horizontal
        toolbar.spacing = ArgoTheme.gutter() * 0.6
        toolbar.alignment = .center
        toolbar.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(toolbarSurface)
        toolbarSurface.addSubview(toolbar)
    }

    @objc private func undoTapped() { undo() }
    @objc private func commentTapped() { endComposition(); toggleComment() }

    @objc private func cycleFontSize() {
        let sizes: [CGFloat] = [20, 23, 26, 30, 34, 40]
        let i = sizes.firstIndex(where: { $0 >= fontSize }) ?? 0
        setFontSize(sizes[(i + 1) % sizes.count])
    }

    private func setFontSize(_ size: CGFloat) {
        endComposition()
        fontSize = max(16, min(48, size))
        UserDefaults.settings.set(Double(fontSize), forKey: "editorFontSize")
        let all = NSRange(location: 0, length: storage.length)
        storage.beginEditing()
        storage.addAttributes(baseAttributes(), range: all)
        ArgoSyntax.highlight(storage, range: all, language: language)
        storage.endEditing()
        selectionPainted = nil
        findHitsNeedPaint = true
        paintSelection()
        textView.layoutIfNeeded()
        refreshCaret(scroll: true)
    }

    @objc private func showKeys() {
        alert("편집기 단축키", """
        ⌘S 저장 · ⌘Z 되돌리기 · ⌘⇧Z 다시 하기 · Esc 닫기
        ⌘F 찾기·바꾸기 · ⌘G / ⌘⇧G 다음·이전 · ⌘L 줄로 가기
        ⌘A 전체 · ⌘C ⌘X ⌘V (선택 없으면 줄 단위)
        Tab / ⇧Tab 들여쓰기 · ⌘] ⌘[ 같음 · ⌘/ 주석
        ⌘D 줄 복제 · ⌘⇧K 줄 지우기 · ⌥↑ ⌥↓ 줄 옮기기
        ⌥← ⌥→ 낱말 · ⌘← ⌘→ 줄 처음·끝 · ⌘↑ ⌘↓ 문서 처음·끝
        ⇧ 를 같이 누르면 고르면서 간다 · PageUp/Down
        한/영: Caps Lock · ⌥ 한 번 · ⌃Space · 한/영 키 (상태줄 왼쪽에 한/EN)
        한글 상태에서 ⌥ 를 누른 채 치면 영문 · 글자 밑줄은 조합 중
        ⌘E 글 넣기 · ⌘⇧E 줄 고치기 · ⌘+ ⌘- 글자 크기

        리모컨: 방향키 커서 · 가운데 도구 모음 · 재생 글 넣기 · 메뉴 닫기
        """)
    }

    private func setToolbar(interactive: Bool) {
        if interactive { endComposition() }
        toolbarInteractive = interactive
        for sub in toolbar.arrangedSubviews { (sub as? UIControl)?.isEnabled = interactive }
        let a: CGFloat = interactive ? 1.0 : 0.0
        toolbar.alpha = a
        toolbarSurface.alpha = a
        setNeedsFocusUpdate()
        updateFocusIfNeeded()
    }

    // MARK: 키

    private func handlePresses(_ presses: Set<UIPress>) -> Bool {
        var handled = false
        for press in presses {
            if press.type == .menu { continue }   // menuPressed 가 받는다
            if let key = press.key {
                let code = key.keyCode.rawValue
                // ⌥(왼쪽 0xE2, 오른쪽 0xE6)를 혼자 눌렀다 떼면 한/영을 바꾼다.
                // 누른 채 다른 키를 치면(⌥← 낱말 이동, ⌥+글자 영문) 바꾸지 않는다.
                lonelyModifier = (code == 0xE2 || code == 0xE6) ? code : nil
                if handleKey(key) {
                    handled = true
                    startRepeat(for: key)
                }
            } else if handleRemote(press.type) {
                handled = true
            }
        }
        return handled
    }

    private func pressesFinished(_ presses: Set<UIPress>) {
        for p in presses {
            if let code = p.key?.keyCode.rawValue, code == 0xE2 || code == 0xE6, lonelyModifier == code {
                lonelyModifier = nil
                toggleInputLanguage()
            }
        }
        for p in presses where p.key?.keyCode == repeatKey {
            repeatTimer?.invalidate()
            repeatKey = nil
        }
    }

    /// 누르고 있으면 되풀이한다. tvOS 는 키를 누르고 있어도 한 번만 알려 준다.
    private func startRepeat(for key: UIKey) {
        repeatTimer?.invalidate()
        repeatKey = nil
        let mods = key.modifierFlags.intersection([.command, .control])
        let repeatable: Set<UIKeyboardHIDUsage> = [
            .keyboardLeftArrow, .keyboardRightArrow, .keyboardUpArrow, .keyboardDownArrow,
            .keyboardDeleteOrBackspace, .keyboardDeleteForward, .keyboardPageUp, .keyboardPageDown
        ]
        let isChar = mods.isEmpty && !key.characters.isEmpty
            && key.keyCode != .keyboardReturnOrEnter && key.keyCode != .keyboardTab
            && key.keyCode != .keyboardEscape
        guard repeatable.contains(key.keyCode) || isChar else { return }
        repeatKey = key.keyCode
        repeatTimer = Timer.scheduledTimer(withTimeInterval: 0.42, repeats: false) { [weak self] _ in
            guard let self, self.repeatKey == key.keyCode else { return }
            self.repeatTimer = Timer.scheduledTimer(withTimeInterval: 0.045, repeats: true) { [weak self] t in
                guard let self, self.repeatKey == key.keyCode else { t.invalidate(); return }
                _ = self.handleKey(key)
            }
        }
    }

    private func handleRemote(_ type: UIPress.PressType) -> Bool {
        if toolbarInteractive {
            return false   // 버튼 사이 이동은 포커스 엔진이 한다
        }
        endComposition()
        switch type {
        case .upArrow:    moveCursor(to: verticalStep(-1), extend: false, keepColumn: true)
        case .downArrow:  moveCursor(to: verticalStep(1), extend: false, keepColumn: true)
        case .leftArrow:  moveCursor(to: composedStep(false), extend: false)
        case .rightArrow: moveCursor(to: composedStep(true), extend: false)
        case .select:     setToolbar(interactive: true)
        case .playPause:  insertTextPrompt()
        default:          return false
        }
        return true
    }

    private func handleKey(_ key: UIKey) -> Bool {
        if toolbarInteractive { return false }
        let m = key.modifierFlags
        let cmd = m.contains(.command)
        let shift = m.contains(.shift)
        let alt = m.contains(.alternate)
        let ctrl = m.contains(.control)
        // 입력 언어가 한국어면 charactersIgnoringModifiers 가 'ㄴ' 처럼 온다.
        // 단축키는 키 자리에서 미국 배열 글자를 얻어 가린다.
        let ch = ArgoKeyMap.latin(key.keyCode, shift: false) ?? key.charactersIgnoringModifiers.lowercased()
        let code = key.keyCode.rawValue

        // 한/영 바꾸기: 한/영 키(LANG1), Caps Lock, ⌃Space.
        // ⌥ 는 떼었을 때 pressesFinished 에서 바꾼다.
        if code == 0x90 || code == 0x39 || (code == 0x2C && ctrl && !cmd) {
            toggleInputLanguage()
            return true
        }
        // 수정키만 눌린 것은 아무 일도 하지 않는다.
        if (0xE0...0xE7).contains(code) { return true }

        // 만드는 중인 한글: 자모와 지우기만 이어 가고, 나머지는 먼저 굳힌다.
        let latinTyped = ArgoKeyMap.latin(key.keyCode, shift: shift)
        let jamo: UInt16? = (koreanInput && !cmd && !ctrl && !alt)
            ? latinTyped.flatMap { $0.first }.flatMap { ArgoHangulComposer.jamo(forLatin: $0) }
            : nil
        if let jamo {
            composeJamo(jamo)
            return true
        }
        if key.keyCode == .keyboardDeleteOrBackspace, !cmd, !alt, composingRange != nil {
            composeBackspace()
            return true
        }
        endComposition()

        let pageLines = max(1, Int(textView.bounds.height / max(1, font.lineHeight * 1.25)) - 2)

        switch key.keyCode {
        case .keyboardLeftArrow:
            if cmd { moveCursor(to: smartHome(), extend: shift) }
            else if alt { moveCursor(to: wordStep(false), extend: shift) }
            else if !shift, let sel = selection { moveCursor(to: sel.location, extend: false) }
            else { moveCursor(to: composedStep(false), extend: shift) }
            return true
        case .keyboardRightArrow:
            if cmd { moveCursor(to: NSMaxRange(lineRange(lineOf(cursor))), extend: shift) }
            else if alt { moveCursor(to: wordStep(true), extend: shift) }
            else if !shift, let sel = selection { moveCursor(to: NSMaxRange(sel), extend: false) }
            else { moveCursor(to: composedStep(true), extend: shift) }
            return true
        case .keyboardUpArrow:
            if alt && !cmd { moveLines(up: true) }
            else if cmd { moveCursor(to: 0, extend: shift) }
            else { moveCursor(to: verticalStep(-1), extend: shift, keepColumn: true) }
            return true
        case .keyboardDownArrow:
            if alt && !cmd { moveLines(up: false) }
            else if cmd { moveCursor(to: ns.length, extend: shift) }
            else { moveCursor(to: verticalStep(1), extend: shift, keepColumn: true) }
            return true
        case .keyboardHome:
            moveCursor(to: cmd || ctrl ? 0 : smartHome(), extend: shift); return true
        case .keyboardEnd:
            moveCursor(to: cmd || ctrl ? ns.length : NSMaxRange(lineRange(lineOf(cursor))), extend: shift); return true
        case .keyboardPageUp:
            moveCursor(to: verticalStep(-pageLines), extend: shift, keepColumn: true); return true
        case .keyboardPageDown:
            moveCursor(to: verticalStep(pageLines), extend: shift, keepColumn: true); return true
        case .keyboardDeleteOrBackspace:
            if cmd {
                // 줄 처음까지 지운다.
                let start = lineRange(lineOf(cursor)).location
                if let sel = selection { replace(sel, with: "") }
                else if cursor > start { replace(NSRange(location: start, length: cursor - start), with: "") }
                else { backspace() }
            } else if alt {
                let from = wordStep(false)
                replace(NSRange(location: from, length: cursor - from), with: "")
            } else { backspace() }
            return true
        case .keyboardDeleteForward:
            deleteForward(); return true
        case .keyboardReturnOrEnter, .keypadEnter:
            if cmd { return false }
            newline(); return true
        case .keyboardTab:
            if ctrl || cmd { return false }
            if shift { outdentLines() }
            else if let sel = selection, lineOf(sel.location) != lineOf(NSMaxRange(sel)) { indentLines() }
            else { insert(indentUnit, coalesce: false) }
            return true
        case .keyboardEscape:
            // 메뉴로 오지 않는 키보드에서만 여기로 온다.
            menuPressed()
            return true
        default:
            break
        }

        if cmd {
            switch ch {
            case "s": save(then: nil)
            case "z": shift ? redo() : undo()
            case "y": redo()
            case "a": anchor = 0; cursor = ns.length; refreshCaret(scroll: false)
            case "c": copy(cut: false)
            case "x": copy(cut: true)
            case "v": paste()
            case "f": findPrompt()
            case "g": if lastFind.isEmpty { findPrompt() } else { find(lastFind, forward: !shift) }
            case "l": gotoLinePrompt()
            case "/": toggleComment()
            case "d": duplicateLines()
            case "k": if shift { deleteLines() } else { return false }
            case "]": indentLines()
            case "[": outdentLines()
            case "e": shift ? editLinePrompt() : insertTextPrompt()
            case "=", "+": setFontSize(fontSize + 2)
            case "-": setFontSize(fontSize - 2)
            case "w": closeRequested()
            default: return false
            }
            return true
        }
        if ctrl {
            // 이맥스식 몇 개. 맥에서 손에 익은 사람이 많다.
            switch ch {
            case "a": moveCursor(to: lineRange(lineOf(cursor)).location, extend: shift)
            case "e": moveCursor(to: NSMaxRange(lineRange(lineOf(cursor))), extend: shift)
            default: return false
            }
            return true
        }

        // 글자: 키 자리에서 얻은 미국 배열 글자를 쓴다. ⌥ 를 누르고 치면
        // 한글 상태에서도 영문이 들어간다.
        if let t = latinTyped {
            typeCharacter(t)
            return true
        }
        // 숫자판 같은 그 밖의 키는 시스템이 준 글자를 쓴다.
        let chars = key.characters
        guard !chars.isEmpty else { return false }
        // 보이지 않는 제어 글자는 넣지 않는다.
        if chars.unicodeScalars.allSatisfy({ CharacterSet.controlCharacters.contains($0) }) { return false }
        typeCharacter(chars)
        return true
    }

    // MARK: 한글 조합

    private func toggleInputLanguage() {
        endComposition()
        koreanInput.toggle()
        flash(koreanInput ? "한글 입력" : "영문 입력")
    }

    private func composeJamo(_ jamo: UInt16) {
        let settled = hangul.input(jamo)
        let text = settled + hangul.composing
        let target = composingRange ?? selection ?? NSRange(location: cursor, length: 0)
        replace(target, with: text, coalesce: true)
        markComposing()
    }

    private func composeBackspace() {
        guard let range = composingRange else { return }
        _ = hangul.deleteBackward()
        replace(range, with: hangul.composing, coalesce: true)
        markComposing()
    }

    /// 만드는 중인 글자 자리를 다시 잡고 밑줄을 긋는다.
    private func markComposing() {
        let len = (hangul.composing as NSString).length
        guard hangul.isComposing, len > 0, cursor >= len else {
            composingRange = nil
            hangul.reset()
            return
        }
        let r = NSRange(location: cursor - len, length: len)
        composingRange = r
        storage.addAttribute(.underlineStyle, value: NSUnderlineStyle.single.rawValue, range: r)
    }

    /// 조합을 끝낸다. 글자는 이미 들어가 있으므로 밑줄만 걷는다.
    private func endComposition() {
        guard let r = composingRange else { hangul.reset(); return }
        if NSMaxRange(r) <= storage.length {
            storage.removeAttribute(.underlineStyle, range: r)
        }
        composingRange = nil
        hangul.reset()
    }
}

// MARK: - SwiftUI 껍데기

@available(tvOS 13.0, *)
struct ArgoCodeEditorView: UIViewControllerRepresentable {
    let filePath: String
    @Binding var isPresented: Bool
    var onSaved: (() -> Void)? = nil

    func makeUIViewController(context: Context) -> ArgoCodeEditorController {
        let vc = ArgoCodeEditorController()
        vc.filePath = filePath
        vc.onClose = { isPresented = false }
        vc.onSaved = onSaved
        return vc
    }

    func updateUIViewController(_ vc: ArgoCodeEditorController, context: Context) {}
}

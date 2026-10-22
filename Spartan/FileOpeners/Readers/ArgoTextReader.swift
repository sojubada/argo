//
//  ArgoTextReader.swift
//  Argo
//
//  텍스트·로그·plist·json 읽기.
//
//  기존 TextView 는 편집기다. 줄마다 TextField 를 한 행씩 놓아서, 화면보다
//  긴 줄은 그냥 잘렸다 — 로그는 한 줄이 화면 몇 배라 사실상 읽을 수 없었다.
//
//  이것은 읽기 전용이고 UITextView 하나에 전부 흘려 넣는다. 줄이 접히고,
//  글꼴 크기를 바꿀 수 있고, 리모컨으로 한 화면씩 넘어간다.
//  고치려면 편집기를 따로 열면 된다.
//

import UIKit
import SwiftUI

@available(tvOS 13.0, *)
final class ArgoTextReaderController: UIViewController {

    var filePath: String = ""
    var onClose: (() -> Void)?
    /// 편집기로 넘길 때 부른다.
    var onEdit: (() -> Void)?

    /// 한 번에 읽어 들일 상한. 100MB 로그를 통째로 메모리에 올리면 죽는다.
    private static let maxBytes = 8 * 1024 * 1024

    private let textView = UITextView()
    private var canvas = PDFCanvasButton(type: .custom)

    private var toolbarSurface = ArgoSurfaceView.makeSurface()
    private var toolbar = UIStackView()
    private var statusLabel = UILabel()
    private var toolbarVisible = true
    private var hideTimer: Timer?

    private var fontSize: CGFloat = 28
    private var truncated = false
    private var lineCount = 0

    private var ext: String { (filePath as NSString).pathExtension.lowercased() }

    /// 로그·코드·설정 파일은 고정폭이 읽기 쉽다. 줄이 세로로 정렬된다.
    private var wantsMonospace: Bool {
        ["log", "json", "plist", "xml", "yml", "yaml", "conf", "cfg",
         "ini", "sh", "swift", "m", "h", "c", "py", "js", "css", "csv"].contains(ext)
    }

    // MARK: 화면

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = ArgoTheme.background()

        fontSize = CGFloat(UserDefaults.settings.object(forKey: "readerFontSize") as? Double ?? 28)

        textView.translatesAutoresizingMaskIntoConstraints = false
        // isEditable 은 tvOS 에 없다 — 이 플랫폼의 UITextView 는 애초에
        // 편집을 하지 않는다. 선택도 끈다.
        textView.isSelectable = false
        textView.isScrollEnabled = true
        // 리모컨 입력은 캔버스가 받는다. 텍스트 뷰가 가져가면 방향키가 엉킨다.
        textView.isUserInteractionEnabled = false
        textView.backgroundColor = .clear
        textView.textColor = ArgoTheme.textPrimary()
        textView.textContainerInset = UIEdgeInsets(top: ArgoTheme.screenMargin(),
                                                   left: ArgoTheme.screenMargin(),
                                                   bottom: ArgoTheme.screenMargin() * 2,
                                                   right: ArgoTheme.screenMargin())
        view.addSubview(textView)

        canvas.translatesAutoresizingMaskIntoConstraints = false
        canvas.addTarget(self, action: #selector(toggleToolbar), for: .primaryActionTriggered)
        canvas.onPress = { [weak self] type in self?.handlePress(type) ?? false }
        view.addSubview(canvas)

        buildToolbar()

        NSLayoutConstraint.activate([
            textView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            textView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            textView.topAnchor.constraint(equalTo: view.topAnchor),
            textView.bottomAnchor.constraint(equalTo: view.bottomAnchor),

            canvas.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            canvas.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            canvas.topAnchor.constraint(equalTo: view.topAnchor),
            canvas.bottomAnchor.constraint(equalTo: view.bottomAnchor),
        ])

        let g = view.safeAreaLayoutGuide
        NSLayoutConstraint.activate([
            toolbarSurface.centerXAnchor.constraint(equalTo: g.centerXAnchor),
            toolbarSurface.bottomAnchor.constraint(equalTo: g.bottomAnchor,
                                                   constant: -ArgoTheme.gutter()),
            toolbar.leadingAnchor.constraint(equalTo: toolbarSurface.leadingAnchor,
                                             constant: ArgoTheme.gutter()),
            toolbar.trailingAnchor.constraint(equalTo: toolbarSurface.trailingAnchor,
                                              constant: -ArgoTheme.gutter()),
            toolbar.topAnchor.constraint(equalTo: toolbarSurface.topAnchor,
                                         constant: ArgoTheme.gutter() * 0.5),
            toolbar.bottomAnchor.constraint(equalTo: toolbarSurface.bottomAnchor,
                                            constant: -ArgoTheme.gutter() * 0.5),
        ])
        view.bringSubviewToFront(toolbarSurface)

        let menu = UITapGestureRecognizer(target: self, action: #selector(menuPressed))
        menu.allowedPressTypes = [NSNumber(value: UIPress.PressType.menu.rawValue)]
        view.addGestureRecognizer(menu)

        loadFile()
        setToolbarVisible(true, animated: false)
    }

    override var preferredFocusEnvironments: [UIFocusEnvironment] { [canvas] }

    // MARK: 읽기

    private func loadFile() {
        let url = URL(fileURLWithPath: filePath)
        guard let handle = try? FileHandle(forReadingFrom: url) else {
            apply(text: "파일을 열 수 없습니다.\n\(filePath)")
            return
        }
        defer { try? handle.close() }

        let data = handle.readData(ofLength: Self.maxBytes)
        let size = (try? FileManager.default.attributesOfItem(atPath: filePath)[.size] as? Int) ?? 0
        truncated = (size ?? 0) > Self.maxBytes

        // plist 는 바이너리일 수 있다. 그때는 읽을 수 있는 모양으로 바꾼다.
        if ext == "plist", let pretty = prettyPlist(data) {
            apply(text: pretty)
            return
        }
        if ext == "json", let pretty = prettyJSON(data) {
            apply(text: pretty)
            return
        }

        if let text = decode(data) {
            apply(text: text)
        } else {
            apply(text: "글자로 읽을 수 없는 파일입니다. 헥스 편집기로 열어 보십시오.")
        }
    }

    /// 인코딩을 찍어서 읽는다. UTF-8 이 아닌 한글 파일(CP949)이 흔하다.
    private func decode(_ data: Data) -> String? {
        if let s = String(data: data, encoding: .utf8) { return s }
        let detected = NSString.stringEncoding(for: data, encodingOptions: nil,
                                               convertedString: nil, usedLossyConversion: nil)
        if detected != 0, let s = String(data: data, encoding: .init(rawValue: detected)) { return s }
        // 마지막으로 손실을 감수하고라도 보여 준다. 안 보이는 것보다 낫다.
        return String(data: data, encoding: .isoLatin1)
    }

    /// 바이너리 plist 도 읽을 수 있게 XML 모양으로 편다.
    private func prettyPlist(_ data: Data) -> String? {
        guard let obj = try? PropertyListSerialization.propertyList(
                from: data, options: [], format: nil) else { return nil }
        guard let xml = try? PropertyListSerialization.data(
                fromPropertyList: obj, format: .xml, options: 0) else { return nil }
        return String(data: xml, encoding: .utf8)
    }

    private func prettyJSON(_ data: Data) -> String? {
        guard let obj = try? JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed]),
              let pretty = try? JSONSerialization.data(withJSONObject: obj,
                                                       options: [.prettyPrinted, .sortedKeys]),
              let s = String(data: pretty, encoding: .utf8) else { return nil }
        return s
    }

    private func apply(text: String) {
        var body = text
        if truncated {
            body += "\n\n--- 여기까지만 읽었습니다 (8MB 상한) ---"
        }
        lineCount = body.reduce(into: 1) { n, c in if c == "\n" { n += 1 } }
        textView.attributedText = styled(body)
        updateStatus()
    }

    /// 거실에서 읽을 수 있게 줄 간격을 넓힌다. 기본값은 TV 에서 너무 빽빽하다.
    private func styled(_ body: String) -> NSAttributedString {
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineSpacing = fontSize * 0.35
        paragraph.paragraphSpacing = fontSize * 0.2
        paragraph.lineBreakMode = .byWordWrapping

        let font: UIFont = wantsMonospace
            ? (UIFont(name: "SF Mono Regular", size: fontSize)
               ?? UIFont.monospacedSystemFont(ofSize: fontSize, weight: .regular))
            : UIFont.systemFont(ofSize: fontSize)

        return NSAttributedString(string: body, attributes: [
            .font: font,
            .foregroundColor: ArgoTheme.textPrimary(),
            .paragraphStyle: paragraph
        ])
    }

    // MARK: 리모컨

    private func handlePress(_ type: UIPress.PressType) -> Bool {
        if type == .upArrow && toolbarVisible { return false }

        let page = textView.bounds.height * 0.85
        switch type {
        case .upArrow:    scrollBy(-page);      return true
        case .downArrow:  scrollBy(page);       return true
        // 좌우는 크게 건너뛴다. 긴 로그에서 한 화면씩 가면 끝이 없다.
        case .leftArrow:  scrollBy(-page * 10); return true
        case .rightArrow: scrollBy(page * 10);  return true
        case .playPause:  cycleFontSize();      return true
        default:          return false
        }
    }

    private func scrollBy(_ delta: CGFloat) {
        let maxY = max(0, textView.contentSize.height - textView.bounds.height)
        var y = textView.contentOffset.y + delta
        y = min(max(0, y), maxY)
        textView.setContentOffset(CGPoint(x: 0, y: y), animated: true)
        updateStatus()
    }

    @objc private func cycleFontSize() {
        let sizes: [CGFloat] = [22, 26, 28, 32, 38, 44]
        let i = sizes.firstIndex(where: { $0 >= fontSize }) ?? 0
        fontSize = sizes[(i + 1) % sizes.count]
        UserDefaults.settings.set(Double(fontSize), forKey: "readerFontSize")
        textView.attributedText = styled(textView.text ?? "")
        setToolbarVisible(true, animated: true)
        updateStatus()
    }

    // MARK: 도구 모음

    private func buildToolbar() {
        statusLabel.font = ArgoTheme.fontMonoBody()
        statusLabel.textColor = ArgoTheme.textSecondary()

        let top = ArgoBarButton.make(title: "처음")
        top.addTarget(self, action: #selector(goTop), for: .primaryActionTriggered)
        let bottom = ArgoBarButton.make(title: "끝")
        bottom.addTarget(self, action: #selector(goBottom), for: .primaryActionTriggered)
        let size = ArgoBarButton.make(title: "글자 크기")
        size.addTarget(self, action: #selector(cycleFontSize), for: .primaryActionTriggered)
        let edit = ArgoBarButton.make(title: "편집")
        edit.addTarget(self, action: #selector(editNow), for: .primaryActionTriggered)
        let close = ArgoBarButton.make(title: "닫기")
        close.addTarget(self, action: #selector(closeNow), for: .primaryActionTriggered)

        toolbar = UIStackView(arrangedSubviews: [top, bottom, statusLabel, size, edit, close])
        toolbar.axis = .horizontal
        toolbar.spacing = ArgoTheme.gutter()
        toolbar.alignment = .center
        toolbar.translatesAutoresizingMaskIntoConstraints = false

        view.addSubview(toolbarSurface)
        toolbarSurface.addSubview(toolbar)
    }

    @objc private func goTop() {
        textView.setContentOffset(.zero, animated: true)
        updateStatus()
    }

    @objc private func goBottom() {
        let maxY = max(0, textView.contentSize.height - textView.bounds.height)
        textView.setContentOffset(CGPoint(x: 0, y: maxY), animated: true)
        updateStatus()
    }

    private func updateStatus() {
        let maxY = max(1, textView.contentSize.height - textView.bounds.height)
        let percent = Int((textView.contentOffset.y / maxY * 100).rounded())
        statusLabel.text = "\(min(100, max(0, percent)))%  ·  \(lineCount)줄  ·  \(Int(fontSize))pt"
    }

    @objc private func toggleToolbar() {
        setToolbarVisible(!toolbarVisible, animated: true)
    }

    private func setToolbarVisible(_ visible: Bool, animated: Bool) {
        toolbarVisible = visible
        hideTimer?.invalidate()
        for sub in toolbar.arrangedSubviews {
            (sub as? UIControl)?.isEnabled = visible
        }
        if !visible { setNeedsFocusUpdate(); updateFocusIfNeeded() }

        let apply = {
            self.toolbar.alpha = visible ? 1 : 0
            self.toolbarSurface.alpha = visible ? 1 : 0
        }
        if animated { UIView.animate(withDuration: 0.2, animations: apply) } else { apply() }

        if visible {
            hideTimer = Timer.scheduledTimer(withTimeInterval: 8, repeats: false) { [weak self] _ in
                self?.setToolbarVisible(false, animated: true)
            }
        }
    }

    @objc private func menuPressed() {
        if toolbarVisible { setToolbarVisible(false, animated: true) } else { closeNow() }
    }

    @objc private func editNow() {
        hideTimer?.invalidate()
        onEdit?()
    }

    @objc private func closeNow() {
        hideTimer?.invalidate()
        onClose?()
    }
}

// MARK: - SwiftUI 껍데기

@available(tvOS 13.0, *)
struct ArgoTextReaderView: UIViewControllerRepresentable {
    let filePath: String
    let fileName: String
    @Binding var isPresented: Bool
    /// 편집기로 넘어갈 때.
    var onEdit: (() -> Void)? = nil

    func makeUIViewController(context: Context) -> ArgoTextReaderController {
        let vc = ArgoTextReaderController()
        vc.filePath = filePath.hasSuffix("/") ? filePath + fileName : filePath
        vc.onClose = { isPresented = false }
        vc.onEdit = {
            isPresented = false
            onEdit?()
        }
        return vc
    }

    func updateUIViewController(_ vc: ArgoTextReaderController, context: Context) {}
}

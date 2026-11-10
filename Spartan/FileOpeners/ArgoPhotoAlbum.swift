//
//  ArgoPhotoAlbum.swift
//  Argo
//
//  폴더 사진 앨범 보기. 사진 하나를 열면 같은 폴더의 사진들을
//  좌우로 넘겨 가며 본다(사진 앱처럼).
//
//  리모컨·키보드
//   - ← / → (클릭·쓸어넘기기·키보드 화살표): 이전·다음 사진
//   - 재생/일시정지: 슬라이드쇼 켜기·끄기 (4초 간격)
//   - 선택(가운데): 사진 정보 보이기·숨기기
//   - ↑ / ↓: 위 정보줄·아래 필름 띠 보이기
//   - 메뉴: 닫기
//
//  메모리: 원본을 통째로 풀지 않고 화면 크기에 맞춰 줄여 읽는다(ImageIO 썸네일).
//  아주 큰 사진(수천만 화소)도 화면 두 배(약 3840px) 안으로만 디코딩한다.
//

import SwiftUI
import UIKit
import ImageIO
import SVGWrapper

@available(tvOS 13.0, *)
final class ArgoPhotoAlbumController: UIViewController {

    static let imageExtensions: Set<String> = [
        "jpg", "jpeg", "png", "heic", "heif", "gif", "bmp", "tif", "tiff", "webp", "svg", "ico"
    ]

    var folder: String = ""
    var startName: String = ""

    private var names: [String] = []
    private var index = 0

    private let canvas = ArgoFocusCanvas()
    private let current = UIImageView()
    private let incoming = UIImageView()
    private let topBar = UIView()
    private let titleLabel = UILabel()
    private let infoLabel = UILabel()
    private let strip = UIStackView()
    private let stripBack = UIView()
    private var stripViews: [UIImageView] = []
    private let stripRadius = 4

    private let big = NSCache<NSString, UIImage>()
    private let thumbs = NSCache<NSString, UIImage>()
    private let loader = DispatchQueue(label: "argo.photo.loader", qos: .userInitiated)

    private var overlayVisible = true
    private var hideWork: DispatchWorkItem?
    private var slideshow: Timer?
    private var animating = false
    private var showInfo = false

    // MARK: 시작

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .black
        big.countLimit = 4
        thumbs.countLimit = 120

        for iv in [current, incoming] {
            iv.contentMode = .scaleAspectFit
            iv.clipsToBounds = true
            iv.frame = photoRect
            view.addSubview(iv)
        }
        incoming.isHidden = true

        // 위 정보줄
        // 사진 위에 얹히는 글자 줄 — 바탕은 투명, 밝은 사진 위에서도 읽히게 글자에 그림자만 준다.
        topBar.backgroundColor = .clear
        view.addSubview(topBar)
        titleLabel.textColor = .white
        titleLabel.font = .systemFont(ofSize: 30, weight: .semibold)
        titleLabel.lineBreakMode = .byTruncatingMiddle
        for l in [titleLabel, infoLabel] {
            l.layer.shadowColor = UIColor.black.cgColor
            l.layer.shadowOpacity = 0.9
            l.layer.shadowRadius = 4
            l.layer.shadowOffset = .zero
        }
        infoLabel.textColor = UIColor.white.withAlphaComponent(0.8)
        infoLabel.font = .systemFont(ofSize: 22)
        infoLabel.numberOfLines = 0
        topBar.addSubview(titleLabel)
        topBar.addSubview(infoLabel)

        // 아래 필름 띠: 지금 사진 앞뒤 4장씩
        stripBack.backgroundColor = UIColor.black.withAlphaComponent(0.55)
        stripBack.layer.cornerRadius = 16
        view.addSubview(stripBack)
        strip.axis = .horizontal
        strip.spacing = 12
        strip.alignment = .center
        stripBack.addSubview(strip)
        for _ in 0..<(stripRadius * 2 + 1) {
            let iv = UIImageView()
            iv.contentMode = .scaleAspectFill
            iv.clipsToBounds = true
            iv.layer.cornerRadius = 8
            iv.backgroundColor = UIColor.white.withAlphaComponent(0.08)
            iv.translatesAutoresizingMaskIntoConstraints = false
            iv.widthAnchor.constraint(equalToConstant: 150).isActive = true
            iv.heightAnchor.constraint(equalToConstant: 100).isActive = true
            strip.addArrangedSubview(iv)
            stripViews.append(iv)
        }

        // 입력은 화면 전체를 덮는 캔버스 하나가 받는다(PDF 뷰어와 같은 방식).
        canvas.frame = view.bounds
        canvas.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        canvas.onPress = { [weak self] type in self?.handlePress(type) ?? false }
        canvas.onSelect = { [weak self] in self?.toggleInfo() }
        view.addSubview(canvas)

        // 리모컨 터치면 쓸어넘기기
        for dir in [UISwipeGestureRecognizer.Direction.left, .right] {
            let g = UISwipeGestureRecognizer(target: self, action: #selector(swiped(_:)))
            g.direction = dir
            canvas.addGestureRecognizer(g)
        }

        // 메뉴 = 닫기 (UIKit 전체 화면으로 띄우므로 직접 닫는다)
        let menu = UITapGestureRecognizer(target: self, action: #selector(closeNow))
        menu.allowedPressTypes = [NSNumber(value: UIPress.PressType.menu.rawValue)]
        view.addGestureRecognizer(menu)

        loadNames()
        show(index: index, direction: 0)
    }

    var onClose: (() -> Void)?

    @objc private func closeNow() {
        if let onClose = onClose { onClose() } else { dismiss(animated: true) }
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        // 포커스를 캔버스로 — 이게 없으면 좌우 입력이 아무 데도 가지 않는다.
        setNeedsFocusUpdate()
        updateFocusIfNeeded()
        ArgoDiag.log("photo", "표시 focused=\(canvas.isFocused)")
    }

    /// 사진이 놓이는 영역. 아래 필름 띠가 보이는 동안은 그만큼 비워 사진과 겹치지 않게 한다.
    private var photoRect: CGRect {
        let b = view.bounds
        let bottom: CGFloat = overlayVisible ? 170 : 0
        return CGRect(x: 0, y: 0, width: b.width, height: max(0, b.height - bottom))
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        if !animating {
            current.frame = photoRect
        }
        let w = view.bounds.width, h = view.bounds.height
        topBar.frame = CGRect(x: 60, y: 40, width: w - 120, height: showInfo ? 150 : 70)
        titleLabel.frame = CGRect(x: 24, y: 14, width: topBar.bounds.width - 48, height: 40)
        infoLabel.frame = CGRect(x: 24, y: 58, width: topBar.bounds.width - 48, height: 80)
        infoLabel.isHidden = !showInfo
        let sw = CGFloat(stripViews.count) * 150 + CGFloat(stripViews.count - 1) * 12
        stripBack.frame = CGRect(x: (w - sw) / 2 - 16, y: h - 150, width: sw + 32, height: 120)
        strip.frame = CGRect(x: 16, y: 10, width: sw, height: 100)
    }

    override var preferredFocusEnvironments: [UIFocusEnvironment] { [canvas] }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        slideshow?.invalidate()
        slideshow = nil
        UIApplication.shared.isIdleTimerDisabled = false
    }

    private func loadNames() {
        let fm = FileManager.default
        let all = (try? fm.contentsOfDirectory(atPath: folder)) ?? []
        var list = all.filter { name in
            !name.hasPrefix(".") && ArgoPhotoAlbumController.imageExtensions.contains((name as NSString).pathExtension.lowercased())
        }
        if !list.contains(startName) { list.append(startName) }
        list.sort { $0.localizedStandardCompare($1) == .orderedAscending }
        names = list
        index = list.firstIndex(of: startName) ?? 0
        ArgoDiag.log("photo", "앨범 열기 \(folder) — 사진 \(names.count)장, \(index + 1)번째부터")
    }

    private func path(_ i: Int) -> String {
        (folder as NSString).appendingPathComponent(names[i])
    }

    // MARK: 입력

    private func handlePress(_ type: UIPress.PressType) -> Bool {
        switch type {
        case .leftArrow:
            step(-1, user: true); return true
        case .rightArrow:
            step(1, user: true); return true
        case .upArrow, .downArrow:
            setOverlay(type == .upArrow ? true : !overlayVisible, autoHide: false)
            return true
        case .playPause:
            toggleSlideshow(); return true
        default:
            return false
        }
    }

    @objc private func swiped(_ g: UISwipeGestureRecognizer) {
        step(g.direction == .left ? 1 : -1, user: true)
    }

    private func step(_ delta: Int, user: Bool) {
        guard !names.isEmpty, !animating else { return }
        if user, slideshow != nil { toggleSlideshow() }
        var next = index + delta
        if next < 0 { next = names.count - 1 }
        if next >= names.count { next = 0 }
        guard next != index else { return }
        index = next
        show(index: index, direction: delta)
    }

    private func toggleInfo() {
        showInfo.toggle()
        updateLabels()
        setOverlay(true, autoHide: !showInfo)
        view.setNeedsLayout()
    }

    private func toggleSlideshow() {
        if let t = slideshow {
            t.invalidate()
            slideshow = nil
            UIApplication.shared.isIdleTimerDisabled = false
            ArgoDiag.log("photo", "슬라이드쇼 끔")
        } else {
            slideshow = Timer.scheduledTimer(withTimeInterval: 4, repeats: true) { [weak self] _ in
                self?.step(1, user: false)
            }
            UIApplication.shared.isIdleTimerDisabled = true
            ArgoDiag.log("photo", "슬라이드쇼 켬")
            setOverlay(false, autoHide: false)
        }
        updateLabels()
    }

    // MARK: 보이기

    private func show(index i: Int, direction: Int) {
        guard names.indices.contains(i) else { return }
        updateLabels()
        updateStrip()
        if slideshow == nil { setOverlay(true, autoHide: true) }
        let key = names[i] as NSString
        if let img = big.object(forKey: key) {
            display(img, direction: direction)
        } else {
            // 큰 사진은 읽는 데 시간이 걸린다. 먼저 필름 띠 썸네일이라도 띄운다.
            if let t = thumbs.object(forKey: key) { display(t, direction: direction) }
            let p = path(i)
            let maxPx = max(view.bounds.width, view.bounds.height) * 2
            loader.async { [weak self] in
                let img = ArgoPhotoAlbumController.decode(p, maxPixel: maxPx)
                DispatchQueue.main.async {
                    guard let self = self else { return }
                    if let img = img { self.big.setObject(img, forKey: key) }
                    guard self.index == i else { return }
                    if let img = img {
                        if self.thumbs.object(forKey: key) != nil { self.current.image = img }
                        else { self.display(img, direction: direction) }
                    } else {
                        self.current.image = nil
                        self.titleLabel.text = (self.titleLabel.text ?? "") + "  — 열 수 없는 사진"
                        ArgoDiag.log("photo", "열기 실패 \(p)")
                    }
                }
            }
        }
        preload(i - 1); preload(i + 1)
    }

    private func display(_ img: UIImage, direction: Int) {
        guard direction != 0, current.image != nil, view.window != nil else {
            current.image = img
            return
        }
        let w = view.bounds.width
        incoming.image = img
        incoming.isHidden = false
        incoming.frame = photoRect.offsetBy(dx: direction > 0 ? w : -w, dy: 0)
        animating = true
        UIView.animate(withDuration: 0.32, delay: 0, options: [.curveEaseOut], animations: {
            self.incoming.frame = self.photoRect
            self.current.frame = self.photoRect.offsetBy(dx: direction > 0 ? -w : w, dy: 0)
        }, completion: { _ in
            self.current.image = img
            self.current.frame = self.photoRect
            self.incoming.isHidden = true
            self.incoming.image = nil
            self.animating = false
        })
    }

    private func preload(_ raw: Int) {
        guard !names.isEmpty else { return }
        let i = (raw + names.count) % names.count
        let key = names[i] as NSString
        guard big.object(forKey: key) == nil else { return }
        let p = path(i)
        let maxPx = max(view.bounds.width, view.bounds.height) * 2
        loader.async { [weak self] in
            guard let img = ArgoPhotoAlbumController.decode(p, maxPixel: maxPx) else { return }
            DispatchQueue.main.async { self?.big.setObject(img, forKey: key) }
        }
    }

    private func updateLabels() {
        guard names.indices.contains(index) else { return }
        let mark = slideshow != nil ? "▶︎ " : ""
        titleLabel.text = "\(mark)\(names[index])    \(index + 1) / \(names.count)"
        if showInfo {
            let p = path(index)
            let size = (try? FileManager.default.attributesOfItem(atPath: p)[.size] as? Int64) ?? 0
            var dims = ""
            if let src = CGImageSourceCreateWithURL(URL(fileURLWithPath: p) as CFURL, nil),
               let props = CGImageSourceCopyPropertiesAtIndex(src, 0, nil) as? [CFString: Any],
               let w = props[kCGImagePropertyPixelWidth] as? Int,
               let h = props[kCGImagePropertyPixelHeight] as? Int {
                dims = "\(w) × \(h)  ·  "
                if let exif = props[kCGImagePropertyExifDictionary] as? [CFString: Any],
                   let date = exif[kCGImagePropertyExifDateTimeOriginal] as? String {
                    dims += "\(date)  ·  "
                }
            }
            let mb = ByteCountFormatter.string(fromByteCount: size, countStyle: .file)
            infoLabel.text = "\(dims)\(mb)\n\(p)"
        }
    }

    private func updateStrip() {
        guard !names.isEmpty else { return }
        for (slot, iv) in stripViews.enumerated() {
            let off = slot - stripRadius
            let raw = index + off
            iv.layer.borderWidth = off == 0 ? 4 : 0
            iv.layer.borderColor = UIColor.white.cgColor
            iv.alpha = off == 0 ? 1 : 0.6
            guard raw >= 0, raw < names.count else {
                iv.image = nil
                iv.isHidden = true
                continue
            }
            iv.isHidden = false
            let key = names[raw] as NSString
            if let t = thumbs.object(forKey: key) {
                iv.image = t
                continue
            }
            iv.image = nil
            let p = path(raw)
            loader.async { [weak self] in
                guard let t = ArgoPhotoAlbumController.decode(p, maxPixel: 300) else { return }
                DispatchQueue.main.async {
                    guard let self = self else { return }
                    self.thumbs.setObject(t, forKey: key)
                    // 그 사이 넘어갔으면 칸이 바뀌었을 수 있다.
                    if self.index + off == raw { iv.image = t }
                }
            }
        }
    }

    private func setOverlay(_ on: Bool, autoHide: Bool) {
        hideWork?.cancel()
        overlayVisible = on
        UIView.animate(withDuration: 0.25) {
            self.topBar.alpha = on ? 1 : 0
            self.stripBack.alpha = on ? 1 : 0
            if !self.animating { self.current.frame = self.photoRect }
        }
        if on && autoHide && !showInfo {
            // 자동으로 숨길 때는 위 글자줄만 — 아래 띠까지 숨기면 사진 크기가 3초마다 바뀌어 눈에 거슬린다.
            let work = DispatchWorkItem { [weak self] in
                UIView.animate(withDuration: 0.25) { self?.topBar.alpha = 0 }
            }
            hideWork = work
            DispatchQueue.main.asyncAfter(deadline: .now() + 3, execute: work)
        }
    }

    // MARK: 읽기

    /// 원본을 통째로 풀지 않고 maxPixel 안으로 줄여 읽는다. 방향(EXIF)도 맞춘다.
    static func decode(_ path: String, maxPixel: CGFloat) -> UIImage? {
        if (path as NSString).pathExtension.lowercased() == "svg" {
            guard let svg = try? SVGDocument(fileURL: URL(fileURLWithPath: path)) else { return nil }
            return svg.uiImage(configuration: SVGDocument.ImageCreationConfiguration(scale: 1.0, orientation: .up))
        }
        let url = URL(fileURLWithPath: path) as CFURL
        guard let src = CGImageSourceCreateWithURL(url, [kCGImageSourceShouldCache: false] as CFDictionary) else {
            return nil
        }
        let opts: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceThumbnailMaxPixelSize: max(64, Int(maxPixel)),
        ]
        guard let cg = CGImageSourceCreateThumbnailAtIndex(src, 0, opts as CFDictionary) else { return nil }
        return UIImage(cgImage: cg)
    }
}

// MARK: - SwiftUI 껍데기

@available(tvOS 13.0, *)
struct ArgoPhotoAlbumView: UIViewControllerRepresentable {
    /// 폴더 경로(끝에 / 가 있어도 된다)와 처음 볼 파일 이름
    let folder: String
    let fileName: String

    func makeUIViewController(context: Context) -> ArgoPhotoAlbumController {
        let vc = ArgoPhotoAlbumController()
        vc.folder = folder
        vc.startName = fileName
        return vc
    }

    func updateUIViewController(_ vc: ArgoPhotoAlbumController, context: Context) {}
}

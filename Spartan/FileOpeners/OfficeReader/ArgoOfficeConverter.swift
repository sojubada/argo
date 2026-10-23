//
//  ArgoOfficeConverter.swift
//  Argo
//
//  오피스 문서를 기기 안에서 PDF 로 바꿉니다.
//
//  WebKit 안에서 JS 로 그리던 길과 완전히 다릅니다. 변환이 끝나면 남는 것은
//  PDF 하나뿐이고, 그건 이미 잘 도는 CoreGraphics 뷰어가 맡습니다 —
//  쪽 넘김도 두 쪽 보기도 맞춤도 공짜로 따라옵니다.
//

import UIKit

@available(tvOS 13.0, *)
enum ArgoOfficeConverter {

    enum Failure: Error {
        case unreadable
        case conversionFailed(Int32)
    }

    /// 변환기에 맡길 확장자.
    ///
    /// pptx 는 아직 뺍니다 — 슬라이드 제목이 통째로 빠집니다. 자리표시자를
    /// 건너뛰는 분기에 걸리는 것으로 보이고, 고쳐지기 전까지는 기존
    /// WebKit 뷰어가 더 낫습니다.
    static let handled: Set<String> = ["docx", "xlsx", "xls"]

    static func handles(_ path: String) -> Bool {
        handled.contains((path as NSString).pathExtension.lowercased())
    }

    // MARK: 폰트

    private static var fontsReady = false

    /// 한글이 빈칸으로 나오지 않게 폰트를 먹입니다.
    ///
    /// tvOS 에는 fontconfig 가 없고, 변환기가 들고 있는 시스템 폰트 탐색도
    /// 여기서는 돌지 않습니다. 대신 기기에 있는 폰트를 우리가 읽어 넘깁니다.
    /// 사용자가 따로 넣어 둔 폰트도 같은 길로 들어옵니다.
    static func registerFonts() {
        guard !fontsReady else { return }
        fontsReady = true

        var candidates = [
            "/System/Library/Fonts/AppleSDGothicNeo.ttc",
            "/System/Library/Fonts/Core/AppleSDGothicNeo.ttc",
            "/System/Library/Fonts/CoreAddition/AppleSDGothicNeo.ttc",
            "/System/Library/Fonts/Core/SFUI.ttf",
            "/System/Library/Fonts/LanguageSupport/AppleSDGothicNeo.ttc",
        ]

        // 사용자가 직접 넣는 자리. 기기에 없는 글꼴이 필요할 때 쓸 수 있게
        // 둡니다 — 파일 탐색기로 넣고 바로 쓰면 됩니다.
        let userDir = "/var/mobile/Media/Argo/fonts"
        if let found = try? FileManager.default.contentsOfDirectory(atPath: userDir) {
            candidates += found
                .filter { ["ttf", "ttc", "otf"].contains(($0 as NSString).pathExtension.lowercased()) }
                .map { userDir + "/" + $0 }
        }

        var loaded = 0
        for path in candidates {
            guard FileManager.default.fileExists(atPath: path),
                  let data = FileManager.default.contents(atPath: path) else { continue }
            let name = ((path as NSString).lastPathComponent as NSString).deletingPathExtension
            let ok = data.withUnsafeBytes { raw -> Int32 in
                guard let base = raw.bindMemory(to: UInt8.self).baseAddress else { return -1 }
                return argoconv_register_font(name, base, data.count)
            }
            if ok == ARGOCONV_OK { loaded += 1 }
            ArgoDiag.log("font", "\(name) \(data.count)바이트 → \(ok == ARGOCONV_OK ? "등록" : "실패")")
        }
        ArgoDiag.log("font", "등록한 폰트 \(loaded)개")
    }

    // MARK: 변환

    /// 문서를 PDF 로 바꾸고 그 파일 경로를 돌려줍니다.
    ///
    /// 바이트로 주고받습니다. tvOS 의 std 는 파일 메타데이터 쪽이 아직
    /// 비어 있어서, 읽고 쓰는 일은 이쪽에서 합니다.
    static func convertToPDF(at path: String) throws -> String {
        registerFonts()

        guard let input = FileManager.default.contents(atPath: path) else {
            throw Failure.unreadable
        }

        var out: UnsafeMutablePointer<UInt8>?
        var outLen = 0
        let started = Date()

        let code = input.withUnsafeBytes { raw -> Int32 in
            guard let base = raw.bindMemory(to: UInt8.self).baseAddress else { return ARGOCONV_BAD_ARGS }
            return argoconv_to_pdf(base, input.count, &out, &outLen)
        }

        guard code == ARGOCONV_OK, let buffer = out, outLen > 0 else {
            ArgoDiag.log("convert", "실패 code=\(code) \((path as NSString).lastPathComponent)")
            throw Failure.conversionFailed(code)
        }
        defer { argoconv_free(buffer, outLen) }

        let pdf = Data(bytes: buffer, count: outLen)
        let name = ((path as NSString).lastPathComponent as NSString).deletingPathExtension
        let dest = NSTemporaryDirectory() + "/argo-\(name).pdf"
        try pdf.write(to: URL(fileURLWithPath: dest), options: .atomic)

        ArgoDiag.log("convert", "\((path as NSString).lastPathComponent) "
                               + "\(input.count) → \(outLen)바이트, "
                               + String(format: "%.2f초", Date().timeIntervalSince(started)))
        return dest
    }
}

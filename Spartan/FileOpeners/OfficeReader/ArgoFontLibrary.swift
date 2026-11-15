//
//  ArgoFontLibrary.swift
//  Argo
//
//  문서를 그릴 글꼴을 고른다.
//
//  사용자가 폰트 폴더(/var/mobile/Media/Argo/fonts)에 글꼴을 넣어 두면,
//  문서를 열 때마다 그 문서가 쓰는 글꼴 이름을 읽어 같은 것을 찾아 넣는다.
//  없으면 닮은 것(명조는 명조, 고딕은 고딕 …)을 골라 그 이름으로 넣는다.
//  그것도 없으면 앱에 든 Pretendard 와 기기의 Apple SD 산돌고딕으로 그린다.
//
//  - 글꼴을 통째로 올려 두지 않는다. 한 개에 10~20MB 라 수십 개면 메모리가
//    모자란다. 문서마다 필요한 것만 넣고 다음 문서 전에 비운다.
//  - 폴더를 처음 훑을 때 이름표만 읽어 색인을 만들고, 파일이 바뀌지 않았으면
//    다음부터는 색인을 그대로 쓴다.
//  - 하위 폴더도 훑는다. 글꼴 묶음을 폴더째 넣어도 된다.
//

import Foundation

struct ArgoFontFace: Codable {
    let path: String
    let index: Int
    let names: [String]
    let weight: Int
    let italic: Bool
    let mono: Bool
    let hangul: Bool
    /// PDF 에 심을 수 있는 TrueType 윤곽. 오피스 문서 쪽 변환기는 이것만 쓴다.
    let ttf: Bool

    var primaryName: String { names.first ?? (path as NSString).lastPathComponent }
    var isBold: Bool { weight >= 600 }
}

@available(tvOS 13.0, *)
enum ArgoFontLibrary {

    static let userDir = "/var/mobile/Media/Argo/fonts"
    private static let indexName = ".argo-font-index.json"

    /// 글꼴을 찾을 폴더들.
    ///
    /// 기기의 파일 체계는 대소문자를 가린다(`Fonts` 와 `fonts` 는 다른 폴더다). 그래서
    /// 표준 자리 한 곳만 보면, 글꼴을 넣고도 "사용자 폴더 0개" 가 된다. Media 바로 아래에서
    /// 이름이 argo·fonts 인 폴더를 대소문자 가리지 않고 찾아 같이 본다.
    private static var cachedRoots: [String]?

    static func userRoots() -> [String] {
        if let cachedRoots { return cachedRoots }
        let fm = FileManager.default
        var roots: [String] = []
        func add(_ p: String) {
            var isDir: ObjCBool = false
            guard fm.fileExists(atPath: p, isDirectory: &isDir), isDir.boolValue else { return }
            // 이미 보고 있는 폴더의 하위면 또 넣지 않는다(두 번 읽지 않게).
            guard !roots.contains(where: { p == $0 || p.hasPrefix($0 + "/") }) else { return }
            roots.append(p)
        }
        add(userDir)
        let media = "/var/mobile/Media"
        for name in (try? fm.contentsOfDirectory(atPath: media)) ?? [] {
            let low = name.lowercased()
            let path = (media as NSString).appendingPathComponent(name)
            if low == "fonts" { add(path) }
            if low == "argo" {
                for sub in (try? fm.contentsOfDirectory(atPath: path)) ?? [] where sub.lowercased() == "fonts" {
                    add((path as NSString).appendingPathComponent(sub))
                }
            }
        }
        add((NSHomeDirectory() as NSString).appendingPathComponent("Documents/fonts"))
        cachedRoots = roots
        return roots
    }

    /// 이 얼굴이 사용자가 넣은 글꼴인가(앱에 든 것·기기 것과 가르기 위해).
    static func isUserFace(_ face: ArgoFontFace) -> Bool {
        userRoots().contains { face.path.hasPrefix($0 + "/") }
    }

    /// 어떤 변환기에 넣을 글꼴인가. 오피스 쪽은 TrueType 만 심을 수 있다.
    enum Consumer { case office, hangul }

    // MARK: 색인

    private struct IndexFile: Codable {
        var version = 1
        var files: [String: Entry] = [:]
        struct Entry: Codable {
            var size: Int64
            var modified: Double
            var faces: [ArgoFontFace]
        }
    }

    private static var cached: [ArgoFontFace]?

    /// 폰트 폴더가 없으면 만든다. 무엇을 넣으면 되는지 적은 안내 파일도 둔다.
    static func ensureFolder() {
        let fm = FileManager.default
        if !fm.fileExists(atPath: userDir) {
            try? fm.createDirectory(atPath: userDir, withIntermediateDirectories: true)
        }
        let readme = (userDir as NSString).appendingPathComponent("README.txt")
        if !fm.fileExists(atPath: readme) {
            let text = """
            Argo 폰트 폴더 / Argo font folder

            문서를 그릴 글꼴(.ttf · .otf · .ttc)을 이 폴더에 넣으십시오. 하위 폴더도 됩니다.
            문서가 쓰는 글꼴과 같은 이름이 있으면 그것을, 없으면 닮은 글꼴을 골라 그립니다.
            엑셀·워드·파워포인트 문서는 .ttf (TrueType) 만 쓸 수 있습니다.

            Put .ttf / .otf / .ttc fonts here (subfolders are fine). Argo uses the font a
            document asks for when it is here, and a similar one when it is not.
            Office documents can only use .ttf (TrueType) fonts.
            """
            try? text.write(toFile: readme, atomically: true, encoding: .utf8)
        }
    }

    /// 쓸 수 있는 글꼴 얼굴 전부. 사용자 폴더 → 앱에 든 것 → 기기 것 순서다.
    static func allFaces() -> [ArgoFontFace] {
        // 앱을 켠 채로 글꼴을 넣어도 다음 문서부터 보이게. 폴더(와 바로 아래 폴더)의 수정 시각이
        // 바뀌었으면 다시 훑는다. 전에는 설정의 '다시 읽기' 를 누르거나 앱을 다시 켜야 했다.
        // (서명은 다시 훑은 '뒤' 에 잰다. 훑으면서 색인 파일을 쓰면 폴더 시각이 바뀌어,
        //  앞에서 재면 문서마다 다시 훑었다.)
        if folderSignature() != lastSignature { cached = nil; cachedRoots = nil; nameIndex = nil }
        if let cached { return cached }
        ensureFolder()
        var faces = indexedUserFaces()
        // ArgoHanja: 한자(4,899자, Noto Sans KR 에서 한자만 추려냄). Pretendard 에 한자가 없어
        // '新'·'社' 가 네모로 나오던 것을 글자 대체로 메운다 — 글꼴 이름으로 고르지는 않는다.
        // LiberationSans: Arial 과 글자 폭·모양이 같은 글꼴(OFL). 'Arial' 을 이것으로 그린다.
        // 나눔스퀘어_ac·나눔고딕 ExtraBold: 회사 문서(pptx 표지·제목)에서 자주 쓰는데 윈도에 없는
        // 네이버 글꼴(OFL — 앱에 넣어 재배포 가능, Nanum-LICENSE.txt 동봉).
        for name in ["Pretendard-Regular", "Pretendard-Bold",
                     "ArgoHanja-Regular", "LiberationSans-Regular", "LiberationSans-Bold",
                     "NanumSquare_acL", "NanumSquare_acR", "NanumSquare_acB", "NanumSquare_acEB",
                     "NanumGothic-ExtraBold"] {
            if let p = Bundle.main.path(forResource: name, ofType: "ttf") { faces += readFaces(p) }
        }
        for p in systemCandidates where FileManager.default.fileExists(atPath: p) {
            faces += readFaces(p)
        }
        cached = faces
        lastSignature = folderSignature()
        let mine = faces.filter(isUserFace)
        // 오피스 문서(워드·엑셀·파워포인트)에는 TrueType 만 심을 수 있다. OTF 는 한글 문서에만 쓴다.
        let ttf = mine.filter { $0.ttf }.count
        ArgoDiag.log("font", "글꼴 얼굴 \(faces.count)개 (사용자 폴더 \(mine.count)개 중 오피스용 TrueType \(ttf)개, 앱·기기 \(faces.count - mine.count)개)")
        return faces
    }

    private static var lastSignature = ""

    /// 글꼴 폴더들과 그 바로 아래 폴더들의 수정 시각. 파일을 넣거나 빼면 그 폴더 시각이 바뀐다.
    private static func folderSignature() -> String {
        let fm = FileManager.default
        var parts: [String] = []
        let roots = cachedRoots ?? userRoots()
        for root in roots {
            func stamp(_ p: String) {
                let m = (try? fm.attributesOfItem(atPath: p)[.modificationDate] as? Date)?.timeIntervalSince1970 ?? 0
                parts.append("\(p)=\(m)")
            }
            stamp(root)
            for sub in ((try? fm.contentsOfDirectory(atPath: root)) ?? []).prefix(200) {
                let p = (root as NSString).appendingPathComponent(sub)
                var isDir: ObjCBool = false
                if fm.fileExists(atPath: p, isDirectory: &isDir), isDir.boolValue { stamp(p) }
            }
        }
        return parts.joined(separator: "|")
    }

    /// 폴더 내용이 바뀌었을 수 있을 때. 다음에 allFaces 를 부르면 다시 훑는다.
    static func invalidate() {
        cached = nil
        cachedRoots = nil
    }

    private static let systemCandidates = [
        "/System/Library/Fonts/AppleSDGothicNeo.ttc",
        "/System/Library/Fonts/Core/AppleSDGothicNeo.ttc",
        "/System/Library/Fonts/CoreAddition/AppleSDGothicNeo.ttc",
        "/System/Library/Fonts/LanguageSupport/AppleSDGothicNeo.ttc",
        "/System/Library/Fonts/Core/SFUI.ttf",
    ]

    private static func indexedUserFaces() -> [ArgoFontFace] {
        let fm = FileManager.default
        let indexPath = (userDir as NSString).appendingPathComponent(indexName)
        var index = (try? Data(contentsOf: URL(fileURLWithPath: indexPath)))
            .flatMap { try? JSONDecoder().decode(IndexFile.self, from: $0) } ?? IndexFile()
        var seen = Set<String>()
        var changed = false
        var out: [ArgoFontFace] = []

        for root in userRoots() {
            var files = 0      // 폴더 안 파일 전부
            var fonts = 0      // 글꼴 확장자인 것
            var failed = 0     // 글꼴인데 이름표를 못 읽은 것
            var faceCount = 0
            let walker = fm.enumerator(atPath: root)
            while let rel = walker?.nextObject() as? String {
                files += 1
                let name = (rel as NSString).lastPathComponent
                // 맥에서 복사하면 생기는 짝 파일(._이름.ttf)은 글꼴이 아니다.
                if name.hasPrefix("._") { continue }
                let ext = (rel as NSString).pathExtension.lowercased()
                guard ["ttf", "otf", "ttc", "otc"].contains(ext) else { continue }
                fonts += 1
                let path = (root as NSString).appendingPathComponent(rel)
                guard let attrs = try? fm.attributesOfItem(atPath: path) else { failed += 1; continue }
                let size = (attrs[.size] as? NSNumber)?.int64Value ?? 0
                let mod = (attrs[.modificationDate] as? Date)?.timeIntervalSince1970 ?? 0
                seen.insert(path)
                if let e = index.files[path], e.size == size, e.modified == mod {
                    out += e.faces
                    faceCount += e.faces.count
                    if e.faces.isEmpty { failed += 1 }
                    continue
                }
                let faces = readFaces(path)
                if faces.isEmpty { failed += 1 }
                index.files[path] = .init(size: size, modified: mod, faces: faces)
                changed = true
                out += faces
                faceCount += faces.count
            }
            ArgoDiag.log("font", "폴더 \(root): 파일 \(files)개 · 글꼴 파일 \(fonts)개 · 얼굴 \(faceCount)개 · 못 읽음 \(failed)개")
        }
        if out.isEmpty {
            ArgoDiag.log("font", "사용자 글꼴 0개 — 넣을 자리: \(userDir) (README.txt 가 있는 폴더)")
            for (dir, n) in findFontsNearby() {
                ArgoDiag.log("font", "글꼴 파일을 여기서 봤습니다: \(dir) — \(n)개. 이 폴더를 \(userDir) 로 옮기거나 그 안에 넣으십시오.")
            }
        }
        // 지운 파일은 색인에서도 뺀다.
        for path in index.files.keys where !seen.contains(path) {
            index.files.removeValue(forKey: path)
            changed = true
        }
        if changed, let data = try? JSONEncoder().encode(index) {
            try? data.write(to: URL(fileURLWithPath: indexPath), options: .atomic)
        }
        return out
    }

    /// 사용자 글꼴이 하나도 없을 때, Media 아래 두 단계까지만 훑어 글꼴 파일이 어디 있는지 찾아본다.
    /// 어디에 넣었는지 로그로 알려 주기 위한 것이다(읽어서 쓰지는 않는다).
    private static func findFontsNearby() -> [(String, Int)] {
        let fm = FileManager.default
        let media = "/var/mobile/Media"
        var counts: [String: Int] = [:]
        func scan(_ dir: String, depth: Int) {
            guard depth <= 2, counts.count < 40 else { return }
            for name in (try? fm.contentsOfDirectory(atPath: dir)) ?? [] {
                if name.hasPrefix(".") { continue }
                let path = (dir as NSString).appendingPathComponent(name)
                var isDir: ObjCBool = false
                guard fm.fileExists(atPath: path, isDirectory: &isDir) else { continue }
                if isDir.boolValue {
                    scan(path, depth: depth + 1)
                } else if ["ttf", "otf", "ttc", "otc"].contains((name as NSString).pathExtension.lowercased()) {
                    counts[dir, default: 0] += 1
                }
            }
        }
        scan(media, depth: 0)
        return counts.sorted { $0.value > $1.value }.prefix(3).map { ($0.key, $0.value) }
    }

    /// 글꼴 파일 하나의 이름표를 읽는다. 파일을 메모리에 올리지 않고 비춰 본다.
    static func readFaces(_ path: String) -> [ArgoFontFace] {
        guard let data = try? Data(contentsOf: URL(fileURLWithPath: path), options: .alwaysMapped) else { return [] }
        struct Raw: Decodable {
            let i: Int; let names: [String]; let weight: Int
            let italic: Bool; let mono: Bool; let hangul: Bool; let ttf: Bool
        }
        var cap = 64 * 1024
        for _ in 0..<2 {
            var buffer = [UInt8](repeating: 0, count: cap)
            let n = data.withUnsafeBytes { raw -> Int in
                guard let base = raw.bindMemory(to: UInt8.self).baseAddress else { return 0 }
                return buffer.withUnsafeMutableBufferPointer { argoconv_font_faces(base, data.count, $0.baseAddress, cap) }
            }
            if n > cap { cap = n; continue }
            guard n > 0, let list = try? JSONDecoder().decode([Raw].self, from: Data(buffer[0..<n])) else { return [] }
            return list.map {
                ArgoFontFace(path: path, index: $0.i, names: $0.names, weight: $0.weight,
                             italic: $0.italic, mono: $0.mono, hangul: $0.hangul, ttf: $0.ttf)
            }
        }
        return []
    }

    // MARK: 문서에 맞추기

    /// 문서 하나를 그리기 전에 부른다. 등록을 비우고, 기본 글꼴과 그 문서가 원하는
    /// 글꼴(또는 닮은 것)을 넣는다. 바꿔 넣은 것들을 사람이 읽을 줄로 돌려준다.
    @discardableResult
    static func prepare(for document: Data, consumer: Consumer) -> [String] {
        argoconv_clear_fonts()
        let faces = allFaces()

        // 기본: 모든 글자를 그릴 수 있게 받쳐 주는 글꼴. 이름은 예전과 같다.
        var registered = Set<String>()
        // 서로 다른 글꼴 파일을 합쳐 이만큼까지만 넣는다. 같은 파일을 여러 이름으로 넣는 것은
        // 변환기가 바이트를 함께 쓰므로 세지 않는다. 넘으면 그 이름은 받쳐 주는 글꼴로 그린다.
        // (문서 하나가 글꼴 이름 56개를 써서 수백 MB 를 올리다 앱이 꺼진 일이 있다.)
        var filesIn = Set<String>()
        var bytesIn = 0
        let budget = 160 * 1024 * 1024
        // 이 글꼴 파일(+ttc 안 번호)을 처음 넣은 이름. 같은 파일을 다른 이름으로 또 쓸 때는
        // 바이트를 다시 넘기지 않고 그 이름에 별칭만 건다(넘길 때마다 변환기가 통째로 복사했다).
        var firstAlias: [String: String] = [:]
        func register(_ face: ArgoFontFace, as alias: String) {
            let key = alias.lowercased()
            guard !registered.contains(key) else { return }
            let fileKey = face.path + "#\(face.index)"
            if let first = firstAlias[fileKey], argoconv_register_font_alias(alias, first) == 0 {
                registered.insert(key)
                return
            }
            guard let data = try? Data(contentsOf: URL(fileURLWithPath: face.path), options: .alwaysMapped)
            else { return }
            if !filesIn.contains(face.path) {
                if bytesIn + data.count > budget, !filesIn.isEmpty {
                    ArgoDiag.log("font", "글꼴 메모리 한도로 넣지 않음: \(alias) (\((face.path as NSString).lastPathComponent), \(data.count / 1_048_576)MB)")
                    return
                }
                filesIn.insert(face.path)
                bytesIn += data.count
            }
            registered.insert(key)
            let rc = data.withUnsafeBytes { raw -> Int32 in
                guard let base = raw.bindMemory(to: UInt8.self).baseAddress else { return -1 }
                return argoconv_register_font(alias, base, data.count)
            }
            if rc == 0, firstAlias[fileKey] == nil { firstAlias[fileKey] = alias }
        }
        for face in faces where !isUserFace(face) && face.index == 0 {
            let file = ((face.path as NSString).lastPathComponent as NSString).deletingPathExtension.lowercased()
            // 나눔 글꼴은 문서가 그 이름을 쓸 때만 넣는다(받침 글꼴로는 Pretendard 가 맡는다).
            if file.hasPrefix("nanum") { continue }
            register(face, as: file)
        }

        var report: [String] = []
        var lines: [String] = []
        for wanted in documentFonts(document) {
            guard let pick = resolve(wanted, in: faces, consumer: consumer) else { continue }
            register(pick.regular, as: wanted)
            if let bold = pick.bold { register(bold, as: wanted + " bold") }
            _ = argoconv_font_alias(wanted, pick.regular.primaryName)
            if !pick.exact {
                report.append("\(wanted) → \(pick.regular.primaryName)")
            }
            lines.append("\(wanted)→\(pick.regular.primaryName)\(pick.exact ? "" : "*")")
        }
        // D. 글꼴 하나당 한 줄이던 것을 문서당 한 줄로. * 는 닮은 글꼴(대체)이다.
        // 대체 글꼴 폭 보정(fontprofile)은 대체된 글꼴에만 적용된다.
        if !lines.isEmpty {
            ArgoDiag.log("font", "문서 글꼴 \(lines.count)개: " + lines.joined(separator: ", ")
                         + (report.isEmpty ? "" : " · 폭 보정 적용 \(report.count)개"))
        }
        return report
    }

    /// 문서가 쓰는 글꼴 이름들.
    static func documentFonts(_ document: Data) -> [String] {
        var cap = 16 * 1024
        for _ in 0..<2 {
            var buffer = [UInt8](repeating: 0, count: cap)
            let n = document.withUnsafeBytes { raw -> Int in
                guard let base = raw.bindMemory(to: UInt8.self).baseAddress else { return 0 }
                return buffer.withUnsafeMutableBufferPointer { argoconv_doc_fonts(base, document.count, $0.baseAddress, cap) }
            }
            if n > cap { cap = n; continue }
            guard n > 0 else { return [] }
            let names = (try? JSONDecoder().decode([String].self, from: Data(buffer[0..<n]))) ?? []
            // 워드 문서의 언어 표시(ko-KR, en-US, x-none)가 글꼴 이름 자리에 섞여 온다. 글꼴이 아니다.
            return names.filter { !isLanguageTag($0) && !isStyleName($0) }
        }
        return []
    }

    /// 워드 스타일 이름(Normal, heading 1 …)이 글꼴 자리에 섞여 오는 것.
    static func isStyleName(_ s: String) -> Bool {
        let t = s.trimmingCharacters(in: .whitespaces).lowercased()
        if ["normal", "title", "subtitle", "default paragraph font", "table normal", "no list"].contains(t) { return true }
        return t.range(of: "^(heading|toc|index) [0-9]$", options: .regularExpression) != nil
    }

    static func isLanguageTag(_ s: String) -> Bool {
        let t = s.trimmingCharacters(in: .whitespaces)
        if t.lowercased() == "x-none" { return true }
        return t.range(of: "^[A-Za-z]{2,3}(-[A-Za-z0-9]{2,8})+$", options: .regularExpression) != nil
    }

    struct Pick {
        let regular: ArgoFontFace
        let bold: ArgoFontFace?
        /// 같은 글꼴을 찾았는가. 아니면 닮은 것으로 바꾼 것이다.
        let exact: Bool
    }

    /// 이름으로 찾고, 없으면 닮은 것을 고른다.
    /// 이름(정규화) → 얼굴 번호, 얼굴마다 갈래·장식체 여부. 얼굴 600개 × 문서 글꼴 수십 개를
    /// 매번 문자열로 훑던 것을 한 번만 계산한다.
    private static var nameIndex: [String: [Int]]?
    private static var indexedCount = -1
    private static var faceCategory: [Category] = []
    private static var faceDecorative: [Bool] = []

    private static func ensureIndex(_ faces: [ArgoFontFace]) {
        if nameIndex != nil, indexedCount == faces.count { return }
        var idx: [String: [Int]] = [:]
        for (i, f) in faces.enumerated() {
            var keys = Set<String>()
            for n in f.names { keys.insert(normalize(n)); keys.insert(normalize(stripStyle(n))) }
            for k in keys { idx[k, default: []].append(i) }
        }
        nameIndex = idx
        indexedCount = faces.count
        faceCategory = faces.map { category(of: $0.names) }
        faceDecorative = faces.map { isDecorative($0.names) }
    }

    static func resolve(_ wanted: String, in faces: [ArgoFontFace], consumer: Consumer) -> Pick? {
        ensureIndex(faces)
        let ok: (Int) -> Bool = { consumer == .hangul || faces[$0].ttf }
        guard faces.indices.contains(where: ok) else { return nil }
        let keys = Set([normalize(wanted)] + aliases(of: wanted))
        func named(_ ks: [String]) -> [ArgoFontFace] {
            var seen = Set<Int>()
            var out: [ArgoFontFace] = []
            for k in ks { for i in nameIndex?[k] ?? [] where ok(i) && seen.insert(i).inserted { out.append(faces[i]) } }
            return out
        }

        // 1. 같은 이름 (이름표의 어느 말로든)
        let same = named(Array(keys))
        if !same.isEmpty {
            return Pick(regular: bestRegular(same), bold: bestBold(same), exact: true)
        }

        // 2. 닮은 갈래 (명조·고딕·손글씨·고정폭·제목용)
        let want = category(of: [wanted])
        // 2-1. 갈래마다 '무난한 대표 글꼴' 이 있으면 그것. 예전에는 폴더에서 처음 만난 것을 골라
        //      고딕 문서가 HY목각파임B(장식체)로 그려졌다.
        for pref in preferred(for: want) {
            let hit = named([pref])
            if !hit.isEmpty {
                let regular = bestRegular(hit)
                return Pick(regular: want == .display ? (bestBold(hit) ?? regular) : regular,
                            bold: want == .display ? nil : bestBold(hit), exact: false)
            }
        }
        // 2-2. 대표 글꼴이 하나도 없으면 같은 갈래에서. 장식체(목각·엽서·매직 …)는 고르지 않는다.
        let usableIdx = faces.indices.filter(ok)
        let plain = usableIdx.filter { !faceDecorative[$0] }
        let poolIdx = plain.isEmpty ? usableIdx : plain
        func of(_ c: Category) -> [ArgoFontFace] { poolIdx.filter { faceCategory[$0] == c }.map { faces[$0] } }
        var candidates = of(want)
        if candidates.isEmpty && want == .display { candidates = of(.sans) }
        if candidates.isEmpty && want != .sans { candidates = of(.sans) }
        if candidates.isEmpty { candidates = poolIdx.map { faces[$0] } }
        // 한글을 그릴 수 있는 것을 먼저 둔다. 한글 문서가 대부분이고, 영문 글꼴로
        // 바꾸면 한글 부분이 다른 글꼴로 갈라져 보인다.
        let withHangul = candidates.filter { $0.hangul }
        if !withHangul.isEmpty { candidates = withHangul }
        // 같은 가족끼리 묶어 첫 가족에서 보통·굵게를 고른다.
        guard let first = candidates.first(where: { !$0.italic && !$0.isBold }) ?? candidates.first else { return nil }
        let family = candidates.filter { $0.names.first.map(normalize) == first.names.first.map(normalize) }
        let familyOrFirst = family.isEmpty ? [first] : family
        let regular = bestRegular(familyOrFirst)
        let bold = want == .display ? nil : bestBold(familyOrFirst)
        return Pick(regular: want == .display ? (bestBold(familyOrFirst) ?? regular) : regular,
                    bold: bold, exact: false)
    }

    /// 갈래별 대표 글꼴(정규화한 이름). 앞에 있을수록 먼저 고른다. 끝의 Pretendard·산돌고딕은 앱·기기에 늘 있다.
    private static func preferred(for c: Category) -> [String] {
        switch c {
        case .serif:
            return ["함초롬바탕", "hcrbatang", "한컴바탕", "hancombatang", "바탕", "batang", "나눔명조", "nanummyeongjo",
                    "본명조", "notoserifkr", "sourcehanserifkr", "kopub바탕체", "kopubbatang"]
        case .sans:
            return ["맑은고딕", "malgungothic", "함초롬돋움", "hcrdotum", "나눔고딕", "nanumgothic", "한컴돋움", "hancomdotum",
                    "돋움", "dotum", "굴림", "gulim", "본고딕", "notosanskr", "sourcehansanskr", "pretendard", "applesdgothicneo"]
        case .display:
            return ["hy헤드라인m", "hyheadlinem", "hyheadlinemedium", "휴먼둥근헤드라인", "hy견고딕", "hygothicextra", "맑은고딕", "malgungothic", "pretendard"]
        case .brush:
            return ["궁서", "gungsuh", "hy궁서b", "hygungsob", "휴먼옛체"]
        case .mono:
            return ["d2coding", "굴림체", "gulimche", "돋움체", "dotumche", "consolas", "couriernew", "liberationmono", "sfmono"]
        }
    }

    /// 제목·꾸밈용 글꼴. 본문 대신 그리면 문서가 엉뚱해 보인다.
    private static func isDecorative(_ names: [String]) -> Bool {
        let all = names.map(normalize).joined(separator: " ")
        return ["목각", "mokgak", "엽서", "yeopseo", "매직", "magic", "엑스포", "expo", "옛체", "그래픽", "graphic",
                "샤넬", "소슬", "튼튼", "모음", "wingding", "webding", "symbol", "marlett", "dingbat", "emoji"]
            .contains { all.contains($0) }
    }

    private static func bestRegular(_ faces: [ArgoFontFace]) -> ArgoFontFace {
        faces.min { score($0, weight: 400) < score($1, weight: 400) } ?? faces[0]
    }

    private static func bestBold(_ faces: [ArgoFontFace]) -> ArgoFontFace? {
        let bold = faces.filter { $0.isBold && !$0.italic }
        return bold.min { score($0, weight: 700) < score($1, weight: 700) }
    }

    private static func score(_ f: ArgoFontFace, weight: Int) -> Int {
        abs(f.weight - weight) + (f.italic ? 500 : 0) + (f.index == 0 ? 0 : 50)
    }

    // MARK: 이름 다루기

    static func normalize(_ s: String) -> String {
        String(s.lowercased().unicodeScalars.filter { !CharacterSet(charactersIn: " -_·.").contains($0) })
    }

    /// "나눔고딕 Bold" → "나눔고딕". 굵기 말을 떼고 가족 이름만 본다.
    private static func stripStyle(_ s: String) -> String {
        var t = s
        for w in [" Regular", " Bold", " Light", " Medium", " Thin", " ExtraBold", " Black", " Heavy",
                  " SemiBold", " ExtraLight", " Italic", " Oblique"] {
            if t.hasSuffix(w) { t = String(t.dropLast(w.count)) }
        }
        return t
    }

    /// 한국어 이름과 영어 이름이 다른 글꼴. 이름표에 둘 다 있는 경우가 많지만,
    /// 한쪽만 적어 둔 파일도 있어 짝을 알아 둔다.
    private static let pairs: [[String]] = [
        ["맑은고딕", "malgungothic"], ["바탕", "batang"], ["바탕체", "batangche"],
        ["굴림", "gulim"], ["굴림체", "gulimche"], ["돋움", "dotum"], ["돋움체", "dotumche"],
        ["궁서", "gungsuh"], ["궁서체", "gungsuhche"], ["새굴림", "newgulim"],
        ["함초롬바탕", "hcrbatang"], ["함초롬돋움", "hcrdotum"],
        ["나눔고딕", "nanumgothic"], ["나눔명조", "nanummyeongjo"], ["나눔바른고딕", "nanumbarungothic"],
        ["나눔스퀘어", "nanumsquare"], ["본고딕", "sourcehansanskr", "notosanskr", "notosanscjkkr"],
        ["본명조", "sourcehanserifkr", "notoserifkr", "notoserifcjkkr"],
        ["애플산돌고딕네오", "applesdgothicneo"],
        // Arial·Helvetica 계열은 글자 폭이 같은 Liberation Sans 로 그린다(앱에 들어 있다).
        ["arial", "liberationsans", "helvetica", "arialmt"], ["한컴바탕", "hancombatang"], ["한컴돋움", "hancomdotum"],
    ]

    private static func aliases(of name: String) -> [String] {
        let n = normalize(name)
        return pairs.first { $0.contains(n) } ?? []
    }

    enum Category { case serif, sans, brush, mono, display }

    /// 이름으로 갈래를 짐작한다. 맞는 말이 없으면 고딕(sans)으로 본다.
    static func category(of names: [String]) -> Category {
        let all = names.map(normalize).joined(separator: " ")
        func has(_ words: [String]) -> Bool { words.contains { all.contains($0) } }
        if has(["궁서", "gungsuh", "붓", "손글씨", "필기", "흘림", "펜글씨", "brush", "script", "handwriting", "pen", "calligraph", "붓글씨"]) { return .brush }
        if has(["mono", "coding", "courier", "consolas", "d2coding", "고정폭", "바탕체", "굴림체", "돋움체", "batangche", "gulimche", "dotumche", "menlo"]) { return .mono }
        if has(["헤드라인", "headline", "견고딕", "태고딕", "impact", "display", "poster"]) { return .display }
        if has(["명조", "바탕", "myeongjo", "myungjo", "batang", "serif", "times", "garamond", "georgia", "cambria", "mincho", "song", "신명", "순명", "궁체", "明", "ming", "宋", "serif"]) {
            // "sans serif" 는 고딕이다.
            if all.contains("sans") { return .sans }
            return .serif
        }
        return .sans
    }
}

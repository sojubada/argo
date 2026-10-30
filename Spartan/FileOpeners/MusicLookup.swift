//
//  MusicLookup.swift
//  Argo
//
//  음악 화면이 쓰는 것들: 앨범 표지·가사 찾기, 가사(LRC) 읽기, 표지 색 뽑기.
//
//  표지 찾는 순서
//    1) 파일 안의 그림  2) 같은 폴더의 cover/folder/front.jpg 나 같은 이름의 그림
//    3) 내려받아 둔 것  4) 온라인 (iTunes 검색 — 열쇠 없이 쓰는 공개 검색)
//  가사 찾는 순서
//    1) 같은 이름의 .lrc  2) 파일 안의 가사  3) 내려받아 둔 것  4) 온라인 (LRCLIB — 공개 가사 목록)
//
//  온라인으로 보내는 것은 곡 제목·가수·앨범 이름·길이뿐이다. 설정에서 끌 수 있다.
//  내려받은 것은 /var/mobile/Media/Argo/cache/ 에 둔다.
//

import Foundation
import UIKit
import AVFoundation
import CoreImage

struct LyricLine: Equatable {
    let time: Double
    let text: String
}

struct Lyrics: Equatable {
    /// 시간이 붙은 줄 (없으면 빈 배열)
    var synced: [LyricLine] = []
    /// 시간 없는 가사
    var plain: String = ""

    var isEmpty: Bool { synced.isEmpty && plain.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
}

struct TrackInfo: Equatable {
    var title: String
    var artist: String
    var album: String
    var duration: Double
}

enum MusicLookup {
    static let onlineKey = "argoMusicOnline"

    /// 온라인 찾기를 쓸지 (처음에는 켬)
    static var onlineEnabled: Bool {
        get { UserDefaults.standard.object(forKey: onlineKey) as? Bool ?? true }
        set { UserDefaults.standard.set(newValue, forKey: onlineKey) }
    }

    static let audioExtensions: Set<String> = ["mp3", "m4a", "aac", "wav", "aif", "aiff", "caf", "flac", "alac", "m4b", "mp2", "ac3"]

    private static let cacheRoot = "/var/mobile/Media/Argo/cache"

    private static func cacheDir(_ kind: String) -> String? {
        let dir = (cacheRoot as NSString).appendingPathComponent(kind)
        try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        return FileManager.default.fileExists(atPath: dir) ? dir : nil
    }

    private static func key(_ info: TrackInfo) -> String {
        let raw = "\(info.artist.lowercased())|\(info.album.lowercased())|\(info.title.lowercased())"
        // 짧고 겹치지 않을 이름 (FNV-1a)
        var h: UInt64 = 0xcbf29ce484222325
        for b in raw.utf8 {
            h ^= UInt64(b)
            h = h &* 0x100000001b3
        }
        return String(h, radix: 16)
    }

    // MARK: - 곡 정보

    /// 파일의 태그. 없으면 파일 이름("가수 - 제목")에서 짐작한다.
    static func trackInfo(asset: AVAsset, fileName: String) -> TrackInfo {
        var title = "", artist = "", album = ""
        for item in asset.commonMetadata {
            guard let key = item.commonKey else { continue }
            let value = item.stringValue ?? ""
            switch key {
            case .commonKeyTitle: title = value
            case .commonKeyArtist: artist = value
            case .commonKeyAlbumName: album = value
            default: break
            }
        }
        let base = (fileName as NSString).deletingPathExtension
        if title.isEmpty || artist.isEmpty {
            let parts = base.components(separatedBy: " - ")
            if parts.count >= 2 {
                if artist.isEmpty { artist = parts[0].trimmingCharacters(in: .whitespaces) }
                if title.isEmpty { title = parts.dropFirst().joined(separator: " - ").trimmingCharacters(in: .whitespaces) }
            }
        }
        if title.isEmpty {
            // "01 제목" 처럼 앞에 붙은 트랙 번호는 뗀다.
            title = base.replacingOccurrences(of: #"^\d{1,3}[\s._-]+"#, with: "", options: .regularExpression)
        }
        let seconds = asset.duration.seconds
        return TrackInfo(title: title, artist: artist, album: album, duration: seconds.isFinite ? seconds : 0)
    }

    // MARK: - 표지

    static func embeddedArtwork(asset: AVAsset) -> UIImage? {
        for item in asset.commonMetadata where item.commonKey == .commonKeyArtwork {
            if let data = item.dataValue, let img = UIImage(data: data) { return img }
        }
        return nil
    }

    static func folderArtwork(audioPath: String) -> UIImage? {
        let dir = (audioPath as NSString).deletingLastPathComponent
        let base = ((audioPath as NSString).lastPathComponent as NSString).deletingPathExtension
        guard let files = try? FileManager.default.contentsOfDirectory(atPath: dir) else { return nil }
        let images = files.filter { ["jpg", "jpeg", "png", "webp"].contains(($0 as NSString).pathExtension.lowercased()) }
        let lower = images.map { ($0, ($0 as NSString).deletingPathExtension.lowercased()) }
        let preferred = [base.lowercased(), "cover", "folder", "front", "album", "albumart", "albumartsmall"]
        for want in preferred {
            if let hit = lower.first(where: { $0.1 == want }) {
                if let img = UIImage(contentsOfFile: (dir as NSString).appendingPathComponent(hit.0)) { return img }
            }
        }
        if let any = lower.first(where: { $0.1.hasPrefix("albumart") || $0.1.contains("cover") }) {
            return UIImage(contentsOfFile: (dir as NSString).appendingPathComponent(any.0))
        }
        return nil
    }

    /// 내려받아 둔 표지, 없으면 iTunes 검색으로 찾아 내려받는다.
    static func onlineArtwork(_ info: TrackInfo) async -> UIImage? {
        let file = cacheDir("artwork").map { ($0 as NSString).appendingPathComponent(key(info) + ".jpg") }
        if let file, let img = UIImage(contentsOfFile: file) { return img }
        guard onlineEnabled, !info.title.isEmpty || !info.album.isEmpty else { return nil }

        // 앨범으로 먼저, 안 되면 곡으로.
        var queries: [(String, String)] = []
        if !info.album.isEmpty { queries.append(("\(info.artist) \(info.album)", "album")) }
        queries.append(("\(info.artist) \(info.title)", "song"))
        for (term, entity) in queries {
            var comps = URLComponents(string: "https://itunes.apple.com/search")!
            comps.queryItems = [
                URLQueryItem(name: "term", value: term.trimmingCharacters(in: .whitespaces)),
                URLQueryItem(name: "entity", value: entity),
                URLQueryItem(name: "limit", value: "5"),
                URLQueryItem(name: "country", value: Locale.current.regionCode ?? "KR"),
            ]
            guard let url = comps.url,
                  let (data, _) = try? await URLSession.shared.data(from: url),
                  let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let results = json["results"] as? [[String: Any]], !results.isEmpty else { continue }
            // 가수 이름이 맞는 것을 먼저 고른다.
            let artistLower = info.artist.lowercased()
            let pick = results.first(where: { ($0["artistName"] as? String)?.lowercased().contains(artistLower) == true && !artistLower.isEmpty }) ?? results[0]
            guard let small = pick["artworkUrl100"] as? String else { continue }
            let big = small.replacingOccurrences(of: "100x100bb", with: "1000x1000bb")
            guard let artURL = URL(string: big),
                  let (imgData, _) = try? await URLSession.shared.data(from: artURL),
                  let img = UIImage(data: imgData) else { continue }
            if let file { try? imgData.write(to: URL(fileURLWithPath: file)) }
            return img
        }
        return nil
    }

    // MARK: - 가사

    static func localLyrics(audioPath: String, asset: AVAsset) -> Lyrics? {
        let lrc = ((audioPath as NSString).deletingPathExtension as NSString).appendingPathExtension("lrc") ?? ""
        if let text = readText(lrc) {
            let parsed = parseLRC(text)
            if !parsed.isEmpty { return parsed }
        }
        if let text = asset.lyrics, !text.isEmpty {
            let parsed = parseLRC(text)
            return parsed.synced.isEmpty ? Lyrics(plain: text) : parsed
        }
        return nil
    }

    static func onlineLyrics(_ info: TrackInfo) async -> Lyrics? {
        let file = cacheDir("lyrics").map { ($0 as NSString).appendingPathComponent(key(info) + ".lrc") }
        if let file, let text = readText(file) {
            let parsed = parseLRC(text)
            return parsed.isEmpty ? nil : parsed
        }
        guard onlineEnabled, !info.title.isEmpty else { return nil }

        func fetch(_ path: String, _ items: [URLQueryItem]) async -> Any? {
            var comps = URLComponents(string: "https://lrclib.net\(path)")!
            comps.queryItems = items
            guard let url = comps.url else { return nil }
            var req = URLRequest(url: url)
            req.setValue("Argo (tvOS file browser)", forHTTPHeaderField: "User-Agent")
            guard let (data, resp) = try? await URLSession.shared.data(for: req),
                  (resp as? HTTPURLResponse)?.statusCode == 200 else { return nil }
            return try? JSONSerialization.jsonObject(with: data)
        }
        var record: [String: Any]?
        if !info.artist.isEmpty {
            var items = [URLQueryItem(name: "track_name", value: info.title), URLQueryItem(name: "artist_name", value: info.artist)]
            if !info.album.isEmpty { items.append(URLQueryItem(name: "album_name", value: info.album)) }
            if info.duration > 0 { items.append(URLQueryItem(name: "duration", value: String(Int(info.duration.rounded())))) }
            record = await fetch("/api/get", items) as? [String: Any]
        }
        if record == nil {
            let q = "\(info.artist) \(info.title)".trimmingCharacters(in: .whitespaces)
            if let list = await fetch("/api/search", [URLQueryItem(name: "q", value: q)]) as? [[String: Any]] {
                // 길이가 가장 가까운 것
                record = list.min(by: { a, b in
                    let da = abs(((a["duration"] as? Double) ?? 0) - info.duration)
                    let db = abs(((b["duration"] as? Double) ?? 0) - info.duration)
                    return da < db
                })
            }
        }
        guard let record else { return nil }
        let synced = record["syncedLyrics"] as? String ?? ""
        let plain = record["plainLyrics"] as? String ?? ""
        let text = synced.isEmpty ? plain : synced
        guard !text.isEmpty else { return nil }
        if let file { try? text.write(toFile: file, atomically: true, encoding: .utf8) }
        let parsed = parseLRC(text)
        return parsed.synced.isEmpty ? Lyrics(plain: plain.isEmpty ? text : plain) : parsed
    }

    /// 텍스트 파일 읽기 — UTF-8 이 아니면 한국어 옛 인코딩(CP949)으로.
    private static func readText(_ path: String) -> String? {
        guard let data = FileManager.default.contents(atPath: path), !data.isEmpty else { return nil }
        if let s = String(data: data, encoding: .utf8) { return s }
        let cp949 = String.Encoding(rawValue: CFStringConvertEncodingToNSStringEncoding(CFStringEncoding(CFStringEncodings.dosKorean.rawValue)))
        if let s = String(data: data, encoding: cp949) { return s }
        return String(data: data, encoding: .utf16)
    }

    /// LRC: "[01:23.45]가사" (한 줄에 시간이 여럿일 수 있다), "[offset:+200]"
    static func parseLRC(_ text: String) -> Lyrics {
        var lines: [LyricLine] = []
        var offset = 0.0
        let stamp = try! NSRegularExpression(pattern: #"\[(\d{1,3}):(\d{1,2})(?:[.:](\d{1,3}))?\]"#)
        var plain: [String] = []
        for raw in text.components(separatedBy: .newlines) {
            let line = raw.trimmingCharacters(in: .whitespaces)
            if line.lowercased().hasPrefix("[offset:") {
                let v = line.dropFirst(8).dropLast().trimmingCharacters(in: .whitespaces)
                offset = (Double(v) ?? 0) / 1000.0
                continue
            }
            let ns = line as NSString
            let matches = stamp.matches(in: line, range: NSRange(location: 0, length: ns.length))
            if matches.isEmpty {
                // [ar:가수] 같은 머리표는 빼고 나머지는 시간 없는 가사로.
                if !(line.hasPrefix("[") && line.hasSuffix("]")) { plain.append(line) }
                continue
            }
            let last = matches.last!.range
            let body = ns.substring(from: last.location + last.length).trimmingCharacters(in: .whitespaces)
            for m in matches {
                let mm = Double(ns.substring(with: m.range(at: 1))) ?? 0
                let ss = Double(ns.substring(with: m.range(at: 2))) ?? 0
                var frac = 0.0
                if m.range(at: 3).location != NSNotFound {
                    let f = ns.substring(with: m.range(at: 3))
                    frac = (Double(f) ?? 0) / pow(10, Double(f.count))
                }
                lines.append(LyricLine(time: max(0, mm * 60 + ss + frac - offset), text: body))
            }
        }
        lines.sort { $0.time < $1.time }
        return Lyrics(synced: lines, plain: lines.isEmpty ? plain.joined(separator: "\n") : "")
    }

    // MARK: - 표지 색

    /// 표지의 위·아래 평균 색 (배경 그라데이션용).
    static func palette(of image: UIImage) -> (UIColor, UIColor) {
        guard let cg = image.cgImage else { return (.darkGray, .black) }
        let ci = CIImage(cgImage: cg)
        let ctx = CIContext(options: [.workingColorSpace: NSNull()])
        func average(_ rect: CGRect) -> UIColor {
            guard let filter = CIFilter(name: "CIAreaAverage", parameters: [kCIInputImageKey: ci, kCIInputExtentKey: CIVector(cgRect: rect)]),
                  let out = filter.outputImage else { return .darkGray }
            var px = [UInt8](repeating: 0, count: 4)
            ctx.render(out, toBitmap: &px, rowBytes: 4, bounds: CGRect(x: 0, y: 0, width: 1, height: 1), format: .RGBA8, colorSpace: nil)
            return UIColor(red: CGFloat(px[0]) / 255, green: CGFloat(px[1]) / 255, blue: CGFloat(px[2]) / 255, alpha: 1)
        }
        let e = ci.extent
        let top = average(CGRect(x: e.minX, y: e.midY, width: e.width, height: e.height / 2))
        let bottom = average(CGRect(x: e.minX, y: e.minY, width: e.width, height: e.height / 2))
        return (top, bottom)
    }

    /// 배경으로 쓸 만큼 어둡게 (흰 글자가 읽히게).
    static func deepen(_ c: UIColor, _ factor: CGFloat) -> UIColor {
        var h: CGFloat = 0, s: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
        c.getHue(&h, saturation: &s, brightness: &b, alpha: &a)
        return UIColor(hue: h, saturation: min(1, s * 1.15), brightness: min(b, 0.55) * factor, alpha: 1)
    }

    // MARK: - 재생 목록

    /// 같은 폴더의 음악 파일 (이름순)
    static func folderPlaylist(for audioPath: String) -> [String] {
        let dir = (audioPath as NSString).deletingLastPathComponent
        guard let files = try? FileManager.default.contentsOfDirectory(atPath: dir) else { return [audioPath] }
        let list = files
            .filter { !$0.hasPrefix(".") && audioExtensions.contains(($0 as NSString).pathExtension.lowercased()) }
            .sorted { $0.localizedStandardCompare($1) == .orderedAscending }
            .map { (dir as NSString).appendingPathComponent($0) }
        return list.contains(audioPath) ? list : [audioPath] + list
    }
}

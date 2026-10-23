//
//  ArgoDiag.swift
//  Argo
//
//  파일로 남기는 진단 기록.
//
//  애플TV 는 콘솔을 붙이기 어렵고, 화면에 띄우면 재현하려는 증상을
//  가립니다. 그래서 파일에 적고 AirDrop 으로 꺼냅니다.
//
//  적는 곳: /var/mobile/Media/Argo/argo-diag.log
//  파일 탐색기에서 바로 보이는 자리입니다. 못 쓰면 앱 Documents 로 물러섭니다.
//

import UIKit

@available(tvOS 13.0, *)
enum ArgoDiag {

    /// 켜고 끕니다. 설정에서 바꿉니다 — 평소에 디스크를 긁을 이유가 없습니다.
    static var isEnabled: Bool {
        get { UserDefaults.settings.object(forKey: "diagEnabled") as? Bool ?? true }
        set { UserDefaults.settings.set(newValue, forKey: "diagEnabled") }
    }

    /// 2MB 를 넘으면 처음부터 다시 씁니다. 한 번 재현에 그만큼 쓸 일이 없고,
    /// 넘기기 시작하면 AirDrop 으로 꺼내기도 번거로워집니다.
    private static let sizeCap: UInt64 = 2 * 1024 * 1024

    /// 쓰기는 한 줄에 하나씩. 여러 곳에서 동시에 부르므로 줄이 섞이면 못 읽습니다.
    private static let queue = DispatchQueue(label: "argo.diag")

    private static let stamp: DateFormatter = {
        let f = DateFormatter()
        f.dateFormat = "HH:mm:ss.SSS"
        return f
    }()

    private(set) static var logPath: String = {
        let dir = "/var/mobile/Media/Argo"
        if !FileManager.default.fileExists(atPath: dir) {
            try? FileManager.default.createDirectory(atPath: dir,
                                                     withIntermediateDirectories: true)
        }
        if FileManager.default.isWritableFile(atPath: dir) {
            return dir + "/argo-diag.log"
        }
        let docs = NSSearchPathForDirectoriesInDomains(.documentDirectory, .userDomainMask, true)
        return (docs.first ?? NSTemporaryDirectory()) + "/argo-diag.log"
    }()

    /// 앱이 뜰 때 한 번. 어느 기기·어느 화면인지 먼저 박아 둡니다.
    static func begin(_ what: String) {
        guard isEnabled else { return }
        rotateIfBig()
        let s = UIScreen.main
        // 빌드 번호를 박아 둔다. 기기에 새 빌드가 실제로 올라갔는지
        // 로그만 보고 알 수 있어야 한다 — 한 번 옛 빌드의 로그를 놓고
        // 고친 코드가 안 먹는다고 헤맨 적이 있다.
        let build = (Bundle.main.infoDictionary?["CFBundleVersion"] as? String) ?? "?"
        let ver   = (Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String) ?? "?"
        write("""

        ========================================
        Argo \(ver) build \(build)
        \(what)  \(Date())
        screen bounds=\(s.bounds.size) native=\(s.nativeBounds.size) \
        scale=\(s.scale) nativeScale=\(s.nativeScale)
        ========================================
        """)
    }

    static func log(_ tag: String, _ message: @autoclosure () -> String) {
        guard isEnabled else { return }
        write("\(stamp.string(from: Date()))  [\(tag)] \(message())")
    }

    /// 기록을 비웁니다. 한 번 재현에 한 파일이어야 읽을 수 있습니다.
    static func clear() {
        queue.sync {
            try? FileManager.default.removeItem(atPath: logPath)
        }
    }

    static var fileSize: UInt64 {
        let a = try? FileManager.default.attributesOfItem(atPath: logPath)
        return (a?[.size] as? UInt64) ?? 0
    }

    // MARK: - 바닥

    private static func rotateIfBig() {
        if fileSize > sizeCap { clear() }
    }

    private static func write(_ line: String) {
        queue.async {
            guard let data = (line + "\n").data(using: .utf8) else { return }
            if let h = FileHandle(forWritingAtPath: logPath) {
                h.seekToEndOfFile()
                h.write(data)
                h.closeFile()
            } else {
                try? data.write(to: URL(fileURLWithPath: logPath))
            }
        }
    }
}

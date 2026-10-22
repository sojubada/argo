//
//  AirDropState.swift
//  Argo
//
//  AirDrop 의 SwiftUI 쪽 얼굴. 실제 일은 AirDropBridge 가 합니다.
//
//  저장된 토글 값을 믿지 않습니다. 모드는 디스크 설정이 아니라 실행 중인
//  sharingd 의 상태라, 재부팅하거나 앱이 뒤로 갔다 오면 꺼져 있을 수 있습니다.
//  그래서 늘 실제 상태를 읽어 보여 줍니다.
//

import SwiftUI
import Combine

@available(tvOS 13.0, *)
final class AirDropState: ObservableObject {

    /// 한 번 누르면 이만큼 열립니다. 받는 중이면 자동으로 연장됩니다.
    static let openSeconds: TimeInterval = 60

    @Published private(set) var isReceiving = false
    @Published private(set) var remaining: TimeInterval = 0
    @Published private(set) var isStarting = false
    /// 켰는데 모드가 올라오지 않은 경우. 조용히 실패하는 것보다 말해 주는 편이 낫습니다.
    @Published var lastFailure: String?

    var isAvailable: Bool { AirDropBridge.isAvailable }

    private var observer: NSObjectProtocol?

    init() {
        // 이름을 직접 적습니다. ObjC 의 NSNotificationName 상수가 Swift 로
        // 어떤 멤버 이름이 되는지 추측하면 빌드를 한 번 낭비하게 됩니다.
        observer = NotificationCenter.default.addObserver(
            forName: Notification.Name("ArgoAirDropStateChanged"),
            object: nil, queue: .main
        ) { [weak self] _ in
            self?.refresh()
        }
        refresh()
    }

    deinit {
        if let observer = observer {
            NotificationCenter.default.removeObserver(observer)
        }
    }

    private func refresh() {
        isReceiving = AirDropBridge.isReceiving
        remaining = AirDropBridge.remainingSeconds
    }

    /// 꺼져 있으면 켜고, 켜져 있으면 끕니다.
    func toggle() {
        guard isAvailable else { return }
        if isReceiving {
            AirDropBridge.disableReceiving()
            refresh()
            return
        }

        isStarting = true
        lastFailure = nil
        AirDropBridge.enableReceiving(seconds: Self.openSeconds) { [weak self] success, mode in
            guard let self = self else { return }
            self.isStarting = false
            self.refresh()
            if !success {
                // 모드가 2(모두)로 올라오지 않았습니다. 켜진 것처럼 보이게 두면
                // 아이폰에서 안 보이는 이유를 알 길이 없습니다.
                self.lastFailure = "AirDrop 을 켜지 못했습니다 (모드 \(mode)). 다시 시도해 보십시오."
            }
        }
    }

    /// 툴바에 보여 줄 짧은 글. 켜져 있을 때만 씁니다.
    var countdownLabel: String {
        guard isReceiving else { return "" }
        return "\(Int(remaining.rounded()))초"
    }
}

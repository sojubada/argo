//
//  VideoPlayerView.swift
//  Argo
//
//  동영상은 애플TV 기본 재생 화면(AVPlayerViewController)을 화면 가득 쓴다.
//
//  전에는 그 위에 SwiftUI 단추들을 따로 얹고, 화면을 다시 그릴 때마다
//  (재생 시간이 바뀌는 초당 10번) play() 를 불렀다. 그래서 멈춤을 눌러도
//  곧바로 다시 재생되었고, 리모컨 누름은 기본 화면과 단추가 서로 가져가
//  어느 쪽도 제대로 듣지 않았다.
//
//  이제 기본 화면 하나만 둔다. 리모컨: 재생·멈춤, 터치면 훑기, 좌우 클릭 ±10초,
//  아래로 쓸면 정보·자막 패널, Menu 로 닫기. 키보드: Space·K 재생·멈춤,
//  J/L ±10초, Esc 닫기.
//
//  부르는 쪽과의 약속(인자)은 예전 그대로다.
//

import SwiftUI
import AVKit
import AVFoundation

struct VideoPlayerView: View {
    @Binding var videoPath: String
    @Binding var videoName: String
    @Binding var isPresented: Bool
    @State var player: AVPlayer

    var body: some View {
        ArgoVideoController(url: url, player: player, onClose: { isPresented = false })
            .ignoresSafeArea()
            .onDisappear { player.pause() }
    }

    private var url: URL {
        let path = videoName.isEmpty ? videoPath : (videoPath as NSString).appendingPathComponent(videoName)
        return URL(fileURLWithPath: path)
    }
}

/// 기본 재생 화면을 띄운다. 재생은 처음 만들 때 한 번만 시작한다 —
/// update 에서 부르면 SwiftUI 가 다시 그릴 때마다 멈춤이 풀린다.
struct ArgoVideoController: UIViewControllerRepresentable {
    let url: URL
    let player: AVPlayer
    let onClose: () -> Void

    func makeUIViewController(context: Context) -> ArgoAVPlayerController {
        let vc = ArgoAVPlayerController()
        vc.onClose = onClose
        let item = AVPlayerItem(url: url)
        // 정보 패널에 보일 제목 (태그가 없으면 파일 이름)
        let title = AVMutableMetadataItem()
        title.identifier = .commonIdentifierTitle
        title.value = (url.lastPathComponent as NSString).deletingPathExtension as NSString
        title.extendedLanguageTag = "und"
        item.externalMetadata = [title]
        player.replaceCurrentItem(with: item)
        vc.player = player
        player.play()
        return vc
    }

    func updateUIViewController(_ vc: ArgoAVPlayerController, context: Context) {
        vc.onClose = onClose
    }
}

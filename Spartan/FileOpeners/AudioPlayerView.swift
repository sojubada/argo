//
//  AudioPlayerView.swift
//  Argo
//
//  음악 재생 화면 — 애플 뮤직 방식.
//
//  - 표지 색을 뽑아 배경을 칠하고, 흐린 표지를 깔아 곡마다 분위기가 바뀐다.
//  - 큰 표지: 멈추면 살짝 작아지고 재생하면 커진다.
//  - 가사: 시간이 붙은 가사는 지금 줄을 크게 띄우고 따라 올라간다.
//  - 같은 폴더가 곧 재생 목록: 이전·다음, 섞기, 반복(끔·전체·한 곡).
//  - 표지·가사는 파일·폴더에서 먼저 찾고, 없으면 온라인에서(설정으로 끔).
//
//  부르는 쪽(ContentView·WebDAVView)과의 약속은 예전 그대로다:
//  callback 이 참이면 audioPath/audioName 의 파일을 새로 틀고, 거짓이면
//  지금 틀고 있는 곡을 보여 준다.
//

import SwiftUI
import AVKit
import AVFoundation
import MediaPlayer

private enum RepeatMode: Int {
    case off, all, one
    var symbol: String {
        switch self {
        case .off: return "repeat"
        case .all: return "repeat"
        case .one: return "repeat.1"
        }
    }
}

/// 리모컨 포커스가 오면 떠오르는 둥근 버튼.
private struct GlassButtonStyle: ButtonStyle {
    var size: CGFloat = 86
    var prominent = false
    func makeBody(configuration: Configuration) -> some View {
        GlassLabel(configuration: configuration, size: size, prominent: prominent)
    }

    struct GlassLabel: View {
        let configuration: ButtonStyle.Configuration
        let size: CGFloat
        let prominent: Bool
        @Environment(\.isFocused) private var focused

        var body: some View {
            configuration.label
                .font(.system(size: size * 0.38, weight: .semibold))
                .foregroundColor(focused ? .black : .white)
                .frame(width: size, height: size)
                .background(
                    Circle().fill(focused ? Color.white : Color.white.opacity(prominent ? 0.22 : 0.0))
                )
                .scaleEffect(focused ? 1.12 : (configuration.isPressed ? 0.94 : 1.0))
                .shadow(color: .black.opacity(focused ? 0.35 : 0), radius: 14, y: 6)
                .animation(.easeOut(duration: 0.18), value: focused)
        }
    }
}

struct AudioPlayerView: View {
    @State var callback = false
    @Binding var audioPath: String
    @Binding var audioName: String
    @State var player: AVPlayer
    @Binding var isPresented: Bool

    @State private var playlist: [String] = []
    @State private var index = 0
    @State private var info = TrackInfo(title: "", artist: "", album: "", duration: 0)
    @State private var artwork: UIImage?
    @State private var colors: (Color, Color) = (Color(white: 0.18), Color.black)
    @State private var lyrics = Lyrics()
    @State private var lyricsSearching = false
    @State private var showLyrics = false
    @State private var isPlaying = false
    @State private var currentTime: Double = 0
    @State private var duration: Double = 0
    @State private var repeatMode: RepeatMode = .off
    @State private var shuffle = false
    @State private var online = MusicLookup.onlineEnabled
    @State private var timeObserver: Any?
    @State private var endObserver: NSObjectProtocol?
    @State private var loadToken = UUID()

    private var currentPath: String { playlist.indices.contains(index) ? playlist[index] : "" }

    var body: some View {
        ZStack {
            background
            if showLyrics {
                lyricsScreen
                    .transition(.opacity)
            } else {
                nowPlaying
                    .transition(.opacity)
            }
        }
        .animation(.easeInOut(duration: 0.35), value: showLyrics)
        .onPlayPauseCommand { playPause() }
        .onExitCommand {
            if showLyrics { showLyrics = false } else { isPresented = false }
        }
        .onAppear(perform: start)
        .onDisappear(perform: stopObserving)
        .onReceive(player.publisher(for: \.timeControlStatus)) { status in
            isPlaying = status == .playing
        }
    }

    // MARK: - 배경

    private var background: some View {
        ZStack {
            LinearGradient(colors: [colors.0, colors.1], startPoint: .topLeading, endPoint: .bottomTrailing)
            if let artwork {
                Image(uiImage: artwork)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
                    .blur(radius: 80)
                    .opacity(0.45)
                    .scaleEffect(1.3)
            }
            LinearGradient(colors: [.black.opacity(0.15), .black.opacity(0.55)], startPoint: .top, endPoint: .bottom)
        }
        .ignoresSafeArea()
        .animation(.easeInOut(duration: 0.8), value: artwork)
    }

    // MARK: - 지금 재생

    private var nowPlaying: some View {
        HStack(alignment: .center, spacing: 90) {
            artworkView
                .frame(width: 620, height: 620)
                .scaleEffect(isPlaying ? 1.0 : 0.86)
                .animation(.spring(response: 0.5, dampingFraction: 0.75), value: isPlaying)

            VStack(alignment: .leading, spacing: 26) {
                VStack(alignment: .leading, spacing: 10) {
                    Text(info.title.isEmpty ? (currentPath as NSString).lastPathComponent : info.title)
                        .font(.system(size: 54, weight: .bold))
                        .foregroundColor(.white)
                        .lineLimit(2)
                    if !info.artist.isEmpty || !info.album.isEmpty {
                        Text([info.artist, info.album].filter { !$0.isEmpty }.joined(separator: " — "))
                            .font(.system(size: 32, weight: .medium))
                            .foregroundColor(.white.opacity(0.72))
                            .lineLimit(1)
                    }
                }

                progress

                HStack(spacing: 34) {
                    Button(action: previous) { Image(systemName: "backward.fill") }
                        .buttonStyle(GlassButtonStyle())
                    Button(action: playPause) { Image(systemName: isPlaying ? "pause.fill" : "play.fill") }
                        .buttonStyle(GlassButtonStyle(size: 110, prominent: true))
                    Button(action: next) { Image(systemName: "forward.fill") }
                        .buttonStyle(GlassButtonStyle())
                }
                .padding(.vertical, 6)

                HStack(spacing: 22) {
                    Button(action: { seek(by: -15) }) { Image(systemName: "gobackward.15") }
                        .buttonStyle(GlassButtonStyle(size: 70))
                    Button(action: { seek(by: 15) }) { Image(systemName: "goforward.15") }
                        .buttonStyle(GlassButtonStyle(size: 70))
                    Button(action: { shuffle.toggle() }) {
                        Image(systemName: "shuffle").opacity(shuffle ? 1 : 0.45)
                    }
                    .buttonStyle(GlassButtonStyle(size: 70))
                    Button(action: cycleRepeat) {
                        Image(systemName: repeatMode.symbol).opacity(repeatMode == .off ? 0.45 : 1)
                    }
                    .buttonStyle(GlassButtonStyle(size: 70))
                    Button(action: { showLyrics = true }) {
                        Image(systemName: "quote.bubble").opacity(lyrics.isEmpty ? 0.45 : 1)
                    }
                    .buttonStyle(GlassButtonStyle(size: 70))
                    Button(action: toggleOnline) {
                        Image(systemName: online ? "network" : "network.slash")
                            .opacity(online ? 1 : 0.45)
                    }
                    .buttonStyle(GlassButtonStyle(size: 70))
                }

                if playlist.count > 1 {
                    upNext
                }
            }
            .frame(maxWidth: 860, alignment: .leading)
        }
        .padding(.horizontal, 110)
    }

    private var artworkView: some View {
        ZStack {
            if let artwork {
                Image(uiImage: artwork)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
            } else {
                LinearGradient(colors: [.white.opacity(0.18), .white.opacity(0.06)], startPoint: .top, endPoint: .bottom)
                Image(systemName: "music.note")
                    .font(.system(size: 180, weight: .light))
                    .foregroundColor(.white.opacity(0.55))
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: 28, style: .continuous))
        .shadow(color: .black.opacity(0.45), radius: 40, y: 22)
    }

    private var progress: some View {
        VStack(spacing: 10) {
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(Color.white.opacity(0.22))
                    Capsule().fill(Color.white.opacity(0.9))
                        .frame(width: duration > 0 ? geo.size.width * CGFloat(min(1, currentTime / duration)) : 0)
                }
            }
            .frame(height: 8)
            HStack {
                Text(currentTime.format())
                Spacer()
                Text("-" + max(0, duration - currentTime).format())
            }
            .font(.system(size: 24, weight: .medium).monospacedDigit())
            .foregroundColor(.white.opacity(0.65))
        }
    }

    private var upNext: some View {
        let upcoming = (1...3).compactMap { k -> String? in
            let i = index + k
            if i < playlist.count { return playlist[i] }
            if repeatMode == .all && !playlist.isEmpty { return playlist[i % playlist.count] }
            return nil
        }
        return VStack(alignment: .leading, spacing: 6) {
            if !upcoming.isEmpty {
                Text(L("다음 곡"))
                    .font(.system(size: 22, weight: .semibold))
                    .foregroundColor(.white.opacity(0.5))
                ForEach(upcoming, id: \.self) { path in
                    Text(((path as NSString).lastPathComponent as NSString).deletingPathExtension)
                        .font(.system(size: 24))
                        .foregroundColor(.white.opacity(0.7))
                        .lineLimit(1)
                }
            }
        }
        .padding(.top, 8)
    }

    // MARK: - 가사

    private var currentLine: Int? {
        guard !lyrics.synced.isEmpty else { return nil }
        let t = currentTime + 0.25
        return lyrics.synced.lastIndex(where: { $0.time <= t })
    }

    private var lyricsScreen: some View {
        HStack(alignment: .center, spacing: 70) {
            VStack(alignment: .leading, spacing: 18) {
                artworkView.frame(width: 360, height: 360)
                Text(info.title)
                    .font(.system(size: 36, weight: .bold))
                    .foregroundColor(.white)
                    .lineLimit(2)
                Text(info.artist)
                    .font(.system(size: 26))
                    .foregroundColor(.white.opacity(0.7))
                    .lineLimit(1)
                Button(action: { showLyrics = false }) {
                    Image(systemName: "chevron.backward")
                }
                .buttonStyle(GlassButtonStyle(size: 70))
                .padding(.top, 10)
            }
            .frame(width: 400)

            Group {
                if !lyrics.synced.isEmpty {
                    syncedLyrics
                } else if !lyrics.plain.isEmpty {
                    ScrollView {
                        Text(lyrics.plain)
                            .font(.system(size: 40, weight: .semibold))
                            .foregroundColor(.white.opacity(0.9))
                            .lineSpacing(14)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .focusable(true)
                    }
                } else {
                    VStack(spacing: 20) {
                        Image(systemName: "text.quote").font(.system(size: 80)).foregroundColor(.white.opacity(0.4))
                        Text(lyricsSearching ? L("가사를 찾는 중…") : L("가사가 없습니다"))
                            .font(.system(size: 36, weight: .semibold))
                            .foregroundColor(.white.opacity(0.6))
                        Text(L("같은 이름의 .lrc 파일을 음악 옆에 두면 그것을 씁니다."))
                            .font(.system(size: 24))
                            .foregroundColor(.white.opacity(0.45))
                    }
                    .frame(maxWidth: .infinity)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .padding(.horizontal, 110)
        .padding(.vertical, 60)
    }

    private var syncedLyrics: some View {
        ScrollViewReader { proxy in
            ScrollView(showsIndicators: false) {
                VStack(alignment: .leading, spacing: 30) {
                    Color.clear.frame(height: 300)
                    ForEach(Array(lyrics.synced.enumerated()), id: \.offset) { i, line in
                        let active = i == currentLine
                        let distance = abs(i - (currentLine ?? 0))
                        Text(line.text.isEmpty ? "♪" : line.text)
                            .font(.system(size: active ? 54 : 44, weight: .bold))
                            .foregroundColor(.white.opacity(active ? 1.0 : max(0.18, 0.5 - Double(distance) * 0.07)))
                            .blur(radius: active ? 0 : min(3, Double(distance) * 0.6))
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .id(i)
                            .animation(.easeOut(duration: 0.3), value: currentLine)
                    }
                    Color.clear.frame(height: 400)
                }
            }
            .onChange(of: currentLine) { line in
                guard let line else { return }
                withAnimation(.easeInOut(duration: 0.5)) {
                    proxy.scrollTo(line, anchor: UnitPoint(x: 0, y: 0.38))
                }
            }
        }
    }

    // MARK: - 재생

    private func start() {
        if callback {
            let path = (audioPath as NSString).appendingPathComponent(audioName)
            playlist = MusicLookup.folderPlaylist(for: path)
            index = playlist.firstIndex(of: path) ?? 0
            load(play: true)
        } else if let url = (player.currentItem?.asset as? AVURLAsset)?.url, url.isFileURL {
            // 틀고 있던 곡을 다시 보여 준다.
            playlist = MusicLookup.folderPlaylist(for: url.path)
            index = playlist.firstIndex(of: url.path) ?? 0
            describeCurrent()
            observe()
        } else if !audioPath.isEmpty || !audioName.isEmpty {
            let path = audioName.isEmpty ? audioPath : (audioPath as NSString).appendingPathComponent(audioName)
            playlist = MusicLookup.folderPlaylist(for: path)
            index = playlist.firstIndex(of: path) ?? 0
            load(play: true)
        }
    }

    private func load(play: Bool) {
        guard !currentPath.isEmpty else { return }
        player.replaceCurrentItem(with: AVPlayerItem(url: URL(fileURLWithPath: currentPath)))
        currentTime = 0
        if play { player.play() }
        describeCurrent()
        observe()
    }

    /// 지금 곡의 태그·표지·가사를 모은다.
    private func describeCurrent() {
        let path = currentPath
        let token = UUID()
        loadToken = token
        let asset = player.currentItem?.asset ?? AVURLAsset(url: URL(fileURLWithPath: path))
        let fileName = (path as NSString).lastPathComponent
        info = MusicLookup.trackInfo(asset: asset, fileName: fileName)
        duration = info.duration
        lyrics = Lyrics()
        let local = MusicLookup.embeddedArtwork(asset: asset) ?? MusicLookup.folderArtwork(audioPath: path)
        setArtwork(local)
        if let l = MusicLookup.localLyrics(audioPath: path, asset: asset) { lyrics = l }
        updateNowPlayingInfo()

        let snapshot = info
        let needArt = local == nil
        let needLyrics = lyrics.isEmpty
        if needArt || needLyrics {
            lyricsSearching = needLyrics
            Task {
                async let art: UIImage? = needArt ? MusicLookup.onlineArtwork(snapshot) : nil
                async let lyr: Lyrics? = needLyrics ? MusicLookup.onlineLyrics(snapshot) : nil
                let (foundArt, foundLyrics) = await (art, lyr)
                await MainActor.run {
                    guard loadToken == token else { return }
                    if let foundArt { setArtwork(foundArt) }
                    if let foundLyrics { lyrics = foundLyrics }
                    lyricsSearching = false
                    updateNowPlayingInfo()
                }
            }
        }
    }

    private func setArtwork(_ image: UIImage?) {
        artwork = image
        if let image {
            let (a, b) = MusicLookup.palette(of: image)
            colors = (Color(MusicLookup.deepen(a, 1.0)), Color(MusicLookup.deepen(b, 0.55)))
        } else {
            colors = (Color(white: 0.2), Color(white: 0.04))
        }
    }

    private func observe() {
        stopObserving()
        timeObserver = player.addPeriodicTimeObserver(forInterval: CMTime(value: 1, timescale: 5), queue: .main) { time in
            let t = time.seconds
            if t.isFinite { currentTime = t }
            if let d = player.currentItem?.duration.seconds, d.isFinite, d > 0 { duration = d }
        }
        endObserver = NotificationCenter.default.addObserver(forName: .AVPlayerItemDidPlayToEndTime, object: player.currentItem, queue: .main) { _ in
            trackEnded()
        }
    }

    private func stopObserving() {
        if let o = timeObserver {
            player.removeTimeObserver(o)
            timeObserver = nil
        }
        if let e = endObserver {
            NotificationCenter.default.removeObserver(e)
            endObserver = nil
        }
    }

    private func trackEnded() {
        switch repeatMode {
        case .one:
            player.seek(to: .zero)
            player.play()
        case .all:
            next()
        case .off:
            if shuffle || index + 1 < playlist.count {
                next()
            } else {
                player.seek(to: .zero)
                player.pause()
            }
        }
    }

    private func playPause() {
        if isPlaying { player.pause() } else { player.play() }
        updateNowPlayingInfo()
    }

    private func next() {
        guard !playlist.isEmpty else { return }
        if shuffle && playlist.count > 1 {
            var k = index
            while k == index { k = Int.random(in: 0..<playlist.count) }
            index = k
        } else {
            index = (index + 1) % playlist.count
        }
        load(play: true)
    }

    private func previous() {
        // 3초 넘게 들었으면 처음으로, 아니면 앞 곡으로 (애플 방식).
        if currentTime > 3 || playlist.count < 2 {
            player.seek(to: .zero)
            return
        }
        index = (index - 1 + playlist.count) % playlist.count
        load(play: true)
    }

    private func seek(by seconds: Double) {
        let t = max(0, min(duration, currentTime + seconds))
        player.seek(to: CMTime(seconds: t, preferredTimescale: 600))
    }

    private func cycleRepeat() {
        repeatMode = RepeatMode(rawValue: (repeatMode.rawValue + 1) % 3) ?? .off
    }

    private func toggleOnline() {
        online.toggle()
        MusicLookup.onlineEnabled = online
        if online && (artwork == nil || lyrics.isEmpty) { describeCurrent() }
    }

    /// TV 의 '지금 재생 중' 정보 (제어 센터 등)
    private func updateNowPlayingInfo() {
        var d: [String: Any] = [
            MPMediaItemPropertyTitle: info.title,
            MPMediaItemPropertyArtist: info.artist,
            MPMediaItemPropertyAlbumTitle: info.album,
            MPMediaItemPropertyPlaybackDuration: duration,
            MPNowPlayingInfoPropertyElapsedPlaybackTime: currentTime,
            MPNowPlayingInfoPropertyPlaybackRate: isPlaying ? 1.0 : 0.0,
        ]
        if let artwork {
            d[MPMediaItemPropertyArtwork] = MPMediaItemArtwork(boundsSize: artwork.size) { _ in artwork }
        }
        MPNowPlayingInfoCenter.default().nowPlayingInfo = d
    }
}

extension Double {
    func format() -> String {
        guard isFinite else { return "00:00" }
        let total = Int(self)
        let h = total / 3600
        let m = (total % 3600) / 60
        let s = total % 60
        return h > 0 ? String(format: "%d:%02d:%02d", h, m, s) : String(format: "%02d:%02d", m, s)
    }
}

extension AVPlayer {
    var isPlaying: Bool {
        return self.rate != 0 && error == nil
    }
}

//
//  MusicVisuals.swift
//  Argo
//
//  음악 화면의 두 가지 볼거리.
//
//  1. 스펙트럼 막대 — 화면 아래에 얇게 깔리는 색 막대.
//     재생기(AVPlayerItem)에 MTAudioProcessingTap 을 걸어 실제 나가는 소리를
//     받아, 1024점 FFT 로 40개 띠(40Hz~16kHz, 로그 간격)의 세기를 낸다.
//     오디오 스레드에서 계산하고 화면에는 초당 30번만 넘긴다.
//     탭을 걸 수 없는 경우(트랙을 못 읽음 등)에는 막대가 바닥에 머문다 — 재생에는 영향이 없다.
//
//  2. 슬라이드쇼 — 음악 폴더(와 그 아래 한 단계 폴더)의 사진, 사진이 없으면 표지를
//     8초마다 천천히 확대하며(켄 번즈) 겹쳐 넘긴다. 사진은 화면 크기로 줄여 읽는다.
//

import SwiftUI
import AVFoundation
import MediaToolbox
import Accelerate
import ImageIO

// MARK: - 스펙트럼 분석

final class AudioSpectrum: ObservableObject {
    static let bandCount = 40

    @Published private(set) var bands: [Float] = Array(repeating: 0, count: AudioSpectrum.bandCount)

    private let fftSize = 1024
    private let log2n: vDSP_Length = 10
    private let fft: FFTSetup?
    private var window: [Float]
    private var ring: [Float]
    private var ringFill = 0
    private var smooth: [Float] = Array(repeating: 0, count: AudioSpectrum.bandCount)
    private var lastPublish: CFAbsoluteTime = 0
    fileprivate var sampleRate: Double = 44_100
    fileprivate var isFloat = true
    /// 정수 표본일 때의 비트 수(16·32). 0 이면 읽지 못하는 형식.
    fileprivate var intBits: UInt32 = 0
    /// 탭이 실제로 불린 횟수. 걸어 놓고도 끝내 불리지 않는 mp3 가 있어 센다.
    fileprivate var processCalls = 0
    fileprivate var loggedFirstCall = false
    private var retried = Set<ObjectIdentifier>()
    private let lock = NSLock()

    init() {
        fft = vDSP_create_fftsetup(10, FFTRadix(kFFTRadix2))
        window = [Float](repeating: 0, count: 1024)
        vDSP_hann_window(&window, vDSP_Length(1024), Int32(vDSP_HANN_NORM))
        ring = [Float](repeating: 0, count: 1024)
    }

    deinit {
        if let fft { vDSP_destroy_fftsetup(fft) }
    }

    /// 재생 항목에 탭을 건다. 이미 소리 섞기(audioMix)가 있으면 건드리지 않는다.
    func attach(to item: AVPlayerItem) {
        guard item.audioMix == nil else { return }
        let asset = item.asset
        lock.lock(); processCalls = 0; loggedFirstCall = false; lock.unlock()
        // 내 기기의 파일이면 트랙을 바로 읽어, 재생이 시작되기 전에 탭을 건다.
        // 재생이 시작된 뒤에 소리 섞기를 넣으면 몇몇 mp3 에서 탭이 끝내 불리지
        // 않았다(막대가 안 움직임).
        if let url = (asset as? AVURLAsset)?.url, url.isFileURL,
           let track = asset.tracks(withMediaType: .audio).first,
           let tap = makeTap() {
            let params = AVMutableAudioMixInputParameters(track: track)
            params.audioTapProcessor = tap
            let mix = AVMutableAudioMix()
            mix.inputParameters = [params]
            item.audioMix = mix
            watch(item, name: url.lastPathComponent)
            return
        }
        asset.loadValuesAsynchronously(forKeys: ["tracks"]) { [weak self, weak item] in
            guard let self, let item else { return }
            var error: NSError?
            guard asset.statusOfValue(forKey: "tracks", error: &error) == .loaded,
                  let track = asset.tracks(withMediaType: .audio).first else { return }
            DispatchQueue.main.async {
                guard item.audioMix == nil, let tap = self.makeTap() else { return }
                let params = AVMutableAudioMixInputParameters(track: track)
                params.audioTapProcessor = tap
                let mix = AVMutableAudioMix()
                mix.inputParameters = [params]
                item.audioMix = mix
                self.watch(item, name: (asset as? AVURLAsset)?.url.lastPathComponent ?? "?")
            }
        }
    }

    /// 몇몇 mp3(앞에 큰 ID3 태그·표지가 붙은 것 등)는 준비가 끝나기 전에 건
    /// 소리 섞기를 무시해 탭이 한 번도 불리지 않았다. 재생이 시작되고 잠시
    /// 뒤에도 불린 적이 없으면 새 탭으로 한 번 다시 건다. 결과는 진단 로그에 남긴다.
    private func watch(_ item: AVPlayerItem, name: String) {
        DispatchQueue.main.asyncAfter(deadline: .now() + 3.0) { [weak self, weak item] in
            guard let self, let item else { return }
            self.lock.lock(); let calls = self.processCalls; self.lock.unlock()
            if calls > 0 { return }
            guard item.status == .readyToPlay else {
                // 아직 준비 중이면 조금 더 기다린다.
                if item.status == .unknown { self.watch(item, name: name) }
                return
            }
            // 멈춰 있으면 탭이 안 불리는 게 정상이다. 재생될 때까지 기다린다.
            let rate = item.timebase.map { CMTimebaseGetRate($0) } ?? 0
            if rate == 0 { self.watch(item, name: name); return }
            let key = ObjectIdentifier(item)
            guard !self.retried.contains(key) else {
                ArgoDiag.log("audio", "스펙트럼 탭이 다시 걸어도 불리지 않음 — \(name)")
                return
            }
            self.retried.insert(key)
            guard let track = item.asset.tracks(withMediaType: .audio).first,
                  let tap = self.makeTap() else {
                ArgoDiag.log("audio", "스펙트럼 탭 재시도 실패(트랙/탭 없음) — \(name)")
                return
            }
            ArgoDiag.log("audio", "스펙트럼 탭이 3초간 불리지 않아 다시 검 — \(name)")
            let params = AVMutableAudioMixInputParameters(track: track)
            params.audioTapProcessor = tap
            let mix = AVMutableAudioMix()
            mix.inputParameters = [params]
            item.audioMix = nil
            item.audioMix = mix
            self.watch(item, name: name)
        }
    }

    /// 멈추면 막대를 내린다.
    func reset() {
        lock.lock()
        smooth = Array(repeating: 0, count: Self.bandCount)
        ringFill = 0
        lock.unlock()
        DispatchQueue.main.async { self.bands = Array(repeating: 0, count: Self.bandCount) }
    }

    private func makeTap() -> MTAudioProcessingTap? {
        var callbacks = MTAudioProcessingTapCallbacks(
            version: kMTAudioProcessingTapCallbacksVersion_0,
            clientInfo: UnsafeMutableRawPointer(Unmanaged.passRetained(self).toOpaque()),
            init: { _, clientInfo, storageOut in
                storageOut.pointee = clientInfo
            },
            finalize: { tap in
                let storage = MTAudioProcessingTapGetStorage(tap)
                Unmanaged<AudioSpectrum>.fromOpaque(storage).release()
            },
            prepare: { tap, _, format in
                let me = Unmanaged<AudioSpectrum>.fromOpaque(MTAudioProcessingTapGetStorage(tap)).takeUnretainedValue()
                let f = format.pointee
                me.sampleRate = f.mSampleRate > 0 ? f.mSampleRate : 44_100
                me.isFloat = f.mFormatFlags & kAudioFormatFlagIsFloat != 0 && f.mBitsPerChannel == 32
                // 정수 표본도 받는다. 전에는 float 이 아니면 그냥 버려 막대가 서지 않았다.
                me.intBits = (!me.isFloat && f.mFormatFlags & kAudioFormatFlagIsSignedInteger != 0
                              && (f.mBitsPerChannel == 16 || f.mBitsPerChannel == 32)) ? f.mBitsPerChannel : 0
                ArgoDiag.log("audio", "스펙트럼 탭 준비 — \(Int(f.mSampleRate))Hz \(f.mChannelsPerFrame)ch \(f.mBitsPerChannel)bit flags=0x\(String(f.mFormatFlags, radix: 16))")
            },
            unprepare: { _ in },
            process: { tap, frames, _, bufferList, framesOut, flagsOut in
                let status = MTAudioProcessingTapGetSourceAudio(tap, frames, bufferList, flagsOut, nil, framesOut)
                let me = Unmanaged<AudioSpectrum>.fromOpaque(MTAudioProcessingTapGetStorage(tap)).takeUnretainedValue()
                me.noteCall(status: status)
                guard status == noErr else { return }
                let buffers = UnsafeMutableAudioBufferListPointer(bufferList)
                guard let first = buffers.first, let data = first.mData else { return }
                let channels = max(1, Int(first.mNumberChannels))
                let frameCount = Int(framesOut.pointee)
                if me.isFloat {
                    let count = Int(first.mDataByteSize) / MemoryLayout<Float>.size / channels
                    me.feed(data.assumingMemoryBound(to: Float.self), count: min(count, frameCount), stride: channels)
                } else if me.intBits == 16 {
                    let count = min(Int(first.mDataByteSize) / 2 / channels, frameCount)
                    me.feedInt16(data.assumingMemoryBound(to: Int16.self), count: count, stride: channels)
                } else if me.intBits == 32 {
                    let count = min(Int(first.mDataByteSize) / 4 / channels, frameCount)
                    me.feedInt32(data.assumingMemoryBound(to: Int32.self), count: count, stride: channels)
                }
            })
        var out: Unmanaged<MTAudioProcessingTap>?
        let err = MTAudioProcessingTapCreate(kCFAllocatorDefault, &callbacks,
                                             kMTAudioProcessingTapCreationFlag_PostEffects, &out)
        guard err == noErr, let made = out else {
            // init 이 불리지 않았으니 맡긴 참조를 돌려받는다.
            Unmanaged<AudioSpectrum>.fromOpaque(callbacks.clientInfo!).release()
            return nil
        }
        return made.takeRetainedValue()
    }

    /// 오디오 스레드. 처음 불릴 때와 오류만 로그에 남긴다(매번 남기면 로그가 넘친다).
    fileprivate func noteCall(status: OSStatus) {
        lock.lock()
        processCalls += 1
        let first = !loggedFirstCall
        loggedFirstCall = true
        lock.unlock()
        if first {
            ArgoDiag.log("audio", status == noErr ? "스펙트럼 탭 동작 시작" : "스펙트럼 탭 원본 읽기 실패 \(status)")
        }
    }

    fileprivate func feedInt16(_ samples: UnsafePointer<Int16>, count: Int, stride: Int) {
        guard count > 0 else { return }
        var tmp = [Float](repeating: 0, count: count)
        for i in 0..<count { tmp[i] = Float(samples[i * stride]) / 32_768 }
        tmp.withUnsafeBufferPointer { feed($0.baseAddress!, count: count, stride: 1) }
    }

    fileprivate func feedInt32(_ samples: UnsafePointer<Int32>, count: Int, stride: Int) {
        guard count > 0 else { return }
        var tmp = [Float](repeating: 0, count: count)
        for i in 0..<count { tmp[i] = Float(samples[i * stride]) / 2_147_483_648 }
        tmp.withUnsafeBufferPointer { feed($0.baseAddress!, count: count, stride: 1) }
    }

    /// 오디오 스레드. 1024개가 모이면 FFT 한 번.
    fileprivate func feed(_ samples: UnsafePointer<Float>, count: Int, stride: Int) {
        guard count > 0 else { return }
        lock.lock()
        defer { lock.unlock() }
        var i = 0
        while i < count {
            ring[ringFill] = samples[i * stride]
            ringFill += 1
            i += 1
            if ringFill == fftSize {
                ringFill = 0
                analyse()
            }
        }
    }

    private func analyse() {
        guard let fft else { return }
        let half = fftSize / 2
        var windowed = [Float](repeating: 0, count: fftSize)
        vDSP_vmul(ring, 1, window, 1, &windowed, 1, vDSP_Length(fftSize))
        var real = [Float](repeating: 0, count: half)
        var imag = [Float](repeating: 0, count: half)
        var mags = [Float](repeating: 0, count: half)
        real.withUnsafeMutableBufferPointer { rp in
            imag.withUnsafeMutableBufferPointer { ip in
                var split = DSPSplitComplex(realp: rp.baseAddress!, imagp: ip.baseAddress!)
                windowed.withUnsafeBufferPointer { wp in
                    wp.baseAddress!.withMemoryRebound(to: DSPComplex.self, capacity: half) {
                        vDSP_ctoz($0, 2, &split, 1, vDSP_Length(half))
                    }
                }
                vDSP_fft_zrip(fft, &split, 1, log2n, FFTDirection(FFT_FORWARD))
                vDSP_zvabs(&split, 1, &mags, 1, vDSP_Length(half))
            }
        }

        // 로그 간격 띠. 띠마다 가장 센 칸을 쓴다.
        let nyquist = sampleRate / 2
        let lo = 40.0, hi = min(16_000.0, nyquist * 0.95)
        var out = [Float](repeating: 0, count: Self.bandCount)
        for b in 0..<Self.bandCount {
            let f0 = lo * pow(hi / lo, Double(b) / Double(Self.bandCount))
            let f1 = lo * pow(hi / lo, Double(b + 1) / Double(Self.bandCount))
            var k0 = Int(f0 / nyquist * Double(half))
            var k1 = Int(f1 / nyquist * Double(half))
            k0 = max(1, min(half - 1, k0))
            k1 = max(k0 + 1, min(half, k1))
            var peak: Float = 0
            for k in k0..<k1 { peak = max(peak, mags[k]) }
            // 세기 → dB → 0~1. 높은 띠는 원래 약하므로 조금 올려 준다.
            let db = 20 * log10(max(peak / Float(fftSize), 1e-7)) + Float(b) * 0.35
            out[b] = max(0, min(1, (db + 62) / 52))
        }
        for b in 0..<Self.bandCount {
            // 빨리 오르고 천천히 내린다.
            smooth[b] = out[b] > smooth[b] ? out[b] * 0.7 + smooth[b] * 0.3 : smooth[b] * 0.82 + out[b] * 0.18
        }
        let now = CFAbsoluteTimeGetCurrent()
        guard now - lastPublish > 1.0 / 30 else { return }
        lastPublish = now
        let snapshot = smooth
        DispatchQueue.main.async { self.bands = snapshot }
    }
}

/// 화면 아래의 얇은 색 막대.
struct SpectrumBar: View {
    @ObservedObject var spectrum: AudioSpectrum
    var tint: Color

    var body: some View {
        Canvas { ctx, size in
            let n = AudioSpectrum.bandCount
            let gap: CGFloat = 6
            let w = (size.width - gap * CGFloat(n - 1)) / CGFloat(n)
            var path = Path()
            for (i, v) in spectrum.bands.enumerated() {
                let h = max(3, size.height * CGFloat(v))
                let rect = CGRect(x: CGFloat(i) * (w + gap), y: size.height - h, width: w, height: h)
                path.addRoundedRect(in: rect, cornerSize: CGSize(width: min(4, w / 2), height: min(4, w / 2)))
            }
            ctx.fill(path, with: .linearGradient(
                Gradient(colors: [tint, Color(red: 1.0, green: 0.36, blue: 0.55), Color(red: 1.0, green: 0.72, blue: 0.25),
                                  Color(red: 0.35, green: 0.9, blue: 0.62), Color(red: 0.3, green: 0.7, blue: 1.0), tint]),
                startPoint: .zero, endPoint: CGPoint(x: size.width, y: 0)))
        }
        .opacity(0.85)
        .animation(.linear(duration: 1.0 / 30), value: spectrum.bands)
        .allowsHitTesting(false)
    }
}

// MARK: - 슬라이드쇼

enum SlideSource {
    private static let imageExt: Set<String> = ["jpg", "jpeg", "png", "heic", "webp", "gif", "bmp", "tif", "tiff"]

    /// 음악 폴더와 그 아래 한 단계 폴더의 사진 (이름순, 최대 300장).
    static func photos(near audioPath: String) -> [String] {
        let dir = (audioPath as NSString).deletingLastPathComponent
        let fm = FileManager.default
        var out: [String] = []
        func scan(_ d: String, depth: Int) {
            guard let names = try? fm.contentsOfDirectory(atPath: d) else { return }
            for name in names.sorted(by: { $0.localizedStandardCompare($1) == .orderedAscending }) where !name.hasPrefix(".") {
                let p = (d as NSString).appendingPathComponent(name)
                var isDir: ObjCBool = false
                guard fm.fileExists(atPath: p, isDirectory: &isDir) else { continue }
                if isDir.boolValue {
                    if depth == 0 { scan(p, depth: 1) }
                } else if imageExt.contains((name as NSString).pathExtension.lowercased()) {
                    out.append(p)
                }
                if out.count >= 300 { return }
            }
        }
        scan(dir, depth: 0)
        return out
    }

    /// 화면 크기에 맞춰 줄여 읽는다 (큰 사진을 통째로 올리지 않는다).
    static func load(_ path: String, maxPixel: Int = 1920) -> UIImage? {
        guard let src = CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, nil) else { return nil }
        let opts: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: maxPixel,
        ]
        guard let cg = CGImageSourceCreateThumbnailAtIndex(src, 0, opts as CFDictionary) else { return nil }
        return UIImage(cgImage: cg)
    }
}

/// 사진을 천천히 확대하며 겹쳐 넘긴다.
struct SlideshowView: View {
    let photos: [String]
    let fallback: UIImage?
    var interval: Double = 8

    @State private var index = 0
    @State private var current: UIImage?
    @State private var zoomed = false
    @State private var timer: Timer?

    var body: some View {
        GeometryReader { geo in
            ZStack {
                Color.black
                if let img = current ?? fallback {
                    Image(uiImage: img)
                        .resizable()
                        .aspectRatio(contentMode: isPortrait(img) ? .fit : .fill)
                        .frame(width: geo.size.width, height: geo.size.height)
                        .scaleEffect(zoomed ? 1.12 : 1.0)
                        .clipped()
                        .id(index)
                        .transition(.opacity)
                }
            }
        }
        .ignoresSafeArea()
        .onAppear {
            UIApplication.shared.isIdleTimerDisabled = true
            show(0)
            timer = Timer.scheduledTimer(withTimeInterval: interval, repeats: true) { _ in
                guard photos.count > 1 else { return }
                show((index + 1) % photos.count)
            }
        }
        .onDisappear {
            UIApplication.shared.isIdleTimerDisabled = false
            timer?.invalidate()
            timer = nil
        }
    }

    private func isPortrait(_ img: UIImage) -> Bool { img.size.height > img.size.width * 1.1 }

    private func show(_ i: Int) {
        guard !photos.isEmpty else {
            zoomed = false
            withAnimation(.linear(duration: interval)) { zoomed = true }
            return
        }
        let path = photos[i]
        DispatchQueue.global(qos: .userInitiated).async {
            let img = SlideSource.load(path)
            DispatchQueue.main.async {
                zoomed = false
                withAnimation(.easeInOut(duration: 1.5)) {
                    index = i
                    current = img
                }
                withAnimation(.linear(duration: interval + 1.5)) { zoomed = true }
            }
        }
    }
}

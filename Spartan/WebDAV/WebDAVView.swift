//
//  WebDAVView.swift
//  Argo
//
//  Remote browsing over WebDAV: connection list, connection editor and the
//  remote file browser (download, view, install IPAs through TrollStore).
//

import SwiftUI
import AVKit

// MARK: - Root

struct WebDAVRootView: View {

    @Binding var directory: String
    @Binding var isPresented: Bool
    /// 받은 문서를 로컬 파일과 같은 뷰어(PDF·오피스·엑셀·글)로 연다. 맡았으면 true.
    var openLocal: ((String) -> Bool)? = nil

    @ObservedObject private var store = WebDAVStore.shared

    /// 0 = connection list, 1 = connection editor, 2 = remote browser
    @State private var screen = 0
    @State private var workingServer = WebDAVServer()
    @State private var isNewServer = false
    /// When on, selecting a connection opens its settings instead of browsing it.
    @State private var editMode = false

    var body: some View {
        // 바탕을 먼저 깐다.
        //
        // tvOS 의 fullScreenCover 는 내용이 바탕을 칠하지 않으면 투명하게
        // 뜬다. 목록과 툴바가 그대로 비쳐 두 화면이 겹쳐 보였고, 비친
        // 단추가 포커스를 받는 것처럼 보여 어디를 누르는지 알 수 없었다.
        ZStack {
            Theme.background
                .edgesIgnoringSafeArea(.all)
            content
        }
    }

    @ViewBuilder
    private var content: some View {
        Group {
            // 단축키는 탐색 화면(2)에서만. 연결 목록·설정 화면에서는 로컬처럼 키를 흘려보낸다.
            Color.clear.frame(width: 0, height: 0)
                .onAppear { ArgoRemoteKeys.active = (screen == 2) }
                .onChange(of: screen) { s in ArgoRemoteKeys.active = (s == 2) }
            if screen == 2 {
                WebDAVBrowserView(server: workingServer,
                                  directory: $directory,
                                  isPresented: $isPresented,
                                  openLocal: openLocal,
                                  onBack: { screen = 0 })
            } else if screen == 1 {
                WebDAVServerEditView(server: $workingServer,
                                     isNew: isNewServer,
                                     onSave: {
                                         store.upsert(workingServer)
                                         screen = 0
                                         editMode = false
                                     },
                                     onDelete: {
                                         store.remove(workingServer)
                                         screen = 0
                                         editMode = false
                                     },
                                     onCancel: {
                                         screen = 0
                                         editMode = false
                                     })
            } else {
                connectionList
            }
        }
    }

    private var connectionList: some View {
        VStack {
            Text(L("원격 연결"))
                .font(.title)

            if store.servers.isEmpty {
                Text(L("저장한 연결이 없습니다. WebDAV, FTP 를 더할 수 있습니다."))
                    .foregroundColor(.secondary)
                    .padding(.top, 20)
            } else {
                // One button per row: tvOS focuses a whole row at once, so a row
                // holding two buttons would always fire the trailing one.
                List(store.servers) { server in
                    Button(action: {
                        workingServer = server
                        if editMode {
                            isNewServer = false
                            screen = 1
                        } else {
                            screen = 2
                        }
                    }) {
                        HStack {
                            Image(systemName: editMode ? "gear" : server.kind.symbol)
                            VStack(alignment: .leading) {
                                Text(server.displayName)
                                Text(server.kind.label + " · " + server.urlString)
                                    .font(.caption)
                                    .foregroundColor(.secondary)
                            }
                        }
                    }
                }
            }

            Text(editMode
                 ? L("고칠 연결을 고르십시오.")
                 : L("둘러볼 연결을 고르십시오."))
                .font(.caption)
                .foregroundColor(.secondary)

            HStack {
                Button(action: {
                    workingServer = WebDAVServer()
                    isNewServer = true
                    screen = 1
                }) {
                    HStack {
                        Image(systemName: "plus")
                        Text(L("연결 더하기"))
                    }
                }

                if !store.servers.isEmpty {
                    Button(action: { editMode.toggle() }) {
                        HStack {
                            Image(systemName: "gear")
                            Text(editMode ? L("완료") : L("연결 고치기"))
                        }
                    }
                }

                Button(action: {
                    directory = WebDAVCache.directory + "/"
                    isPresented = false
                }) {
                    HStack {
                        Image(systemName: "folder")
                        Text(L("받은 폴더 열기"))
                    }
                }
            }

            Text(LF("임시로 받는 곳: {0} — {1} 사용 중", WebDAVCache.directory, ByteCountFormatter.string(fromByteCount: WebDAVCache.usedBytes, countStyle: .file)))
                .font(.caption)
                .foregroundColor(.secondary)
            Text(IPAInstaller.routeDescription)
                .font(.caption)
                .foregroundColor(.secondary)
        }
    }
}

// MARK: - Connection editor

struct WebDAVServerEditView: View {

    @Binding var server: WebDAVServer
    var isNew: Bool
    var onSave: () -> Void
    var onDelete: () -> Void
    var onCancel: () -> Void

    @State private var status = ""
    @State private var testing = false

    var body: some View {
        VStack(alignment: .leading) {
            Text(isNew ? L("새 연결") : L("연결 고치기"))
                .font(.title)

            // 종류. 한 줄에 버튼 셋 — 포커스가 한 칸씩 옮겨 간다.
            HStack {
                ForEach(RemoteKind.allCases, id: \.self) { k in
                    Button(action: { server.kind = k; status = "" }) {
                        HStack {
                            Image(systemName: server.kind == k ? "largecircle.fill.circle" : "circle")
                            Text(k.label)
                        }
                    }
                }
            }

            ArgoTextField(L("이름 (선택)"), text: $server.name)
            ArgoTextField(urlPlaceholder, text: $server.urlString)
            ArgoTextField(L("사용자 이름"), text: $server.username)
            ArgoTextField(L("비밀번호"), text: $server.password, secure: true)
            if server.kind == .webdav {
                Toggle(L("검증되지 않은 인증서 허용 (집 NAS)"), isOn: $server.allowInsecureTLS)
            }

            Text(kindHint)
                .font(.caption)
                .foregroundColor(.secondary)

            if !status.isEmpty {
                Text(status)
                    .font(.caption)
                    .foregroundColor(.secondary)
            }

            HStack {
                Button(action: test) {
                    Text(testing ? L("확인 중…") : L("연결 확인"))
                }
                Button(action: {
                    if server.isValid {
                        onSave()
                    } else {
                        status = invalidMessage
                    }
                }) {
                    Text(L("저장"))
                }
                Button(action: onCancel) {
                    Text(L("취소"))
                }
                if !isNew {
                    Button(action: onDelete) {
                        Text(L("삭제"))
                            .foregroundColor(.red)
                    }
                }
            }
        }
        .padding()
    }

    private var urlPlaceholder: String {
        switch server.kind {
        case .webdav: return "https://host:port/path"
        case .ftp:    return "ftp://host:21/path"
        }
    }

    private var kindHint: String {
        switch server.kind {
        case .webdav: return L("Nextcloud·시놀로지 같은 클라우드와 NAS 는 대부분 WebDAV 로 붙습니다.")
        case .ftp:    return L("일반 FTP 입니다. 암호화되지 않으니 집 안 네트워크에서 쓰십시오. 사용자 이름을 비우면 익명으로 접속합니다.")
        }
    }

    private var invalidMessage: String {
        switch server.kind {
        case .webdav: return L("http:// 나 https:// 로 시작하는 주소를 넣으십시오.")
        case .ftp:    return L("ftp:// 로 시작하는 주소를 넣으십시오.")
        }
    }

    private func test() {
        guard server.isValid else {
            status = invalidMessage
            return
        }
        testing = true
        status = ""
        let client = RemoteClients.make(server)
        client.testConnection { result in
            testing = false
            switch result {
            case .success:
                status = L("연결되었습니다.")
            case .failure(let error):
                status = error.localizedDescription
            }
            _ = client // keep the client alive until the callback runs
        }
    }
}

// MARK: - Remote browser

struct WebDAVBrowserView: View {

    let server: WebDAVServer
    @Binding var directory: String
    @Binding var isPresented: Bool
    var openLocal: ((String) -> Bool)?
    var onBack: () -> Void

    @State private var client: RemoteFileClient
    @State private var path: String
    @State private var items: [WebDAVItem] = []
    /// 한 번에 그리는 행 수. 수천 항목을 한꺼번에 그리면 멈추므로 끝에 닿을 때마다 늘린다.
    @State private var visibleCount = 150
    @State private var loading = false
    @State private var message = ""

    /// Where "copy" puts things: whatever folder the local browser is sitting on.
    @State private var destination: String

    @State private var detailItem: WebDAVItem?
    @State private var multiSelect = false
    @State private var picked: Set<String> = []
    /// 키보드 고르기(Space)·정보(⌘I)에 쓰는 지금 포커스된 줄.
    @FocusState private var focusedPath: String?
    /// 마지막으로 연 줄(사진·영상·폴더). 닫고 돌아오거나 목록을 다시 읽어도 커서를 여기로 돌려 놓는다.
    @State private var lastFocus: String?
    /// 처음 한 번만 목록을 읽는다. 사진·영상 화면을 닫고 돌아올 때마다 onAppear 가 다시 불려
    /// 폴더를 통째로 다시 읽고, 그 바람에 커서가 첫 파일로 갔다.
    @State private var didInitialLoad = false
    /// 목록 읽기 번호. 폴더를 빠르게 옮기거나 새로고침을 겹쳐 눌렀을 때, 늦게 온 옛 폴더 결과가
    /// 지금 폴더 목록을 덮어쓰지 않게 한다(큰 폴더는 응답이 수 초 늦다).
    @State private var reloadToken = 0
    /// ⌘C·⌘X 로 담아 둔 것(서버 안 복사·이동). 폴더를 옮겨 다녀도 남는다.
    @State private var clip: RemoteClip?
    @State private var showKeyHelp = false

    @State private var transferring = false
    @State private var progress: Double = 0
    @State private var transferLabel = ""

    // Viewers. SwiftUI on tvOS only honours one .sheet per view, so a single
    // sheet switches on viewerKind instead of stacking five modifiers.
    @State private var localPath = ""
    @State private var localName = ""
    @State private var showViewer = false
    @State private var viewerKind = 0   // 0 hex, 1 audio, 2 video, 3 image, 4 text
    @State private var isSVG = false
    @State private var player = AVPlayer()

    init(server: WebDAVServer,
         directory: Binding<String>,
         isPresented: Binding<Bool>,
         openLocal: ((String) -> Bool)? = nil,
         onBack: @escaping () -> Void) {
        self.server = server
        self._directory = directory
        self._isPresented = isPresented
        self.openLocal = openLocal
        self.onBack = onBack
        let c = RemoteClients.make(server)
        self._client = State(initialValue: c)
        self._path = State(initialValue: c.startPath)
        self._destination = State(initialValue: directory.wrappedValue)
    }

    private var atRoot: Bool {
        WebDAVClient.normalize(path) == WebDAVClient.normalize(client.startPath)
    }

    var body: some View {
        VStack {
            header

            if transferring {
                Text("\(transferLabel)  \(Int(progress * 100))%")
                    .font(.headline)
            } else if loading {
                Text("Loading...")
                    .foregroundColor(.secondary)
            }

            if !message.isEmpty {
                Text(message)
                    .font(.caption)
                    .foregroundColor(.secondary)
            }

            if detailItem != nil {
                detailPanel
            } else {
                listPanel
            }
        }
        .onAppear {
            ArgoRemoteKeys.active = true
            if !didInitialLoad {
                didInitialLoad = true
                reload()
            } else if let f = lastFocus, items.contains(where: { $0.path == f }) {
                // 돌아왔을 뿐이다 — 다시 읽지 않고 커서만 제자리로.
                DispatchQueue.main.async { focusedPath = f }
            }
        }
        // The remote's Menu button walks back up the tree, the way it does in
        // the local browser, instead of closing the whole sheet.
        .onExitCommand { goBack() }
        .onReceive(NotificationCenter.default.publisher(for: ArgoRemoteKeys.notification)) { note in
            if let raw = note.object as? String, let cmd = ArgoRemoteCommand(rawValue: raw) {
                handleKey(cmd)
            }
        }
        .sheet(isPresented: $showViewer) {
            viewerContent
        }
        .alert(isPresented: $showKeyHelp) {
            Alert(title: Text(L("WebDAV 단축키")),
                  message: Text(ArgoRemoteKeys.helpText),
                  dismissButton: .default(Text(L("닫기"))))
        }
    }

    private var viewerContent: some View {
        Group {
            if viewerKind == 4 {
                TextView(filePath: localPath, fileName: localName, isPresented: $showViewer)
            } else if viewerKind == 3 {
                ImageView(imagePath: localPath, imageName: localName, isSVG: isSVG)
            } else if viewerKind == 2 {
                VideoPlayerView(videoPath: $localPath, videoName: $localName,
                                isPresented: $showViewer, player: player)
            } else if viewerKind == 1 {
                AudioPlayerView(callback: false, audioPath: $localPath, audioName: $localName,
                                player: player, isPresented: $showViewer)
            } else {
                HexView(filePath: localPath, fileName: localName, isPresented: $showViewer)
            }
        }
    }

    // MARK: Header

    private var header: some View {
        VStack {
            Text(path)
                .font(.headline)
            Text("\(server.displayName)   copy to \(destination)")
                .font(.caption)
                .foregroundColor(.secondary)
            // 지금 몇 번째인지 / 전체, 고른 수.
            Text(positionText)
                .font(.caption)
                .monospacedDigit()
                .foregroundColor(.secondary)

            HStack {
                Button(action: { goBack() }) {
                    Image(systemName: "chevron.left")
                        .frame(width: 50, height: 50)
                }

                Button(action: { reload() }) {
                    Image(systemName: "arrow.clockwise")
                        .frame(width: 50, height: 50)
                }

                Button(action: {
                    if multiSelect {
                        multiSelect = false
                        picked.removeAll()
                    } else {
                        detailItem = nil
                        multiSelect = true
                    }
                }) {
                    Image(systemName: multiSelect ? "checkmark.circle.fill" : "checkmark.circle")
                        .frame(width: 50, height: 50)
                }

                if multiSelect {
                    Button(action: {
                        if picked.count == items.count {
                            picked.removeAll()
                        } else {
                            picked = Set(items.map { $0.path })
                        }
                    }) {
                        Image(systemName: "square.stack")
                            .frame(width: 50, height: 50)
                    }
                }

                Button(action: { downloadTargets(keyTargets()) }) {
                    ZStack {
                        Image(systemName: "doc.on.doc")
                            .frame(width: 50, height: 50)
                        Image(systemName: "arrow.down")
                            .resizable()
                            .frame(width: 15, height: 13)
                            .offset(x: -4, y: 11.75)
                    }
                }
                .disabled(transferring || (picked.isEmpty && focusedPath == nil))

                Button(action: {
                    directory = destination.hasSuffix("/") ? destination : destination + "/"
                    isPresented = false
                }) {
                    Image(systemName: "folder")
                        .frame(width: 50, height: 50)
                }

                Button(action: { onBack() }) {
                    Image(systemName: "globe")
                        .frame(width: 50, height: 50)
                }

                Button(action: { isPresented = false }) {
                    Image(systemName: "xmark")
                        .frame(width: 50, height: 50)
                }
            }
        }
    }

    private var positionText: String {
        var s = "\(items.count)"
        if let f = focusedPath, let i = items.firstIndex(where: { $0.path == f }) {
            s = "\(i + 1) / \(items.count)"
        }
        if !picked.isEmpty { s += "   ✓ \(picked.count)" }
        return s
    }

    // MARK: Listing

    private var listPanel: some View {
        List {
            ForEach(items.prefix(visibleCount)) { item in
            Button(action: { open(item) }) {
                HStack {
                    if multiSelect {
                        Image(systemName: picked.contains(item.path) ? "checkmark.circle.fill" : "circle")
                    }
                    Image(systemName: icon(for: item))
                    VStack(alignment: .leading) {
                        Text(item.name)
                        if !item.isDirectory {
                            Text("\(item.sizeDescription)   \(item.modifiedDescription)")
                                .font(.caption)
                                .foregroundColor(.secondary)
                        }
                    }
                }
            }
            .focused($focusedPath, equals: item.path)
            // 선택 버튼 = 바로 열기. 복사·설치·정보는 길게 눌러 고른다.
            .contextMenu {
                if !item.isDirectory {
                    Button(action: { openRemote(item) }) {
                        Label(L("열기"), systemImage: "eye")
                    }
                    Button(action: { copyOne(item) }) {
                        Label(L("미디어 폴더로 복사"), systemImage: "doc.on.doc")
                    }
                    Button(action: { detailItem = item }) {
                        Label(L("정보·설치"), systemImage: "info.circle")
                    }
                }
            }
            }
            if items.count > visibleCount {
                Text("\(visibleCount) / \(items.count) 항목 — 계속 내리면 더 불러옵니다")
                    .font(.caption)
                    .foregroundColor(.secondary)
                    .onAppear { visibleCount = min(items.count, visibleCount + 200) }
            }
        }
    }

    private func icon(for item: WebDAVItem) -> String {
        if item.isDirectory { return "folder" }
        if item.isIPA { return "square.and.arrow.down" }
        switch ArgoRemoteKind.of(item.name) {
        case .video: return "film"
        case .audio: return "music.note"
        case .image: return "photo"
        case .unsupportedMedia: return "film.fill"
        case .other: return "doc"
        }
    }

    // MARK: Detail

    private var detailPanel: some View {
        VStack(alignment: .leading) {
            if let item = detailItem {
                Text(item.name)
                    .font(.headline)
                Text("\(item.sizeDescription)   \(item.modifiedDescription)")
                    .font(.caption)
                    .foregroundColor(.secondary)

                if item.isIPA {
                    Button(action: { downloadThenInstall(item) }) {
                        HStack {
                            Image(systemName: "square.and.arrow.down")
                            Text("Install with TrollStore")
                        }
                    }
                    .disabled(transferring)
                }

                Button(action: { openRemote(item) }) {
                    HStack {
                        Image(systemName: "eye")
                        Text(L("열기"))
                    }
                }
                .disabled(transferring)

                Button(action: { copyOne(item) }) {
                    HStack {
                        Image(systemName: "doc.on.doc")
                        Text("Copy to \(destination)")
                    }
                }
                .disabled(transferring)

                Button(action: { detailItem = nil }) {
                    Text("Back")
                }
            }
        }
        .padding()
    }

    // MARK: Navigation

    private func goBack() {
        if detailItem != nil {
            detailItem = nil
        } else if multiSelect {
            multiSelect = false
            picked.removeAll()
        } else if !atRoot {
            lastFocus = path   // 위로 가면 방금 나온 폴더에 커서
            path = WebDAVClient.parent(of: path)
            picked.removeAll()
            reload()
        } else {
            ArgoRemoteKeys.active = false
            onBack()
        }
    }

    private func open(_ item: WebDAVItem) {
        lastFocus = item.path
        if item.isDirectory {
            path = item.path
            picked.removeAll()
            reload()
        } else if multiSelect {
            if picked.contains(item.path) {
                picked.remove(item.path)
            } else {
                picked.insert(item.path)
            }
        } else if item.isIPA {
            // IPA 는 열 것이 없다 — 설치 화면으로.
            detailItem = item
        } else {
            openRemote(item)
        }
    }


    // MARK: Keyboard (RemoteKeys.swift 의 표)

    private func handleKey(_ cmd: ArgoRemoteCommand) {
        if transferring, cmd != .help {
            message = L("전송이 끝날 때까지 기다려 주세요.")
            return
        }
        switch cmd {
        case .goBackOrUp:
            goBack()
        case .goRoot:
            detailItem = nil
            picked.removeAll()
            path = client.startPath
            reload()
        case .goUp:
            if !atRoot {
                detailItem = nil
                picked.removeAll()
                path = WebDAVClient.parent(of: path)
                reload()
            }
        case .refresh:
            reload()
        case .toggleSelect:
            guard let f = focusedPath, items.contains(where: { $0.path == f }) else { return }
            detailItem = nil
            multiSelect = true
            if picked.contains(f) { picked.remove(f) } else { picked.insert(f) }
        case .toggleMultiSelect:
            if multiSelect {
                multiSelect = false
                picked.removeAll()
            } else {
                detailItem = nil
                multiSelect = true
            }
        case .selectAll:
            detailItem = nil
            multiSelect = true
            if picked.count == items.count { picked.removeAll() } else { picked = Set(items.map { $0.path }) }
        case .copy, .cut:
            let t = keyTargets()
            guard !t.isEmpty else {
                message = L("먼저 고르세요 (Space 또는 ⌘A).")
                return
            }
            clip = RemoteClip(items: t, cut: cmd == .cut, serverID: server.id)
            message = "\(t.count)" + (cmd == .cut ? L("개 잘라냄 — 옮길 폴더에서 ⌘V") : L("개 복사함 — 넣을 폴더에서 ⌘V"))
        case .paste:
            paste()
        case .download:
            downloadTargets(keyTargets())
        case .compress:
            compressTargets(keyTargets())
        case .info:
            if let f = focusedPath, let it = items.first(where: { $0.path == f }), !it.isDirectory {
                detailItem = it
            }
        case .help:
            showKeyHelp = true
        }
    }

    /// 고른 것이 있으면 고른 것, 없으면 지금 포커스된 줄.
    private func keyTargets() -> [WebDAVItem] {
        if !picked.isEmpty { return items.filter { picked.contains($0.path) } }
        if let f = focusedPath, let it = items.first(where: { $0.path == f }) { return [it] }
        return []
    }

    // MARK: Server-side copy / move (⌘C·⌘X → ⌘V)

    private func paste() {
        guard let c = clip else {
            message = L("먼저 ⌘C(복사) 또는 ⌘X(잘라내기)로 담으세요.")
            return
        }
        guard c.serverID == server.id, let dav = client as? WebDAVClient else {
            message = L("서버 안 복사·이동은 같은 WebDAV 서버 안에서만 됩니다. 애플TV로 받으려면 ⌘D.")
            return
        }
        let here = WebDAVClient.normalize(path)
        var taken = Set(items.map { $0.name })
        var jobs: [(WebDAVItem, String)] = []
        for it in c.items {
            let src = WebDAVClient.normalize(it.path)
            if it.isDirectory, here == src || here.hasPrefix(src + "/") {
                message = L("폴더를 자기 안으로 넣을 수 없습니다") + ": " + it.name
                return
            }
            var name = it.name
            if WebDAVClient.normalize(WebDAVClient.parent(of: it.path)) == here {
                if c.cut { continue }                 // 같은 자리로 이동 — 할 일 없음
                name = copyName(it.name, taken: taken) // 같은 폴더에 복사 — "이름 사본"
            }
            taken.insert(name)
            jobs.append((it, WebDAVClient.child(path, name)))
        }
        guard !jobs.isEmpty else { message = L("이미 이 폴더에 있습니다."); return }
        pasteStep(dav, jobs, 0, cut: c.cut)
    }

    private func pasteStep(_ dav: WebDAVClient, _ jobs: [(WebDAVItem, String)], _ i: Int, cut: Bool) {
        if i >= jobs.count {
            transferring = false
            if cut { clip = nil }
            picked.removeAll()
            multiSelect = false
            reload()
            message = "\(jobs.count)" + (cut ? L("개를 옮겼습니다.") : L("개를 복사했습니다."))
            return
        }
        let (it, to) = jobs[i]
        transferring = true
        progress = Double(i) / Double(jobs.count)
        transferLabel = (cut ? L("옮기는 중") : L("복사하는 중")) + " \(it.name) (\(i + 1)/\(jobs.count))"
        dav.transfer(copy: !cut, from: it.path, to: to) { result in
            switch result {
            case .success:
                pasteStep(dav, jobs, i + 1, cut: cut)
            case .failure(let e):
                transferring = false
                if case .http(412)? = e as? WebDAVError {
                    message = L("같은 이름이 이미 있어 멈췄습니다") + ": " + it.name
                } else {
                    message = e.localizedDescription
                }
                reload()
            }
        }
    }

    private func copyName(_ name: String, taken: Set<String>) -> String {
        let ext = (name as NSString).pathExtension
        let base = ext.isEmpty ? name : (name as NSString).deletingPathExtension
        for n in 1..<1000 {
            let suffix = n == 1 ? " " + L("사본") : " " + L("사본") + " \(n)"
            let c = ext.isEmpty ? base + suffix : base + suffix + "." + ext
            if !taken.contains(c) { return c }
        }
        return name + " " + UUID().uuidString.prefix(4)
    }

    // MARK: Download / compress (폴더는 안까지 통째로)

    /// 이름을 로컬 경로 한 칸으로 쓸 수 있게(서버가 준 이름이 '..' 이나 '/' 를 품어도 받는 곳 밖으로 못 나가게).
    private func safeName(_ n: String) -> String {
        let s = n.replacingOccurrences(of: "/", with: "_")
        return (s.isEmpty || s == "." || s == "..") ? "_" : s
    }

    /// 고른 것을 펼친다: 파일 목록(상대 폴더 포함)과 만들 폴더 목록. 너무 크면 nil.
    private func collect(_ roots: [WebDAVItem],
                         done: @escaping (_ files: [(WebDAVItem, String)], _ dirs: [String]) -> Void,
                         fail: @escaping () -> Void) {
        var files: [(WebDAVItem, String)] = []
        var made: [String] = []
        var queue: [(WebDAVItem, String, Int)] = []
        for r in roots {
            if r.isDirectory { queue.append((r, safeName(r.name), 0)) } else { files.append((r, "")) }
        }
        func next() {
            if files.count > 5000 || made.count > 1000 {
                message = L("너무 많습니다(파일 5,000개·폴더 1,000개까지). 나눠서 받아 주세요.")
                fail(); return
            }
            guard !queue.isEmpty else { done(files, made); return }
            let (dir, rel, depth) = queue.removeFirst()
            made.append(rel)
            transferLabel = L("목록 읽는 중") + " " + rel
            client.list(path: dir.path) { result in
                switch result {
                case .success(let kids):
                    for k in kids {
                        let r2 = rel + "/" + safeName(k.name)
                        if k.isDirectory {
                            if depth < 30 { queue.append((k, r2, depth + 1)) }
                        } else {
                            files.append((k, rel))
                        }
                    }
                    next()
                case .failure(let e):
                    message = e.localizedDescription
                    fail()
                }
            }
        }
        next()
    }

    private func fetchAll(_ files: [(WebDAVItem, String)], _ dirs: [String], into base: String,
                          label: String, done: @escaping (Bool) -> Void) {
        let fm = FileManager.default
        for d in dirs {
            try? fm.createDirectory(atPath: (base as NSString).appendingPathComponent(d),
                                    withIntermediateDirectories: true)
        }
        func step(_ i: Int) {
            if i >= files.count { done(true); return }
            let (it, rel) = files[i]
            let folder = rel.isEmpty ? base : (base as NSString).appendingPathComponent(rel)
            let target = (folder as NSString).appendingPathComponent(safeName(it.name))
            transferLabel = "\(label) \(it.name)  (\(i + 1)/\(files.count))"
            progress = 0
            client.fetch(item: it, to: target, progress: { progress = $0 }, completion: { result in
                switch result {
                case .success:
                    step(i + 1)
                case .failure(let e):
                    message = e.localizedDescription
                    done(false)
                }
            })
        }
        step(0)
    }

    private func downloadTargets(_ t: [WebDAVItem]) {
        guard !t.isEmpty else {
            message = L("받을 것을 먼저 고르세요 (Space 또는 ⌘A).")
            return
        }
        transferring = true
        progress = 0
        message = ""
        let base = destination
        collect(t, done: { files, dirs in
            fetchAll(files, dirs, into: base, label: L("받는 중")) { ok in
                transferring = false
                if ok {
                    message = "\(files.count)" + L("개를 받았습니다") + " → " + base
                    picked.removeAll()
                    multiSelect = false
                }
            }
        }, fail: { transferring = false })
    }

    private func compressTargets(_ t: [WebDAVItem]) {
        guard !t.isEmpty else {
            message = L("압축할 것을 먼저 고르세요 (Space 또는 ⌘A).")
            return
        }
        transferring = true
        progress = 0
        message = ""
        let stage = (ArgoRemoteCache.root as NSString).appendingPathComponent("zip-" + UUID().uuidString)
        try? FileManager.default.createDirectory(atPath: stage, withIntermediateDirectories: true)
        // zip 이름: 하나면 그 이름, 여럿이면 지금 폴더 이름.
        let first = t[0].isDirectory ? t[0].name : (t[0].name as NSString).deletingPathExtension
        let folderName = (WebDAVClient.normalize(path) as NSString).lastPathComponent
        var zipBase = safeName(t.count == 1 ? first : (folderName == "/" || folderName.isEmpty ? server.displayName : folderName))
        if zipBase == "_" { zipBase = "archive" }
        let dest = freshLocal((destination as NSString).appendingPathComponent(zipBase + ".zip"))
        collect(t, done: { files, dirs in
            fetchAll(files, dirs, into: stage, label: L("압축할 것 받는 중")) { ok in
                guard ok else {
                    transferring = false
                    try? FileManager.default.removeItem(atPath: stage)
                    return
                }
                transferLabel = L("압축하는 중") + " " + (dest as NSString).lastPathComponent
                progress = 1
                let names = (try? FileManager.default.contentsOfDirectory(atPath: stage)) ?? []
                DispatchQueue.global(qos: .userInitiated).async {
                    var failure: Error?
                    do {
                        try Compression.compressFiles(listOfFiles: names, directory: stage + "/",
                                                      destination: dest, compType: .zip)
                    } catch {
                        failure = error
                    }
                    try? FileManager.default.removeItem(atPath: stage)
                    DispatchQueue.main.async {
                        transferring = false
                        if let failure = failure {
                            message = L("압축하지 못했습니다") + ": " + failure.localizedDescription
                        } else {
                            message = L("압축했습니다") + " → " + dest
                            picked.removeAll()
                            multiSelect = false
                        }
                    }
                }
            }
        }, fail: {
            transferring = false
            try? FileManager.default.removeItem(atPath: stage)
        })
    }

    /// 같은 이름이 있으면 "이름 2.zip" 처럼 비켜 쓴다.
    private func freshLocal(_ p: String) -> String {
        let fm = FileManager.default
        guard fm.fileExists(atPath: p) else { return p }
        let ns = p as NSString
        let ext = ns.pathExtension
        let base = (ns.lastPathComponent as NSString).deletingPathExtension
        for n in 2..<1000 {
            let c = ns.deletingLastPathComponent + "/" + base + " \(n)" + (ext.isEmpty ? "" : "." + ext)
            if !fm.fileExists(atPath: c) { return c }
        }
        return p
    }

    // MARK: Open like a local file

    /// 로컬 파일을 누를 때와 같은 결과가 나오게 연다.
    /// 음악·동영상은 받지 않고 바로 재생, 사진은 앨범(넘길 때 그 장만 받음),
    /// 문서는 임시 보관함에 받아 로컬과 같은 뷰어로.
    private func openRemote(_ item: WebDAVItem) {
        lastFocus = item.path
        guard !transferring else { return }
        detailItem = nil
        switch ArgoRemoteKind.of(item.name) {
        case .unsupportedMedia:
            message = L("애플TV 재생기가 열 수 없는 형식입니다") + " (." + (item.name as NSString).pathExtension + ")"
        case .video, .audio:
            if server.kind == .webdav, let remote = client.directURL(for: item) {
                message = L("재생 준비 중…")
                ArgoRemoteStream.shared.streamURL(remote: remote, server: server, name: item.name) { url in
                    message = ""
                    guard let url = url else {
                        message = L("바로 재생을 준비하지 못했습니다. 받아서 엽니다.")
                        downloadThenView(item)
                        return
                    }
                    ArgoDiag.log("stream", "바로 재생 \(item.name)")
                    ArgoMediaPresenter.presentVideo(url: url, player: player,
                                                    playlist: videoPlaylist(startingAt: item)) {}
                }
            } else {
                // FTP 는 구간 읽기를 중계하지 않는다 — 받아서 연다.
                downloadThenView(item)
            }
        case .image:
            openRemoteAlbum(item)
        case .other:
            downloadThenView(item)
        }
    }

    /// 같은 폴더의 같은 종류(영상끼리·음악끼리) 파일을 목록 순서대로 이어 재생하기 위한 목록.
    private func videoPlaylist(startingAt item: WebDAVItem) -> ArgoVideoPlaylist? {
        let kind = ArgoRemoteKind.of(item.name)
        let list = items.filter { !$0.isDirectory && ArgoRemoteKind.of($0.name) == kind }
        guard list.count > 1, let start = list.firstIndex(where: { $0.path == item.path }) else { return nil }
        let c = client
        let srv = server
        return ArgoVideoPlaylist(
            count: list.count, index: start,
            name: { list[$0].name },
            url: { i, done in
                let it = list[i]
                guard let remote = c.directURL(for: it) else { done(nil); return }
                // 앞 영상의 중계가 뒤에서 계속 받지 않게 끊고 새로 연다.
                ArgoRemoteStream.shared.cancelAll()
                ArgoRemoteStream.shared.streamURL(remote: remote, server: srv, name: it.name, completion: done)
            })
    }

    /// 이 원격 폴더의 사진들을 앨범으로. 처음엔 이름만 넘기고, 사진은 보는 장과 이웃 장만 받는다.
    private func openRemoteAlbum(_ item: WebDAVItem) {
        let safe: (String) -> String = { $0.replacingOccurrences(of: "/", with: "_") }
        var byName: [String: WebDAVItem] = [:]
        for it in items where !it.isDirectory && ArgoRemoteKind.of(it.name) == .image {
            if byName[safe(it.name)] == nil { byName[safe(it.name)] = it }
        }
        if byName[safe(item.name)] == nil { byName[safe(item.name)] = item }
        let c = client
        ArgoRemoteStream.shared.cancelAll()
        ArgoDiag.log("photo", "앨범 열기 \(byName.count)장 · \(ArgoDiag.memoryText())")
        let vc = ArgoPhotoAlbumController()
        vc.folder = ArgoRemoteCache.folder(server: server, remoteFolder: WebDAVClient.parent(of: item.path))
        vc.startName = safe(item.name)
        vc.remoteNames = Array(byName.keys)
        vc.remoteFetch = { name in
            guard let it = byName[name] else { return nil }
            return ArgoRemoteCache.fetchSync(client: c, item: it)
        }
        vc.onClose = { [weak vc] in vc?.dismiss(animated: true) }
        guard let top = AirDropBridge.topViewController() else { return }
        vc.modalPresentationStyle = .fullScreen
        top.present(vc, animated: true)
    }

    private func reload() {
        loading = true
        message = ""
        reloadToken += 1
        let token = reloadToken
        client.list(path: path) { result in
            guard token == reloadToken else { return }   // 더 새 읽기가 있다 — 이 결과는 버린다
            loading = false
            switch result {
            case .success(let listed):
                visibleCount = 150
                items = listed
                if listed.isEmpty { message = "This folder is empty." }
                else if listed.count > 1000 { message = "항목 \(listed.count)개 — 나눠서 보여 줍니다" }
                // 목록을 다시 읽어도(새로고침·폴더 이동 후 돌아옴) 마지막에 연 줄로 커서를 되돌린다.
                if let f = lastFocus, let idx = listed.firstIndex(where: { $0.path == f }) {
                    // 아래쪽 줄이면 그 줄까지 그려 둔다. 다만 수만 줄을 한꺼번에 그리면 멈추거나 꺼지므로
                    // 2000줄 안쪽일 때만(그보다 멀면 처음 줄에 둔다).
                    if idx + 30 <= 2000 {
                        visibleCount = max(visibleCount, idx + 30)
                        DispatchQueue.main.async { focusedPath = f }
                    }
                }
            case .failure(let error):
                items = []
                message = error.localizedDescription
            }
        }
    }

    // MARK: Transfers

    private func localTarget(for item: WebDAVItem) -> String {
        return (destination as NSString).appendingPathComponent(item.name)
    }

    private func download(_ item: WebDAVItem, to target: String,
                          label: String, completion: @escaping (String?) -> Void) {
        transferring = true
        progress = 0
        transferLabel = label
        message = ""
        client.fetch(item: item, to: target, progress: { fraction in
            progress = fraction
        }, completion: { result in
            switch result {
            case .success(let saved):
                completion(saved)
            case .failure(let error):
                transferring = false
                message = error.localizedDescription
                completion(nil)
            }
        })
    }

    private func copyOne(_ item: WebDAVItem) {
        download(item, to: localTarget(for: item), label: "Copying \(item.name)") { saved in
            transferring = false
            if let saved = saved { message = "Copied to \(saved)" }
        }
    }

    private func downloadThenView(_ item: WebDAVItem) {
        // 임시 보관함에 받는다. 같은 파일을 이미 받아 두었으면 바로 연다.
        let target = ArgoRemoteCache.localPath(server: server, item: item)
        if ArgoRemoteCache.isFresh(target, item: item) {
            ArgoRemoteCache.touch(target)
            localPath = target
            localName = item.name
            present(target)
            return
        }
        ArgoRemoteCache.prepare(target)
        download(item, to: target, label: L("여는 중") + " \(item.name)") { local in
            transferring = false
            guard let local = local else { return }
            localPath = local
            localName = item.name
            present(local)
        }
    }

    private func present(_ local: String) {
        // 로컬 파일과 같은 뷰어(PDF·오피스·엑셀·글)가 맡으면 그쪽으로.
        if let openLocal = openLocal, openLocal(local) { return }
        let type = FileInfo.yandereDevFileType(file: local)
        isSVG = false
        if type == 1 || type == 2 {
            // 음악·동영상은 시트가 아니라 화면 가득 띄운다 (MediaKeys.swift).
            // 시트 안에서는 리모컨 누름이 이 화면의 onExitCommand 로 새어 나갔다.
            // local 은 이미 파일 전체 경로다 — 이름을 또 붙이면 없는 경로가 된다.
            let file = local
            if type == 2 {
                ArgoMediaPresenter.presentVideo(url: URL(fileURLWithPath: file), player: player) {}
            } else {
                let p = player
                let view = AudioPlayerView(callback: true, audioPath: .constant(file), audioName: .constant(""),
                                           player: p, isPresented: .constant(true))
                ArgoMediaPresenter.presentAudio(view) { p.pause() }
            }
            return
        } else if type == 3 {
            viewerKind = 3
        } else if type == 12 {
            isSVG = true
            viewerKind = 3
        } else if type == 4 {
            viewerKind = 4
        } else {
            viewerKind = 0
        }
        showViewer = true
    }

    private func downloadThenInstall(_ item: WebDAVItem) {
        download(item, to: WebDAVCache.localPath(for: item),
                 label: "Downloading \(item.name)") { local in
            guard let local = local else { return }
            transferring = true
            transferLabel = "Installing \(item.name)"
            progress = 1
            IPAInstaller.install(localPath: local,
                                 remoteFallbackURL: client.directURL(for: item)) { result in
                transferring = false
                switch result {
                case .success(let log):
                    message = "Installed. \(log)"
                case .handedOff:
                    message = "Handed the IPA to TrollStore."
                case .failure(let error):
                    message = error
                }
            }
        }
    }
}

/// ⌘C·⌘X 로 담아 둔 원격 항목.
struct RemoteClip {
    let items: [WebDAVItem]
    let cut: Bool
    let serverID: UUID
}

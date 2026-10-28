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
            if screen == 2 {
                WebDAVBrowserView(server: workingServer,
                                  directory: $directory,
                                  isPresented: $isPresented,
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
    var onBack: () -> Void

    @State private var client: RemoteFileClient
    @State private var path: String
    @State private var items: [WebDAVItem] = []
    @State private var loading = false
    @State private var message = ""

    /// Where "copy" puts things: whatever folder the local browser is sitting on.
    @State private var destination: String

    @State private var detailItem: WebDAVItem?
    @State private var multiSelect = false
    @State private var picked: Set<String> = []

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
         onBack: @escaping () -> Void) {
        self.server = server
        self._directory = directory
        self._isPresented = isPresented
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
        .onAppear { reload() }
        // The remote's Menu button walks back up the tree, the way it does in
        // the local browser, instead of closing the whole sheet.
        .onExitCommand { goBack() }
        .sheet(isPresented: $showViewer) {
            viewerContent
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
                        let files = items.filter { !$0.isDirectory }
                        if picked.count == files.count {
                            picked.removeAll()
                        } else {
                            picked = Set(files.map { $0.path })
                        }
                    }) {
                        Image(systemName: "square.stack")
                            .frame(width: 50, height: 50)
                    }
                }

                Button(action: { copyPicked() }) {
                    ZStack {
                        Image(systemName: "doc.on.doc")
                            .frame(width: 50, height: 50)
                        Image(systemName: "arrow.down")
                            .resizable()
                            .frame(width: 15, height: 13)
                            .offset(x: -4, y: 11.75)
                    }
                }
                .disabled(transferring || picked.isEmpty)

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

    // MARK: Listing

    private var listPanel: some View {
        List(items) { item in
            Button(action: { open(item) }) {
                HStack {
                    if multiSelect && !item.isDirectory {
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
        }
    }

    private func icon(for item: WebDAVItem) -> String {
        if item.isDirectory { return "folder" }
        if item.isIPA { return "square.and.arrow.down" }
        return "doc"
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

                Button(action: { downloadThenView(item) }) {
                    HStack {
                        Image(systemName: "eye")
                        Text("View")
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
            path = WebDAVClient.parent(of: path)
            reload()
        } else {
            onBack()
        }
    }

    private func open(_ item: WebDAVItem) {
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
        } else {
            detailItem = item
        }
    }

    private func reload() {
        loading = true
        message = ""
        client.list(path: path) { result in
            loading = false
            switch result {
            case .success(let listed):
                items = listed
                if listed.isEmpty { message = "This folder is empty." }
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

    private func copyPicked() {
        let queue = items.filter { picked.contains($0.path) && !$0.isDirectory }
        guard !queue.isEmpty else {
            message = "Select some files first."
            return
        }
        copyStep(queue, 0)
    }

    private func copyStep(_ queue: [WebDAVItem], _ index: Int) {
        if index >= queue.count {
            transferring = false
            message = "Copied \(queue.count) file(s) to \(destination)"
            picked.removeAll()
            multiSelect = false
            return
        }
        let item = queue[index]
        download(item, to: localTarget(for: item),
                 label: "Copying \(item.name)  (\(index + 1)/\(queue.count))") { saved in
            if saved == nil { return }   // download() already reported the failure
            copyStep(queue, index + 1)
        }
    }

    private func downloadThenView(_ item: WebDAVItem) {
        download(item, to: WebDAVCache.localPath(for: item),
                 label: "Opening \(item.name)") { local in
            transferring = false
            guard let local = local else { return }
            localPath = local
            localName = item.name
            present(local)
        }
    }

    private func present(_ local: String) {
        let type = FileInfo.yandereDevFileType(file: local)
        isSVG = false
        if type == 1 {
            viewerKind = 1
        } else if type == 2 {
            viewerKind = 2
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

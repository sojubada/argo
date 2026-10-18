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
            Text("WebDAV")
                .font(.title)

            if store.servers.isEmpty {
                Text("No saved connections yet.")
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
                            Image(systemName: editMode ? "gear" : "globe")
                            VStack(alignment: .leading) {
                                Text(server.displayName)
                                Text(server.urlString)
                                    .font(.caption)
                                    .foregroundColor(.secondary)
                            }
                        }
                    }
                }
            }

            Text(editMode
                 ? "Edit mode: choose a connection to change its settings."
                 : "Choose a connection to browse it.")
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
                        Text("Add connection")
                    }
                }

                if !store.servers.isEmpty {
                    Button(action: { editMode.toggle() }) {
                        HStack {
                            Image(systemName: "gear")
                            Text(editMode ? "Done" : "Edit connections")
                        }
                    }
                }

                Button(action: {
                    directory = WebDAVCache.directory + "/"
                    isPresented = false
                }) {
                    HStack {
                        Image(systemName: "folder")
                        Text("Open download folder")
                    }
                }
            }

            Text("Downloads are staged in \(WebDAVCache.directory) - \(ByteCountFormatter.string(fromByteCount: WebDAVCache.usedBytes, countStyle: .file)) used")
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
            Text(isNew ? "New connection" : "Edit connection")
                .font(.title)

            TextField("Name (optional)", text: $server.name)
            TextField("https://host:port/path", text: $server.urlString)
            TextField("Username", text: $server.username)
            SecureField("Password", text: $server.password)
            Toggle("Allow untrusted certificates", isOn: $server.allowInsecureTLS)

            if !status.isEmpty {
                Text(status)
                    .font(.caption)
                    .foregroundColor(.secondary)
            }

            HStack {
                Button(action: test) {
                    Text(testing ? "Testing..." : "Test")
                }
                Button(action: {
                    if server.isValid {
                        onSave()
                    } else {
                        status = "Enter a valid http:// or https:// URL first."
                    }
                }) {
                    Text("Save")
                }
                Button(action: onCancel) {
                    Text("Cancel")
                }
                if !isNew {
                    Button(action: onDelete) {
                        Text("Delete")
                            .foregroundColor(.red)
                    }
                }
            }
        }
        .padding()
    }

    private func test() {
        guard server.isValid else {
            status = "Enter a valid http:// or https:// URL first."
            return
        }
        testing = true
        status = ""
        let client = WebDAVClient(server: server)
        client.testConnection { result in
            testing = false
            switch result {
            case .success:
                status = "Connected."
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

    @State private var client: WebDAVClient
    @State private var path: String
    @State private var items: [WebDAVItem] = []
    @State private var loading = false
    @State private var message = ""

    @State private var selected: WebDAVItem?
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
        self._client = State(initialValue: WebDAVClient(server: server))
        self._path = State(initialValue: server.rootPath)
    }

    var body: some View {
        VStack {
            header

            if transferring {
                Text("\(transferLabel) \(Int(progress * 100))%")
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

            if selected != nil {
                detailPanel
            } else {
                listPanel
            }
        }
        .onAppear { reload() }
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
            Text(server.displayName)
                .font(.title)
            Text(path)
                .font(.caption)
                .foregroundColor(.secondary)

            HStack {
                Button(action: {
                    if selected != nil {
                        selected = nil
                    } else if WebDAVClient.normalize(path) == WebDAVClient.normalize(server.rootPath) {
                        onBack()
                    } else {
                        path = WebDAVClient.parent(of: path)
                        reload()
                    }
                }) {
                    Image(systemName: "chevron.left")
                }
                Button(action: { reload() }) {
                    Image(systemName: "arrow.clockwise")
                }
                Button(action: { onBack() }) {
                    Text("Connections")
                }
                Button(action: { isPresented = false }) {
                    Text("Close")
                }
            }
        }
    }

    // MARK: Listing

    private var listPanel: some View {
        List(items) { item in
            Button(action: { open(item) }) {
                HStack {
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
            if let item = selected {
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
                        Text("Download and view")
                    }
                }
                .disabled(transferring)

                Button(action: { download(item) { _ in } }) {
                    HStack {
                        Image(systemName: "folder")
                        Text("Download only")
                    }
                }
                .disabled(transferring)

                Button(action: { selected = nil }) {
                    Text("Back")
                }
            }
        }
        .padding()
    }

    // MARK: Actions

    private func open(_ item: WebDAVItem) {
        if item.isDirectory {
            path = item.path
            reload()
        } else {
            selected = item
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

    private func download(_ item: WebDAVItem, completion: @escaping (String?) -> Void) {
        let destination = WebDAVCache.localPath(for: item)
        transferring = true
        progress = 0
        transferLabel = "Downloading \(item.name)"
        message = ""
        client.download(item: item, to: destination, progress: { fraction in
            progress = fraction
        }, completion: { result in
            transferring = false
            switch result {
            case .success(let localPath):
                message = "Saved to \(localPath)"
                completion(localPath)
            case .failure(let error):
                message = error.localizedDescription
                completion(nil)
            }
        })
    }

    private func downloadThenView(_ item: WebDAVItem) {
        download(item) { local in
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
        download(item) { local in
            guard let local = local else { return }
            transferring = true
            transferLabel = "Installing \(item.name)"
            progress = 1
            IPAInstaller.install(localPath: local,
                                 remoteFallbackURL: client.url(forPath: item.path)) { result in
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

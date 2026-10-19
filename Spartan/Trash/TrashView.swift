//
//  TrashView.swift
//  Argo
//

import SwiftUI

private enum TrashConfirmAction {
    case none
    case deleteSingle(TrashEntry)
    case deleteSelected
    case emptyAll
}

struct TrashView: View {
    @Binding var isPresented: Bool
    var onChanged: (() -> Void)? = nil

    @ObservedObject private var manager = TrashManager.shared

    @State private var detailEntry: TrashEntry? = nil
    @State private var multiSelect: Bool = false
    @State private var picked: Set<String> = []
    @State private var statusMessage: String = ""
    @State private var confirmAction: TrashConfirmAction = .none

    var body: some View {
        VStack(spacing: 16) {
            headerView

            if case .none = confirmAction {
                if let entry = detailEntry {
                    detailPanel(for: entry)
                } else {
                    listPanel
                }
            } else {
                confirmationPanel
            }

            if !statusMessage.isEmpty {
                Text(statusMessage)
                    .font(.caption)
                    .foregroundColor(.secondary)
                    .lineLimit(2)
            }
        }
        .padding(40)
        .onAppear {
            manager.reload()
        }
        .onExitCommand {
            handleExitCommand()
        }
    }

    // MARK: - Header & Toolbar

    private var headerView: some View {
        VStack(spacing: 10) {
            Text("Trash")
                .font(.title2)

            Text("\(manager.entries.count) item(s) · \(formatBytes(manager.totalSize))")
                .font(.caption)
                .foregroundColor(.secondary)

            HStack(spacing: 20) {
                // 새로고침
                Button(action: {
                    manager.reload()
                    statusMessage = "Refreshed."
                }) {
                    Image(systemName: "arrow.clockwise")
                        .frame(width: 50, height: 50)
                }

                // 선택모드 토글
                Button(action: {
                    if multiSelect {
                        multiSelect = false
                        picked.removeAll()
                    } else {
                        detailEntry = nil
                        confirmAction = .none
                        multiSelect = true
                    }
                }) {
                    Image(systemName: multiSelect ? "checkmark.circle.fill" : "checkmark.circle")
                        .frame(width: 50, height: 50)
                }
                .disabled(manager.entries.isEmpty)

                // 전체선택
                if multiSelect {
                    Button(action: {
                        if picked.count == manager.entries.count {
                            picked.removeAll()
                        } else {
                            picked = Set(manager.entries.map { $0.id })
                        }
                    }) {
                        Image(systemName: "square.stack")
                            .frame(width: 50, height: 50)
                    }
                }

                // 선택항목 복원
                if multiSelect {
                    Button(action: {
                        restoreSelected()
                    }) {
                        Image(systemName: "arrow.uturn.backward")
                            .frame(width: 50, height: 50)
                    }
                    .disabled(picked.isEmpty)
                }

                // 선택항목 영구삭제
                if multiSelect {
                    Button(action: {
                        confirmAction = .deleteSelected
                    }) {
                        Image(systemName: "trash.slash")
                            .frame(width: 50, height: 50)
                            .foregroundColor(.red)
                    }
                    .disabled(picked.isEmpty)
                }

                // 휴지통 비우기
                Button(action: {
                    confirmAction = .emptyAll
                }) {
                    Image(systemName: "trash")
                        .frame(width: 50, height: 50)
                        .foregroundColor(.red)
                }
                .disabled(manager.entries.isEmpty)

                // 닫기
                Button(action: {
                    onChanged?()
                    isPresented = false
                }) {
                    Image(systemName: "xmark")
                        .frame(width: 50, height: 50)
                }
            }
        }
    }

    // MARK: - List Panel

    private var listPanel: some View {
        Group {
            if manager.entries.isEmpty {
                Spacer()
                Text("Trash is empty.")
                    .font(.headline)
                    .foregroundColor(.secondary)
                Spacer()
            } else {
                List(manager.entries) { entry in
                    Button(action: {
                        selectRow(entry)
                    }) {
                        HStack(spacing: 16) {
                            if multiSelect {
                                Image(systemName: picked.contains(entry.id) ? "checkmark.circle.fill" : "circle")
                            }
                            Image(systemName: entry.isDirectory ? "folder" : "doc")
                            VStack(alignment: .leading, spacing: 4) {
                                Text(entry.originalName)
                                    .font(.headline)
                                Text("\(entry.isLegacy ? "Unknown original location" : entry.originalPath) · \(entry.deletedAtDescription) · \(formatBytes(entry.size))")
                                    .font(.caption)
                                    .foregroundColor(.secondary)
                            }
                        }
                    }
                }
            }
        }
    }

    // MARK: - Detail Panel

    private func detailPanel(for entry: TrashEntry) -> some View {
        VStack(alignment: .leading, spacing: 20) {
            HStack(spacing: 12) {
                Image(systemName: entry.isDirectory ? "folder" : "doc")
                    .font(.title2)
                Text(entry.originalName)
                    .font(.title3)
            }

            VStack(alignment: .leading, spacing: 8) {
                Text("Original Path: \(entry.isLegacy ? "Unknown original location" : entry.originalPath)")
                    .font(.caption)
                    .foregroundColor(.secondary)
                Text("Deleted At: \(entry.deletedAtDescription)")
                    .font(.caption)
                    .foregroundColor(.secondary)
                Text("Size: \(formatBytes(entry.size))")
                    .font(.caption)
                    .foregroundColor(.secondary)
            }

            HStack(spacing: 24) {
                Button(action: {
                    restoreSingle(entry)
                }) {
                    HStack {
                        Image(systemName: "arrow.uturn.backward")
                        Text("Restore")
                    }
                }
                .disabled(entry.isLegacy)

                Button(action: {
                    confirmAction = .deleteSingle(entry)
                }) {
                    HStack {
                        Image(systemName: "trash.slash")
                        Text("Delete Permanently")
                    }
                    .foregroundColor(.red)
                }

                Button(action: {
                    detailEntry = nil
                }) {
                    Text("Back")
                }
            }

            Spacer()
        }
        .padding()
    }

    // MARK: - Confirmation Panel

    private var confirmationPanel: some View {
        VStack(spacing: 24) {
            Spacer()

            Image(systemName: "exclamationmark.triangle.fill")
                .font(.largeTitle)
                .foregroundColor(.red)

            Text(confirmationTitle)
                .font(.headline)

            Text(confirmationMessage)
                .font(.caption)
                .foregroundColor(.secondary)
                .multilineTextAlignment(.center)

            HStack(spacing: 30) {
                Button(action: {
                    executeConfirmedAction()
                }) {
                    Text("Delete Permanently")
                        .foregroundColor(.red)
                }

                Button(action: {
                    confirmAction = .none
                }) {
                    Text("Cancel")
                }
            }

            Spacer()
        }
        .padding()
    }

    private var confirmationTitle: String {
        switch confirmAction {
        case .none:
            return ""
        case .deleteSingle(let entry):
            return "Delete \"\(entry.originalName)\" permanently?"
        case .deleteSelected:
            return "Delete \(picked.count) selected item(s) permanently?"
        case .emptyAll:
            return "Empty Trash?"
        }
    }

    private var confirmationMessage: String {
        switch confirmAction {
        case .none:
            return ""
        case .deleteSingle:
            return "This item will be permanently deleted and cannot be recovered."
        case .deleteSelected:
            return "The selected items will be permanently deleted and cannot be recovered."
        case .emptyAll:
            return "All \(manager.entries.count) item(s) in Trash will be permanently deleted and cannot be recovered."
        }
    }

    // MARK: - Actions

    private func selectRow(_ entry: TrashEntry) {
        if multiSelect {
            if picked.contains(entry.id) {
                picked.remove(entry.id)
            } else {
                picked.insert(entry.id)
            }
        } else {
            detailEntry = entry
        }
    }

    private func restoreSingle(_ entry: TrashEntry) {
        let result = manager.restore(entry)
        switch result {
        case .restored(let path):
            statusMessage = "Restored: \(path)"
            detailEntry = nil
            onChanged?()
        case .renamed(let path):
            let newName = (path as NSString).lastPathComponent
            statusMessage = "Name conflicted, restored as '\(newName)' (\(path))"
            detailEntry = nil
            onChanged?()
        case .failed(let reason):
            statusMessage = "Restore failed: \(reason)"
        }
    }

    private func restoreSelected() {
        let targets = manager.entries.filter { picked.contains($0.id) }
        guard !targets.isEmpty else { return }

        var restoredCount = 0
        var renamedCount = 0
        var failedCount = 0
        var lastMessage = ""

        for entry in targets {
            if entry.isLegacy {
                failedCount += 1
                continue
            }
            switch manager.restore(entry) {
            case .restored(let path):
                restoredCount += 1
                lastMessage = "Restored: \(path)"
            case .renamed(let path):
                renamedCount += 1
                let newName = (path as NSString).lastPathComponent
                lastMessage = "Name conflicted, restored as '\(newName)'"
            case .failed:
                failedCount += 1
            }
        }

        picked.removeAll()
        multiSelect = false
        onChanged?()

        if targets.count == 1 && !lastMessage.isEmpty {
            statusMessage = lastMessage
        } else {
            statusMessage = "Restored \(restoredCount), renamed \(renamedCount), failed \(failedCount)."
        }
    }

    private func executeConfirmedAction() {
        let current = confirmAction
        confirmAction = .none

        switch current {
        case .none:
            break
        case .deleteSingle(let entry):
            if manager.deleteForever(entry) {
                statusMessage = "Deleted permanently: \(entry.originalName)"
                detailEntry = nil
                onChanged?()
            } else {
                statusMessage = "Failed to delete \(entry.originalName)."
            }
        case .deleteSelected:
            let targets = manager.entries.filter { picked.contains($0.id) }
            var deletedCount = 0
            for entry in targets {
                if manager.deleteForever(entry) {
                    deletedCount += 1
                }
            }
            picked.removeAll()
            multiSelect = false
            statusMessage = "Permanently deleted \(deletedCount) item(s)."
            onChanged?()
        case .emptyAll:
            if manager.emptyAll() {
                detailEntry = nil
                picked.removeAll()
                multiSelect = false
                statusMessage = "Trash emptied."
                onChanged?()
            } else {
                statusMessage = "Some items could not be deleted."
            }
        }
    }

    private func handleExitCommand() {
        if case .none = confirmAction {
            if detailEntry != nil {
                detailEntry = nil
            } else if multiSelect {
                multiSelect = false
                picked.removeAll()
            } else {
                onChanged?()
                isPresented = false
            }
        } else {
            confirmAction = .none
        }
    }

    private func formatBytes(_ bytes: Int64) -> String {
        return ByteCountFormatter.string(fromByteCount: bytes, countStyle: .file)
    }
}
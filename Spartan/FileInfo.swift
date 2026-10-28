//
//  FileTypes.swift
//  Spartan
//
//  Created by WhitetailAni on 11/20/23.
//

import Foundation
import AVFoundation
import libarchiveBridge
import Zip

class FileInfo {
    class func yandereDevFileType(file: String) -> Double { //I tried using unified file types but they all returned nil so I have to use this awful yandere dev shit
        //im sorry
        
        //FUTURE ME WANTS YOU TO KNOW I AM TALKING ABOUT THE IF ELSE STACK POST NOT THE PEDO STUFF
        
        //also apparently i look like yandere dev. thanks ethan
        //i tried to update this to be faster using async dispatch, but it was like 50 times slower and locked up half the time. you can see the attempt below, maybe someday i'll fix it
        
        if (isSymlink(filePath: file)) {
            return 8 //symlink
        } else if (isDirectory(filePath: file)) {
            return 0 //directory
        } else if (isVideo(filePath: file)) { //video has to come first as otherwise video files detect as audio (since they are audio files as well)
            return 2 //video file
        } else if (isAudio(filePath: file)) {
            return 1 //audio file
        } else if (isImage(filePath: file)) {
            return 3 //image
        } else if (isPlist(filePath: file) != 0) {
            return isPlist(filePath: file)
            //5.1 = xml plist
            //5.9 = bplist
        } else if (isHTML(filePath: file)) {
            return 14 //html
        } else if (isPDF(filePath: file)) {
            return 15 //pdf - ahead of the executable and archive checks, which
                       //would otherwise claim a PDF carrying the +x bit and
                       //would read the whole file to do it
        } else if (isOfficeDoc(filePath: file)) {
            return 16 //office & hwp document - docx/xlsx/pptx/hwpx are ZIP
                       //containers, so this has to beat isArchive to them
        } else if (doesFileHaveFileExtension(filePath: file, extensions: [".dmg"])) {
            return 13 //dmg
        } else if (doesFileHaveFileExtension(filePath: file, extensions: [".svg"])) {
            return 12 //svg
        } else if (doesFileHaveFileExtension(filePath: file, extensions: [".ttf", ".otf", ".ttc", ".pfb", ".pfa"])) {
            return 11 //a font (badly)
        } else if (ArgoTextKind.textExtensions.contains((file as NSString).pathExtension.lowercased())
                   || ArgoTextKind.language(for: file) == .makefile) {
            // 확장자로 아는 글 파일. 내용 검사보다 먼저 본다 — 실행 권한이 붙은
            // .sh 는 '실행 파일' 로, CP949 로 된 .txt 는 '모르는 파일' 로 갔다.
            return 4 //text file
        } else if(isCar(filePath: file)) {
            return 10 //asset catalog
        } else if (fileManager.isExecutableFile(atPath: file)) { //executables detect as utf32 lol
            return 7 //executable
        } else if (isArchive(filePath: file)) {
            return 6 //archive
        } else if (isText(filePath: file)) { //these must be flipped because otherwise xml plist detects as text
            return 4 //text file
        } else if (doesFileHaveFileExtension(filePath: file, extensions: [".deb"])) {
            return 9 //deb
        } else {
            return 69 //unknown
        }
    }
    
    class func yandereDevFileType2(file: String) -> Double {
        //this is my attempt to make filetypes faster.
        //however, it's slower!
        //how wonderful
        var filetype: Double = 69
        var checkingComplete = [false, false, false, false]
        
        DispatchQueue.global().asyncAfter(deadline: .now() + 0.01) {
            if (isSymlink(filePath: file)) {
                filetype = 8 //symlink
            } else if (isDirectory(filePath: file)) {
                filetype = 0 //directory
            } //symlinks detect as directories so symlink check first
            checkingComplete[0] = true
        } //they're split out into groups where order matters. symlink has to come before directory, video has to come before audio, etc.
        DispatchQueue.global().asyncAfter(deadline: .now() + 0.02) {
            if (isVideo(filePath: file)) { //video has to come first as otherwise video files detect as audio (since they are audio files as well)
                filetype = 2 //video file
            } else if (isAudio(filePath: file)) {
                filetype = 1 //audio file
            } else if (isImage(filePath: file)) {
                filetype = 3 //image
            }
            checkingComplete[1] = true
        }
        DispatchQueue.global().asyncAfter(deadline: .now() + 0.03) {
            if (doesFileHaveFileExtension(filePath: file, extensions: [".dmg"])) {
                filetype = 13 //dmg
            } else if (doesFileHaveFileExtension(filePath: file, extensions: [".svg"])) {
                filetype = 12 //svg
            } else if (doesFileHaveFileExtension(filePath: file, extensions: [".ttf", ".otf", ".ttc", ".pfb", ".pfa"])) {
                filetype = 11 //a font (badly)
            } else if (doesFileHaveFileExtension(filePath: file, extensions: [".zip", ".cbz"])) {
                filetype = 6 //archive
            } else if (doesFileHaveFileExtension(filePath: file, extensions: [".deb"])) {
                filetype = 9 //deb
            }
            checkingComplete[2] = true
        }
        DispatchQueue.global().asyncAfter(deadline: .now() + 0.04) {
            if (isPlist(filePath: file) != 0) {
                filetype = isPlist(filePath: file)
                //5.1 = xml plist
                //5.9 = bplist
            } else if (isHTML(filePath: file)) {
                filetype = 14 //html
            } else if(isCar(filePath: file)) {
                filetype = 10 //asset catalog
            } else if (fileManager.isExecutableFile(atPath: file)) { //executables detect as utf32 lol
                filetype = 7 //executable
            } else if (isText(filePath: file)) { //these must be flipped because otherwise xml plist detects as text
                filetype = 4 //text file
            }
            checkingComplete[3] = true
        }
        while filetype == 69 { } //wait for checking to finish
        return filetype
    }
    class func isDirectory(filePath: String) -> Bool {
        var isDirectory: ObjCBool = false
        FileManager.default.fileExists(atPath: filePath, isDirectory: &isDirectory)
        return isDirectory.boolValue
    }
    class func isAudio(filePath: String) -> Bool {
        let fileURL = URL(fileURLWithPath: filePath)
        let asset = AVAsset(url: fileURL)
        let playableKey = "playable"

        let playablePredicate = NSPredicate(format: "%K == %@", playableKey, NSNumber(value: true))
        let playableItems = asset.tracks(withMediaCharacteristic: .audible).filter { playablePredicate.evaluate(with: $0) }

        return playableItems.count > 0
    }
    class func isVideo(filePath: String) -> Bool {
        let fileURL = URL(fileURLWithPath: filePath)
        let asset = AVAsset(url: fileURL)
        let playableKey = "playable"

        let playablePredicate = NSPredicate(format: "%K == %@", playableKey, NSNumber(value: true))
        let playableItems = asset.tracks(withMediaCharacteristic: .visual).filter { playablePredicate.evaluate(with: $0) }

        return playableItems.count > 0
    }
    class func isImage(filePath: String) -> Bool {
        guard fileManager.fileExists(atPath: filePath) else {
            return false
        }
    
        if let image = UIImage(contentsOfFile: filePath) {
            return image.size.width > 0 && image.size.height > 0
        }
        return false
    }
    class func isText(filePath: String) -> Bool {
        // 앞부분만 본다. 전에는 파일을 통째로 읽어 판정해서, 큰 파일이 든
        // 폴더를 열 때마다 그 파일을 전부 읽었다.
        guard let data = headBytes(filePath, 64 * 1024), !data.isEmpty else {
            return false
        }
        // UTF-16 은 BOM 으로 안다. 그 밖에 0 바이트가 있으면 바이너리다.
        if data.starts(with: [0xFF, 0xFE]) || data.starts(with: [0xFE, 0xFF]) { return true }
        if data.contains(0) { return false }
        // 잘린 끝의 반쪽 글자 때문에 UTF-8 판정이 실패하지 않게 몇 바이트 떼고 본다.
        let probe = data.count > 8 ? data.prefix(data.count - 4) : data
        if String(data: probe, encoding: .utf8) != nil { return true }
        if String(data: probe, encoding: ArgoTextCodec.cp949) != nil { return true }
        // 제어 글자가 거의 없으면 글로 친다.
        let controls = data.filter { $0 < 0x09 || ($0 > 0x0D && $0 < 0x20) }.count
        return controls * 100 < data.count
    }

    /// 파일 앞부분만 읽는다.
    class func headBytes(_ path: String, _ count: Int) -> Data? {
        guard let h = FileHandle(forReadingAtPath: path) else { return nil }
        defer { h.closeFile() }
        return h.readData(ofLength: count)
    }
    class func isPlist(filePath: String) -> Double {
        // 머리 몇 바이트면 된다. 통째로 읽지 않는다.
        guard let data = headBytes(filePath, 16) else {
            return 0
        }
        
        if data.count > 5 {
            if let header = String(data: data.subdata(in: 0..<5), encoding: .utf8) {
                if header == "<?xml" {
                    return 5.1
                } else if header == "bplis" {
                    return 5.9
                }
            }
        }
        return 0
    }
    class func isSymlink(filePath: String) -> Bool {
        let fileURL = URL(fileURLWithPath: filePath)
        
        do {
            let resourceValues = try fileURL.resourceValues(forKeys: [.isSymbolicLinkKey])
            if let isSymbolicLink = resourceValues.isSymbolicLink {
                return isSymbolicLink
            }
        } catch {
            print("Error: \(error)")
        }
        return false
    }
    class func isCar(filePath: String) -> Bool {
        guard let data = fileManager.contents(atPath: filePath) else {
            return false
        }
        if data.count > 8 {
            if let header = String(data: data.subdata(in: 0..<8), encoding: .utf8) {
                return header == "BOMStore"
            }
        }
        return false
    }
    class func isHTML(filePath: String) -> Bool {
        guard let data = fileManager.contents(atPath: filePath) else {
            return false
        }
        if data.count > 15 {
            if let header = String(data: data.subdata(in: 0..<15), encoding: .utf8) {
                return header == "<!DOCTYPE html>"
            }
        }
        return false
    }
class func isOfficeDoc(filePath: String) -> Bool {
        let ext = (filePath as NSString).pathExtension.lowercased()
        let officeExtensions: Set<String> = [
            "doc", "docx",
            "xls", "xlsx",
            "ppt", "pptx",
            "hwp", "hwpx"
        ]
        return officeExtensions.contains(ext)
    }
    class func isPDF(filePath: String) -> Bool {
        if doesFileHaveFileExtension(filePath: filePath, extensions: [".pdf"]) {
            return true
        }
        // Read only the signature. isArchive() above pulls whole files into
        // memory, so this check is placed ahead of it in the chain to keep a
        // large PDF from being slurped just to be classified.
        guard let handle = FileHandle(forReadingAtPath: filePath) else {
            return false
        }
        defer {
            try? handle.close()
        }
        let headerData = handle.readData(ofLength: 5)
        if headerData.count == 5,
           let header = String(data: headerData, encoding: .ascii) {
            return header == "%PDF-"
        }
        return false
    }
    class func isArchive(filePath: String) -> Bool {
        guard let fileCString = filePath.cString(using: .utf8) else {
            return false
        }
        
        guard let fileData = fileManager.contents(atPath: filePath) else {
            return false
        }
        if fileData.count > 2 {
            if fileData.subdata(in: 0..<2) == Data(fromHexEncodedString: "504B") {
                return true
            }
        }

        let archive = archive_read_new()
        var entry = archive_entry_new()
        
        archive_read_support_format_all(archive)
        archive_read_support_filter_all(archive)
        archive_read_support_filter_lzma(archive)

        guard archive_read_open_filename(archive, fileCString, 10240) == ARCHIVE_OK else {
            archive_read_close(archive)
            archive_read_free(archive)
            return false
        }

        if archive_read_next_header(archive, &entry) == ARCHIVE_OK {
            archive_read_close(archive)
            archive_read_free(archive)
            return true
        }

        archive_read_close(archive)
        archive_read_free(archive)
        return false
    }
class func doesFileHaveFileExtension(filePath: String, extensions: [String]) -> Bool {
        let lowerPath = filePath.lowercased()
        for ext in extensions {
            let normalizedExt = ext.hasPrefix(".") ? ext.lowercased() : ("." + ext.lowercased())
            if lowerPath.hasSuffix(normalizedExt) {
                return true
            }
        }
        return false
    }
    
    class func isDirectoryEmpty(atPath: String) -> Int {
        do {
            let files = try fileManager.contentsOfDirectory(atPath: atPath)
            return files.isEmpty ? 1 : 0
        } catch {
            return 2
        }
    }
    
    class func getFileInfo(filePath: String) -> [String] {
        do {
            let attributes = try fileManager.attributesOfItem(atPath: filePath)
    
            let creationDate = attributes[.creationDate] as? Date ?? Date.distantPast
            let modificationDate = attributes[.modificationDate] as? Date ?? Date.distantPast
            
            let fileSize = attributes[.size] as? Int ?? 0
            
            let fileOwner: String = ((attributes[.ownerAccountName] as? String) ?? "")
            
            let fileOwnerID = attributes[.groupOwnerAccountID] as? Int ?? 0
            let filePerms = String(format: "%03d", attributes[.posixPermissions] as? Int ?? "000")
            
            let dateFormatter = DateFormatter()
            dateFormatter.dateStyle = .medium
            dateFormatter.timeStyle = .medium
            
            var fileInfoString: [String] = []
            fileInfoString.append(NSLocalizedString("INFO_PATH", comment: "Ma! I got a thing going here.") + filePath)
            fileInfoString.append(NSLocalizedString("INFO_SIZE", comment: "- You got lint on your fuzz.") + ByteCountFormatter().string(fromByteCount: Int64(fileSize)))
            fileInfoString.append(NSLocalizedString("INFO_CREATION", comment: "- Ow! That's me!") + dateFormatter.string(from: creationDate))
            fileInfoString.append(NSLocalizedString("INFO_MODIFICATION", comment: "- Wave to us! We'll be in row 118,000.") + dateFormatter.string(from: modificationDate))
            fileInfoString.append(NSLocalizedString("INFO_OWNER", comment: "- Bye!") + fileOwner)
            fileInfoString.append(NSLocalizedString("INFO_OWNERID", comment: "Barry, I told you, stop flying in the house!") + String(fileOwnerID))
            fileInfoString.append(NSLocalizedString("INFO_PERMISSIONS", comment: "- Hey, Adam.") + filePerms)
            
            return fileInfoString
        } catch {
            return ["Error: \(error.localizedDescription)"]
        }
    }
    
    class func readSymlinkDestination(path: String) throws -> String {
        var rawPath = "/"
        do {
             rawPath += try fileManager.destinationOfSymbolicLink(atPath: removeLastChar(path))
        } catch {
            print("Make a valid symlink please. (Error ID 167)")
            throw "167"
        }
        
        let rawPathType = FileInfo.yandereDevFileType(file: rawPath)
        if rawPathType == 0 || rawPathType == 8 {
            rawPath += "/"
        }
        
        return rawPath
    }    
    
    class func isTar(filePath: String) -> Bool {
        return false
    }
}

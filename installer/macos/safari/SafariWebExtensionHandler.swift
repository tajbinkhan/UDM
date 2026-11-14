// UDM's side of Safari's native messaging, in the app extension that
// installer/macos/safari/build.sh puts in UDM.app/Contents/PlugIns. It takes
// the place of the handler Xcode's safari-web-extension-converter writes.
//
// Safari hands each runtime.sendNativeMessage of the extension's worker to
// beginRequest, and its answer back; nothing here can send the worker a
// message of its own. So the worker polls (extension/chromium/native.js),
// and this keeps one `udm-host safari` running between requests: each
// request's frames go to its standard input, and the answer carries every
// frame it wrote to its standard output since the last request.
//
//   request: { "frames": [message, ...] }
//   answer:  { "session": id, "frames": [message, ...], "more": bool, "error": text }
//
// `session` names the udm-host that answered, so the worker knows to say
// hello again to a new one; `error` says that udm-host stopped, and why (the
// last line it wrote to standard error), and the next request starts another.
//
// The app extension runs in Safari's sandbox, and so does udm-host, which
// it starts (udm-host.entitlements). udm-host starts no daemon there: UDM
// must already be running.

import Foundation
import SafariServices

/// The key of a native message in an extension item's userInfo: the value
/// of SFExtensionMessageKey, spelled out so that older SDKs build this too.
private let messageKey = "message"

final class SafariWebExtensionHandler: NSObject, NSExtensionRequestHandling {
    func beginRequest(with context: NSExtensionContext) {
        let item = context.inputItems.first as? NSExtensionItem
        let message = item?.userInfo?[messageKey] as? [String: Any]
        let frames = message?["frames"] as? [Any] ?? []
        Host.shared.exchange(frames) { reply in
            let response = NSExtensionItem()
            response.userInfo = [messageKey: reply]
            context.completeRequest(returningItems: [response], completionHandler: nil)
        }
    }
}

/// The one udm-host of this app extension, and the requests that use it,
/// one at a time.
private final class Host {
    static let shared = Host()

    /// How long a request waits for udm-host's first frame: an answer to
    /// what it sent, or, for a poll that sent nothing, news.
    private let replyWait: TimeInterval = 2
    private let pollWait: TimeInterval = 0.5

    private let queue = DispatchQueue(label: "com.udm.app.safari-extension.host")
    private var connection: Connection?

    private init() {
        // A write to a udm-host that just stopped must fail, not end this
        // process.
        signal(SIGPIPE, SIG_IGN)
    }

    func exchange(_ frames: [Any], reply: @escaping ([String: Any]) -> Void) {
        queue.async { reply(self.answer(frames)) }
    }

    private func answer(_ frames: [Any]) -> [String: Any] {
        let current: Connection
        if let running = connection {
            current = running
        } else {
            do {
                current = try Connection(program: Host.program())
            } catch {
                return ["frames": [Any](), "error": "Could not start udm-host: \(error.localizedDescription)"]
            }
            connection = current
        }
        let sent = current.send(frames)
        // A write that failed means udm-host stopped: wait for its reason.
        let taken = current.take(waiting: !sent ? 1 : frames.isEmpty ? pollWait : replyWait)
        var reply: [String: Any] = ["session": current.session, "frames": taken.frames, "more": taken.more]
        if let ended = taken.ended {
            reply["error"] = ended
            connection = nil
        }
        return reply
    }

    /// udm-host: the copy signed for the sandbox inside this app extension,
    /// else UDM.app's own (this app extension is in UDM.app/Contents/PlugIns).
    private static func program() -> URL {
        if let own = Bundle.main.url(forAuxiliaryExecutable: "udm-host") {
            return own
        }
        return Bundle.main.bundleURL
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("MacOS/udm-host")
    }
}

/// One running udm-host: what goes to it, and the frames and the end it
/// reported, which its reader threads collect.
private final class Connection {
    /// The most a reply carries, so a long list does not make one message
    /// too large: the rest waits for the next request (`more`).
    private static let replyBytes = 4 << 20
    /// The largest frame udm-host writes; a larger length means the stream
    /// is broken.
    private static let frameBytes = 64 << 20

    let session = UUID().uuidString
    private let process = Process()
    private let input = Pipe()
    private let output = Pipe()
    private let errors = Pipe()
    private let errorsRead = DispatchGroup()
    private let state = NSCondition()
    // Guarded by `state`.
    private var frames: [(object: Any, bytes: Int)] = []
    private var ended: String?
    private var errorText = ""

    init(program: URL) throws {
        process.executableURL = program
        process.arguments = ["safari"]
        // In the sandbox HOME is this app extension's container; UDM's data
        // folder, with the daemon's token, is in the user's own.
        var environment = ProcessInfo.processInfo.environment
        if let home = Connection.userHome() {
            environment["HOME"] = home
        }
        process.environment = environment
        process.standardInput = input
        process.standardOutput = output
        process.standardError = errors
        try process.run()
        errorsRead.enter()
        Thread { self.readErrors() }.start()
        Thread { self.readFrames() }.start()
    }

    /// Write `messages` to udm-host as native-messaging frames. False when
    /// it no longer reads them.
    func send(_ messages: [Any]) -> Bool {
        var data = Data()
        for message in messages {
            guard JSONSerialization.isValidJSONObject(message),
                  let json = try? JSONSerialization.data(withJSONObject: message)
            else { continue }
            withUnsafeBytes(of: UInt32(json.count).littleEndian) { data.append(contentsOf: $0) }
            data.append(json)
        }
        if data.isEmpty {
            return true
        }
        do {
            try input.fileHandleForWriting.write(contentsOf: data)
            return true
        } catch {
            return false
        }
    }

    /// The frames udm-host wrote, waiting up to `seconds` for the first;
    /// whether more are left for the next request; and, once every frame is
    /// taken, why it stopped, if it did.
    func take(waiting seconds: TimeInterval) -> (frames: [Any], more: Bool, ended: String?) {
        state.lock()
        defer { state.unlock() }
        let deadline = Date(timeIntervalSinceNow: seconds)
        while frames.isEmpty, ended == nil, state.wait(until: deadline) {}
        var taken: [Any] = []
        var bytes = 0
        while let first = frames.first, taken.isEmpty || bytes + first.bytes <= Connection.replyBytes {
            taken.append(first.object)
            bytes += first.bytes
            frames.removeFirst()
        }
        return (frames: taken, more: !frames.isEmpty, ended: frames.isEmpty ? ended : nil)
    }

    private func readFrames() {
        let handle = output.fileHandleForReading
        var buffer = Data()
        reading: while true {
            let chunk = handle.availableData
            if chunk.isEmpty {
                break
            }
            buffer.append(chunk)
            while buffer.count >= 4 {
                let start = buffer.startIndex
                let length = buffer[start ..< start + 4].reversed().reduce(0) { $0 << 8 | Int($1) }
                if length > Connection.frameBytes {
                    process.terminate()
                    break reading
                }
                guard buffer.count >= 4 + length else { break }
                let payload = buffer.subdata(in: start + 4 ..< start + 4 + length)
                buffer.removeSubrange(start ..< start + 4 + length)
                guard let object = try? JSONSerialization.jsonObject(with: payload) else { continue }
                state.lock()
                frames.append((object: object, bytes: length))
                state.broadcast()
                state.unlock()
            }
        }
        process.waitUntilExit()
        _ = errorsRead.wait(timeout: .now() + 1)
        state.lock()
        let last = errorText.split(whereSeparator: \.isNewline).last.map(String.init)
        ended = last ?? "udm-host stopped (exit status \(process.terminationStatus))"
        state.broadcast()
        state.unlock()
    }

    /// Keep the end of what udm-host writes to standard error: its last line
    /// says why it stopped.
    private func readErrors() {
        let handle = errors.fileHandleForReading
        while true {
            let chunk = handle.availableData
            if chunk.isEmpty {
                break
            }
            state.lock()
            errorText = String((errorText + String(decoding: chunk, as: UTF8.self)).suffix(4096))
            state.unlock()
        }
        errorsRead.leave()
    }

    /// The user's home folder, which HOME does not name in the sandbox.
    private static func userHome() -> String? {
        guard let entry = getpwuid(getuid()), let dir = entry.pointee.pw_dir else { return nil }
        return String(cString: dir)
    }
}

import Foundation

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Foundation.Data((message + "\n").utf8))
    exit(1)
}

guard CommandLine.arguments.count == 2 || CommandLine.arguments.count == 3 else {
    fail("usage: jaw_swift INPUT [OUTPUT]")
}

do {
    let inputURL = URL(fileURLWithPath: CommandLine.arguments[1])
    let source = try Foundation.Data(contentsOf: inputURL)
    var reader = JawDataReader(source)
    var messages: [Root] = []
    for _ in 0..<7 {
        messages.append(try Root.jawRead(from: &reader))
    }
    guard reader.offset == source.count else {
        fail("Swift decoder did not consume the complete input")
    }

    var writer = JawDataWriter()
    for message in messages {
        try message.jawWrite(to: &writer)
    }
    guard writer.data == source else {
        fail("Swift round-trip changed the wire representation")
    }

    let inputStream = InputStream(data: source)
    inputStream.open()
    defer { inputStream.close() }
    var streamReader = JawInputStream(inputStream)
    var streamedMessages: [Root] = []
    for _ in 0..<7 {
        streamedMessages.append(try Root.jawRead(from: &streamReader))
    }
    guard streamedMessages == messages else {
        fail("Swift InputStream adapter changed decoded values")
    }

    let outputStream = OutputStream.toMemory()
    outputStream.open()
    defer { outputStream.close() }
    var streamWriter = JawOutputStream(outputStream)
    for message in messages {
        try message.jawWrite(to: &streamWriter)
    }
    try streamWriter.flush()
    guard
        let streamedData = outputStream.property(forKey: .dataWrittenToMemoryStreamKey)
            as? Foundation.Data,
        streamedData == source
    else {
        fail("Swift OutputStream adapter changed the wire representation")
    }

    if CommandLine.arguments.count == 3 {
        let outputURL = URL(fileURLWithPath: CommandLine.arguments[2])
        try writer.data.write(to: outputURL)
    }
} catch {
    fail("Swift driver failed: \(error)")
}

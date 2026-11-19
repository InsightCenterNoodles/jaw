import Foundation

@inline(never)
func measure(
    _ label: String,
    bytes: Int,
    iterations: Int,
    _ operation: () throws -> Int
) rethrows -> Int {
    let clock = ContinuousClock()
    var checksum = 0
    let start = clock.now
    for _ in 0..<iterations {
        checksum &+= try operation()
    }
    let elapsed = start.duration(to: clock.now)
    let components = elapsed.components
    let seconds = Double(components.seconds)
        + Double(components.attoseconds) / 1_000_000_000_000_000_000
    let totalBytes = Double(bytes * iterations)
    let throughput = totalBytes / max(seconds, .leastNonzeroMagnitude) / (1024 * 1024)
    let nanos = seconds * 1_000_000_000 / Double(iterations)
    print(
        "\(label.padding(toLength: 30, withPad: " ", startingAt: 0)) "
            + String(format: "%10.0f ns/op %10.1f MiB/s", nanos, throughput)
    )
    return checksum
}

func encoded<Value: JawWritable>(_ value: Value, capacity: Int = 0) throws -> Foundation.Data {
    var writer = JawDataWriter(minimumCapacity: capacity)
    try value.jawWrite(to: &writer)
    return writer.data
}

func run() throws {
    let byteCount = 1024 * 1024
    let floatCount = byteCount / MemoryLayout<Float>.size
    let recordCount = byteCount / SmallRecord.jawEncodedSize

    let byteBlob = ByteBlob((0..<byteCount).map { UInt8(truncatingIfNeeded: $0) })
    let floatBlob = FloatBlob((0..<floatCount).map { Float($0) * 0.25 })
    let records = RecordList(
        (0..<recordCount).map { SmallRecord(tag: UInt32($0), value: Double($0) * 0.5) }
    )
    let envelope = Envelope(
        header: SmallRecord(tag: 0x1234_5678, value: 42.5),
        bytes: byteBlob,
        samples: floatBlob,
        records: records
    )

    let byteData = try encoded(byteBlob, capacity: byteCount + 4)
    let floatData = try encoded(floatBlob, capacity: byteCount + 4)
    let recordData = try encoded(records, capacity: byteCount + 4)
    let envelopeData = try encoded(envelope)
    var checksum = 0

    checksum &+= try measure("encode [UInt8] 1 MiB", bytes: byteData.count, iterations: 200) {
        var writer = JawDataWriter(minimumCapacity: byteData.count)
        try byteBlob.jawWrite(to: &writer)
        return writer.data.count ^ Int(writer.data[writer.data.count / 2])
    }
    checksum &+= try measure("decode [UInt8] 1 MiB", bytes: byteData.count, iterations: 200) {
        var reader = JawDataReader(byteData)
        let decoded = try ByteBlob.jawRead(from: &reader).values
        return decoded.count ^ Int(decoded[decoded.count / 2])
    }
    checksum &+= try measure("decode stream [UInt8]", bytes: byteData.count, iterations: 100) {
        let stream = InputStream(data: byteData)
        stream.open()
        defer { stream.close() }
        var reader = JawInputStream(stream)
        let decoded = try ByteBlob.jawRead(from: &reader).values
        return decoded.count ^ Int(decoded[decoded.count / 2])
    }
    checksum &+= try measure("encode [Float] 1 MiB", bytes: floatData.count, iterations: 200) {
        var writer = JawDataWriter(minimumCapacity: floatData.count)
        try floatBlob.jawWrite(to: &writer)
        return writer.data.count ^ Int(writer.data[writer.data.count / 2])
    }
    checksum &+= try measure("decode [Float] 1 MiB", bytes: floatData.count, iterations: 200) {
        var reader = JawDataReader(floatData)
        let decoded = try FloatBlob.jawRead(from: &reader).values
        return decoded.count ^ Int(decoded[decoded.count / 2].bitPattern)
    }
    checksum &+= try measure("records encode ~1 MiB", bytes: recordData.count, iterations: 50) {
        var writer = JawDataWriter(minimumCapacity: recordData.count)
        try records.jawWrite(to: &writer)
        return writer.data.count ^ Int(writer.data[writer.data.count / 2])
    }
    checksum &+= try measure("records decode ~1 MiB", bytes: recordData.count, iterations: 50) {
        var reader = JawDataReader(recordData)
        let decoded = try RecordList.jawRead(from: &reader).values
        return decoded.count ^ Int(decoded[decoded.count / 2].tag)
    }
    checksum &+= try measure("nested envelope encode", bytes: envelopeData.count, iterations: 25) {
        var writer = JawDataWriter(minimumCapacity: envelopeData.count)
        try envelope.jawWrite(to: &writer)
        return writer.data.count ^ Int(writer.data[writer.data.count / 2])
    }
    checksum &+= try measure("nested envelope decode", bytes: envelopeData.count, iterations: 25) {
        var reader = JawDataReader(envelopeData)
        let decoded = try Envelope.jawRead(from: &reader)
        let values = decoded.records.values
        return values.count ^ Int(values[values.count / 2].tag)
    }

    var reused = JawDataWriter(minimumCapacity: envelopeData.count)
    checksum &+= try measure("reused envelope writer", bytes: envelopeData.count, iterations: 25) {
        reused.reset()
        try envelope.jawWrite(to: &reused)
        return reused.data.count ^ Int(reused.data[reused.data.count / 2])
    }
    print("checksum: \(checksum)")
}

do {
    try run()
} catch {
    FileHandle.standardError.write(Foundation.Data("benchmark failed: \(error)\n".utf8))
    exit(1)
}

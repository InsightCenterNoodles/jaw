import Foundation

// Alias the generated namespace for convenience
typealias G = basic

func demand(_ cond: Bool, _ label: String) {
    if !cond { fputs("Error: \(label)\n", stderr); exit(1) }
}

func buildExpected() -> [G.Root] {
    var expected: [G.Root] = []

    // 1) Variant = MyPOD
    do {
        let name = Data("pod-one".utf8)
        let pod = G.MyPOD(a_thing: 10, b_thing: 20)
        let var0 = G.MyVariant.alt0(pod)
        let r = G.Root(name: name, _var: var0)
        expected.append(r)
    }

    // 2) Variant = MyOtherPOD
    do {
        let name = Data()
        let first = G.MyPOD(a_thing: 1, b_thing: 0x1122334455667788)
        let bytes = Data([1,2,3,4])
        let second = try! G.FixedData_4(bytes)
        let other = G.MyOtherPOD(first: first, second: second)
        let var1 = G.MyVariant.alt1(other)
        let r = G.Root(name: name, _var: var1)
        expected.append(r)
    }

    // 3) Variant = SmallSeq with floats
    do {
        let name = Data("floats".utf8)
        let seq = G.SmallSeq(list: [1.0, 2.5, -3.25, 0.0])
        let r = G.Root(name: name, _var: G.MyVariant.defaultAlt(seq))
        expected.append(r)
    }

    // 4) Another MyPOD with larger values
    do {
        let name = Data("pod-two".utf8)
        let pod = G.MyPOD(a_thing: 255, b_thing: 0xCAFEBABECAFED00D)
        let r = G.Root(name: name, _var: G.MyVariant.alt0(pod))
        expected.append(r)
    }

    // 5) Another SmallSeq, longer
    do {
        let name = Data("seq-long".utf8)
        var list: [Float] = []
        for i in 0..<10 { list.append(Float(i) * 0.5) }
        let seq = G.SmallSeq(list: list)
        let r = G.Root(name: name, _var: G.MyVariant.defaultAlt(seq))
        expected.append(r)
    }

    return expected
}

func encodeAll(_ msgs: [G.Root]) throws -> Data {
    var w = G.Writer()
    for r in msgs {
        try G.write_Root(&w, r)
    }
    return w.data()
}

func decodeAll(_ buf: Data, _ count: Int) throws -> [G.Root] {
    var r = G.Reader(buf)
    var out: [G.Root] = []
    for _ in 0..<count {
        out.append(try G.read_Root(&r))
    }
    return out
}

func compareExpectedActual(_ expected: [G.Root], _ actual: [G.Root]) {
    demand(expected.count == actual.count, "size matches")
    for i in 0..<expected.count {
        let e = expected[i]
        let a = actual[i]
        demand(a.name == e.name, "name matches at \(i)")
        switch (e._var, a._var) {
        case (.alt0(let ep), .alt0(let ap)):
            demand(ap.a_thing == ep.a_thing, "MyPOD.a_thing at \(i)")
            demand(ap.b_thing == ep.b_thing, "MyPOD.b_thing at \(i)")
        case (.alt1(let eo), .alt1(let ao)):
            demand(ao.first.a_thing == eo.first.a_thing, "MyOtherPOD.first.a_thing at \(i)")
            demand(ao.first.b_thing == eo.first.b_thing, "MyOtherPOD.first.b_thing at \(i)")
            let asec = ao.second.data
            let esec = eo.second.data
            for j in 0..<4 {
                demand(asec[j] == esec[j], "MyOtherPOD.second[\(j)] at \(i)")
            }
        case (.defaultAlt(let es), .defaultAlt(let as_)):
            demand(as_.list.count == es.list.count, "SmallSeq.size at \(i)")
            for j in 0..<as_.list.count {
                demand(as_.list[j] == es.list[j], "SmallSeq.value[\(j)] at \(i)")
            }
        default:
            demand(false, "variant tag matches at \(i)")
        }
    }
}

func usage() {
    fputs("Usage: jaw_swift [--dump PATH] [--read PATH]\n", stderr)
}

@main
struct Driver {
  static func main() {
    do {
      try run()
    } catch {
      fputs("Error: \(error)\n", stderr)
      exit(1)
    }
  }
}

func run() throws {
    var dumpPath: String? = nil
    var readPath: String? = nil
    var i = 1
    let args = CommandLine.arguments
    while i < args.count {
        switch args[i] {
        case "--dump" where i + 1 < args.count:
            dumpPath = args[i+1]; i += 2
        case "--read" where i + 1 < args.count:
            readPath = args[i+1]; i += 2
        default:
            usage(); return
        }
    }

    let expected = buildExpected()
    if let rp = readPath {
        let data = try Data(contentsOf: URL(fileURLWithPath: rp))
        let actual = try decodeAll(data, expected.count)
        compareExpectedActual(expected, actual)
        var r = G.Reader(data)
        for _ in 0..<expected.count { _ = try G.read_Root(&r) }
        demand(r.pos == data.count, "buffer fully consumed")
        print("Verified dump ok (\(expected.count) messages)")
    }

    let data = try encodeAll(expected)
    let actual = try decodeAll(data, expected.count)
    compareExpectedActual(expected, actual)
    print("All checks passed (\(expected.count) messages)")
    if let dp = dumpPath {
        try data.write(to: URL(fileURLWithPath: dp))
        print("Wrote \(data.count) bytes to \(dp)")
    }
}

extends SceneTree

const Jaw = preload("res://example.gd")
const Recursive = preload("res://any.gd")
const Edges = preload("res://edges.gd")

var failed := false


func check(condition: bool, label: String) -> void:
    if not condition:
        push_error(label)
        failed = true


func check_error(result: Jaw.JawResult, code: Error, offset: int, label: String) -> void:
    check(result.error == code and result.offset == offset and not result.message.is_empty(), label)


func check_golden() -> void:
    var hex := FileAccess.get_file_as_string("res://golden.hex").replace("\n", "").replace("\r", "").replace(" ", "")
    var wire := hex.hex_decode()
    var cursor := 0
    var sizes := [18, 15, 6, 25, 18, 51, 123]
    var names := ["pod-one", "", "void", "floats", "pod-two", "seq-long", "complex"]
    var tags := [1, 2, 3, 4, 1, 4, 5]
    for index in range(sizes.size()):
        var frame := wire.slice(cursor, cursor + sizes[index])
        var decoded := Jaw.Jaw_Root.decode(frame)
        if decoded.error != OK:
            check(false, "golden decode at index %d: %s" % [index, decoded.message])
            return
        var packet: Jaw.Jaw_Root = decoded.value
        check(packet.name.values.get_string_from_utf8() == names[index], "golden name")
        check(packet.var_.tag == tags[index], "golden tag")
        var encoded := packet.encode()
        check(encoded.error == OK and encoded.value == frame, "golden re-encode")
        cursor += sizes[index]
    check(cursor == wire.size(), "golden fixture fully consumed")

    var packet := Jaw.Jaw_Root.new()
    packet.name = Jaw.Jaw_ShortString.new("pod-one".to_utf8_buffer())
    var pod := Jaw.Jaw_MyPOD.new()
    pod.a_thing = 10
    pod.b_thing = 20
    packet.var_ = Jaw.Jaw_MyVariant.make_Pod(pod)
    check(packet.encode().value == wire.slice(0, sizes[0]), "golden encode from fresh values")

    check_error(Jaw.Jaw_Root.decode(PackedByteArray([0, 255])), ERR_INVALID_DATA, 1, "unknown tag")
    check_error(Jaw.Jaw_Root.decode(PackedByteArray([0, 3, 99])), ERR_INVALID_DATA, 2, "trailing bytes")
    check_error(Jaw.Jaw_Root.decode(PackedByteArray([5, 0])), ERR_FILE_EOF, 1, "truncated byte array")


func check_recursive() -> void:
    var items: Array[Recursive.Jaw_Any] = [Recursive.Jaw_Any.make_Integer(-42), Recursive.Jaw_Any.make_Null()]
    var message := Recursive.Jaw_Any.make_Array(Recursive.Jaw_AnyArray.new(items))
    var encoded := message.encode()
    check(encoded.error == OK and encoded.value.hex_encode() == "050200000002d6ffffffffffffff00", "recursive encode")
    var decoded := Recursive.Jaw_Any.decode(encoded.value)
    check(decoded.error == OK and decoded.value.payload.values[0].payload == -42, "recursive decode")
    var too_deep := Recursive.Jaw_Any.make_Null()
    var data := PackedByteArray()
    for index in range(128):
        too_deep = Recursive.Jaw_Any.make_Array(Recursive.Jaw_AnyArray.new([too_deep]))
        data.append_array(PackedByteArray([5, 1, 0, 0, 0]))
    data.append(0)
    check(too_deep.encode().error == ERR_INVALID_DATA, "encode nesting limit")
    check(Recursive.Jaw_Any.decode(data).error == ERR_INVALID_DATA, "decode nesting limit")


func check_edges() -> void:
    var scalars := Edges.Jaw_Scalars.new()
    scalars.small = -2
    scalars.medium = -32768
    scalars.large = -2147483648
    scalars.signed_value = Edges.Jaw_MIN_I64
    scalars.unsigned_value = Edges.Jaw_MAX_ID
    scalars.ratio = -2.0
    scalars.valid = Edges.Jaw_ENABLED
    var encoded := scalars.encode()
    check(encoded.error == OK and encoded.value.hex_encode() == "fe0080000000800000000000000080ffffffffffffffff00000000000000c001", "signed, float and bool wire values")
    var decoded := Edges.Jaw_Scalars.decode(encoded.value)
    check(decoded.error == OK and decoded.value.signed_value == Edges.Jaw_MIN_I64 and decoded.value.valid, "signed and bool decode")
    var bad_bool: PackedByteArray = encoded.value.duplicate()
    bad_bool[bad_bool.size() - 1] = 2
    check(Edges.Jaw_Scalars.decode(bad_bool).error == ERR_INVALID_DATA and Edges.Jaw_Scalars.decode(bad_bool).offset == bad_bool.size() - 1, "invalid bool")
    scalars.small = 128
    check(scalars.encode().error == ERR_INVALID_DATA and scalars.encode().offset == 0, "integer overflow")

    var packed_data := PackedByteArray([3, 1, 4])
    var packed := Edges.Jaw_Packed.decode(packed_data)
    check(packed.error == OK and packed.value.encode().value == packed_data, "preserve packed boolean bytes")
    check(packed.value.valid.get_value().error == ERR_INVALID_DATA and packed.value.pair.values[0].get_value().value, "validate packed bool on access")

    var flags := Edges.Jaw_Flags.new(Edges.Jaw_TOP_BIT)
    flags.mode = 7
    check(flags.set_ == 1 and flags.encode().value.hex_encode() == "0700000000000080", "high-bit mask")
    var full := Edges.Jaw_Full.new()
    full.all_bits = Edges.Jaw_MAX_ID
    check(full.all_bits == -1 and full.encode().value.hex_encode() == "ffffffffffffffff", "full-width mask")
    var high_tag := Edges.Jaw_Choice.decode("0000000000000080".hex_decode())
    check(high_tag.error == OK and high_tag.value.tag == Edges.Jaw_TOP_BIT, "high-bit variant tag")
    var top := Edges.Jaw_Choice.make_Top()
    check(top.encode().value.hex_encode() == "0000000000000080", "high-bit variant encode")
    var max_tag := Edges.Jaw_Choice.make_Max(4294967295)
    check(max_tag.encode().value.hex_encode() == "ffffffffffffffffffffffff", "maximum u64 tag")
    max_tag.payload = "wrong"
    check(max_tag.encode().error == ERR_INVALID_DATA, "wrong variant payload type")

    var unknown := Edges.Jaw_Kind.decode("0200000000000000".hex_decode())
    check(unknown.error == OK and unknown.value.member_name().is_empty() and unknown.value.encode().value[0] == 2, "preserve unknown enum")
    check(Edges.Jaw_Kind.new(Edges.Jaw_Kind.VALUE_A).member_name() == "A", "known enum")
    check(Edges.Jaw_Four.new(PackedByteArray([1])).encode().error == ERR_INVALID_DATA, "fixed length")
    check(Edges.Jaw_Bytes.decode("ffffffffffffffff".hex_decode()).error == ERR_INVALID_DATA, "u64 count exceeds capacity")
    check(Edges.Jaw_Words.decode("03010002000300".hex_decode()).error == ERR_INVALID_DATA, "byte guard for wide elements")
    check(Edges.Jaw_EmptyList.decode("05000000".hex_decode()).error == ERR_INVALID_DATA, "guard for zero-sized elements")
    var empty_list := Edges.Jaw_EmptyList.decode("02000000".hex_decode())
    check(empty_list.error == OK and empty_list.value.values.size() == 2, "zero-sized element decode")
    check(Edges.Jaw_Empty.new().encode().value.is_empty() and Edges.Jaw_SCALE == 1.25, "empty sequence and float constant")
    var named := Edges.Jaw_Node.decode("01020304".hex_decode())
    check(named.error == OK and named.value.encode().value.hex_encode() == "01020304", "escaped identifier collisions")


func _initialize() -> void:
    check_golden()
    check_recursive()
    check_edges()
    if not failed:
        print("JAW GDScript checks passed")
    quit(1 if failed else 0)

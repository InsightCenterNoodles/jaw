#!/usr/bin/env python3
"""
Python driver for basic.jaw generated module.

Builds the same test dataset as the C++ driver and supports:
 - local round-trip encode/decode verification
 - optionally dumping the encoded bytes (--dump PATH)
 - optionally reading a dump and verifying content (--read PATH)
"""

import argparse
import ctypes
from typing import List

import basic


def build_expected() -> List[basic.Root]:
    expected: List[basic.Root] = []

    # 1) Variant = MyPOD
    r1 = basic.Root()
    r1.name = b"pod-one"
    pod = basic.MyPOD()
    pod.a_thing = 10
    pod.b_thing = 20
    r1.var = basic.MyVariant(tag=0, value=pod)
    expected.append(r1)

    # 2) Variant = MyOtherPOD
    r2 = basic.Root()
    r2.name = b""
    first = basic.MyPOD()
    first.a_thing = 1
    first.b_thing = 0x1122334455667788
    second = (ctypes.c_uint8 * 4)(1, 2, 3, 4)
    other = basic.MyOtherPOD()
    other.first = first
    other.second = second
    r2.var = basic.MyVariant(tag=1, value=other)
    expected.append(r2)

    # 3) Variant = void
    r3 = basic.Root()
    r3.name = b"void"
    r3.var = basic.MyVariant(tag=2, value=None)
    expected.append(r3)

    # 4) Variant = SmallSeq with floats
    r3 = basic.Root()
    r3.name = b"floats"
    seq = basic.SmallSeq()
    seq.list = [1.0, 2.5, -3.25, 0.0]
    r3.var = basic.MyVariant(tag=3, value=seq)
    expected.append(r3)

    # 5) Another MyPOD with larger values
    r4 = basic.Root()
    r4.name = b"pod-two"
    pod2 = basic.MyPOD()
    pod2.a_thing = 255
    pod2.b_thing = 0xCAFEBABECAFED00D
    r4.var = basic.MyVariant(tag=0, value=pod2)
    expected.append(r4)

    # 6) Another SmallSeq, longer
    r5 = basic.Root()
    r5.name = b"seq-long"
    seq2 = basic.SmallSeq()
    seq2.list = [i * 0.5 for i in range(10)]
    r5.var = basic.MyVariant(tag=3, value=seq2)
    expected.append(r5)

    return expected


def encode_all(msgs: List[basic.Root]) -> bytes:
    w = basic.Writer()
    for r in msgs:
        basic.write_Root(w, r)
    return w.getvalue()


def decode_all(buf: bytes, count: int) -> List[basic.Root]:
    r = basic.Reader(buf)
    out: List[basic.Root] = []
    for _ in range(count):
        out.append(basic.read_Root(r))
    return out


def demand(cond: bool, label: str) -> None:
    if not cond:
        raise AssertionError(label)


def compare_expected_actual(expected: List[basic.Root], actual: List[basic.Root]) -> None:
    demand(len(expected) == len(actual), "size matches")
    for i, (e, a) in enumerate(zip(expected, actual)):
        demand(a.name == e.name, f"name matches at {i}")
        demand(int(a.var.tag) == int(e.var.tag), f"variant tag matches at {i}")
        tag = int(e.var.tag)
        if tag == 0:
            ep = e.var.value
            ap = a.var.value
            demand(int(ap.a_thing) == int(ep.a_thing), f"MyPOD.a_thing at {i}")
            demand(int(ap.b_thing) == int(ep.b_thing), f"MyPOD.b_thing at {i}")
        elif tag == 1:
            eo = e.var.value
            ao = a.var.value
            demand(int(ao.first.a_thing) == int(eo.first.a_thing), f"MyOtherPOD.first.a_thing at {i}")
            demand(int(ao.first.b_thing) == int(eo.first.b_thing), f"MyOtherPOD.first.b_thing at {i}")
            for j in range(4):
                demand(int(ao.second[j]) == int(eo.second[j]), f"MyOtherPOD.second[{j}] at {i}")
        elif tag == 2:
            demand(a.var.value is None, f"void at {i}")
        elif tag == 3:
            es = e.var.value
            aseq = a.var.value
            demand(len(aseq.list) == len(es.list), f"SmallSeq.size at {i}")
            for j, (x, y) in enumerate(zip(aseq.list, es.list)):
                demand(x == y, f"SmallSeq.value[{j}] at {i}")
        else:
            demand(False, f"unexpected tag {tag} at {i}")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dump", metavar="PATH", help="Path to write the encoded binary stream")
    ap.add_argument("--read", metavar="PATH", help="Path to read a previously dumped binary stream")
    args = ap.parse_args()

    expected = build_expected()
    # If --read is provided, verify that dump
    if args.read:
        with open(args.read, "rb") as f:
            data = f.read()
        actual = decode_all(data, len(expected))
        compare_expected_actual(expected, actual)
        # ensure fully consumed
        r = basic.Reader(data)
        for _ in range(len(expected)):
            _ = basic.read_Root(r)
        demand(r._pos == len(r._buf), "buffer fully consumed")
        print(f"Verified dump ok ({len(expected)} messages)")

    # Always run local round-trip, and optionally dump it
    data = encode_all(expected)
    actual = decode_all(data, len(expected))
    compare_expected_actual(expected, actual)
    print(f"All checks passed ({len(expected)} messages)")
    if args.dump:
        with open(args.dump, "wb") as f:
            f.write(data)
        print(f"Wrote {len(data)} bytes to {args.dump}")


if __name__ == "__main__":
    main()

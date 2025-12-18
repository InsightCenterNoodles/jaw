#!/usr/bin/env python3
"""
Python driver for example.jaw generated module.

Builds the same test dataset as the C++ driver and supports:
 - local round-trip encode/decode verification
 - optionally dumping the encoded bytes (--dump PATH)
 - optionally reading a dump and verifying content (--read PATH)
"""

import argparse
import array
from typing import List

import example


def build_expected() -> List[example.Root]:
    expected: List[example.Root] = []

    # 1) Variant = MyPOD
    pod = example.MyPOD(10, 20)
    expected.append(
        example.Root(
            array.array("B", b"pod-one"), 
            example.MyVariant.make_MyPOD_1(pod)
        )
    )

    # 2) Variant = MyOtherPOD
    other = example.MyOtherPOD(
        example.MyPOD(1, 0x1122334455667788),
        [1, 2, 3, 4]
    )

    expected.append(
        example.Root(
            array.array("B", b""), 
            example.MyVariant.make_MyOtherPOD_2(other)
        )
    )

    # 3) Variant = void
    expected.append(
        example.Root(
            array.array("B", b"void"), 
            example.MyVariant.make_void_3()
        )
    )

    # 4) Variant = SmallSeq with floats
    seq = example.SmallSeq([1.0, 2.5, -3.25, 0.0])
    expected.append(
        example.Root(
            array.array("B", b"floats"), 
            example.MyVariant.make_SmallSeq_4(seq)
        )
    )

    # 5) Another MyPOD with larger values
    pod2 = example.MyPOD(255, 0xCAFEBABECAFED00D)
    expected.append(
        example.Root(
            array.array("B", b"pod-two"), 
            example.MyVariant.make_MyPOD_1(pod2)
        )
    )

    # 6) Another SmallSeq, longer
    seq2 = example.SmallSeq([i * 0.5 for i in range(10)])
    expected.append(
        example.Root(
            array.array("B", b"seq-long"), 
            example.MyVariant.make_SmallSeq_4(seq2)
        )
    )

    return expected


def encode_all(msgs: List[example.Root]) -> bytes:
    w = example.Writer()
    for r in msgs:
        example.write_Root(w, r)
    return w.getvalue()


def decode_all(buf: bytes, count: int) -> List[example.Root]:
    r = example.Reader(buf)
    out: List[example.Root] = []
    for _ in range(count):
        out.append(example.read_Root(r))
    return out


def demand(cond: bool, label: str) -> None:
    if not cond:
        raise AssertionError(label)


def compare_expected_actual(expected: List[example.Root], actual: List[example.Root]) -> None:
    demand(len(expected) == len(actual), "size matches")
    for i, (e, a) in enumerate(zip(expected, actual)):
        demand(a.name == e.name, f"name matches at {i}: {a.name} == {e.name}")
        demand(int(a.var.tag) == int(e.var.tag), f"variant tag matches at {i}")
        tag = int(e.var.tag)
        if e.var.is_MyPOD_1():
            ep = e.var.value
            ap = a.var.value
            demand(int(ap.a_thing) == int(ep.a_thing), f"MyPOD.a_thing at {i}")
            demand(int(ap.b_thing) == int(ep.b_thing), f"MyPOD.b_thing at {i}")
        elif e.var.is_MyOtherPOD_2():
            eo = e.var.value
            ao = a.var.value
            demand(int(ao.first.a_thing) == int(eo.first.a_thing), f"MyOtherPOD.a_thing at {i}")
            demand(int(ao.first.b_thing) == int(eo.first.b_thing), f"MyOtherPOD.b_thing at {i}")
            for j in range(4):
                demand(int(ao.second[j]) == int(eo.second[j]), f"MyOtherPOD.second[{j}] at {i}")
        elif tag == 3:
            demand(a.var.value is None, f"void at {i}")
        elif e.var.is_SmallSeq_4():
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
        r = example.Reader(data)
        for _ in range(len(expected)):
            _ = example.read_Root(r)
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
